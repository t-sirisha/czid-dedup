use core::hash::Hash;
use core::hash::Hasher;
use std::cmp;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fs::File;
use std::io;
use bio::alphabets::dna::revcomp;

use super::fastx;
use super::paired::PairedRecord;

pub struct Cluster {
    id: String,
    size: u64,
}

pub struct Clusters<T: io::Write> {
    cluster_map: HashMap<u64, Cluster>,
    cluster_order: Vec<u64>,
    cluster_csv_writer: Option<csv::Writer<T>>,
    total_records: u64,
    prefix_length_opt: Option<usize>,
}

impl<T: std::io::Write> Clusters<T> {
    fn insert_record(&mut self, seq_hash: u64, id: String, is_revcomp: bool) -> Result<bool, csv::Error> {
        self.total_records += 1;
        match self.cluster_map.get_mut(&seq_hash) {
            Some(cluster) => {
                cluster.size += 1;
                self.cluster_csv_writer
                    .as_mut()
                    .map(|cluster_csv_writer| {
                        let id_entry = if is_revcomp {
                            format!("{} (rc)", id) // Mark revcomp sequences
                        } else {
                            id.clone()
                        };
                        cluster_csv_writer
                            .write_record(vec![&cluster.id, &id_entry])
                            .map(|_| false)
                    })
                    .unwrap_or(Ok(false))
            }
            None => {
                let res_opt = self.cluster_csv_writer.as_mut().map(|cluster_csv_writer| {
                    cluster_csv_writer
                        .write_record(vec![&id, &id])
                        .map(|_| true)
                });
                self.cluster_map.insert(seq_hash, Cluster { id, size: 1 });
                self.cluster_order.push(seq_hash);
                res_opt.unwrap_or(Ok(true))
            }
        }
    }


    fn get_prefix<'a, 'b>(&'a self, seq: &'b [u8]) -> &'b [u8] {
        let seq_length = seq.len();
        let prefix_length = self
            .prefix_length_opt
            .map(|prefix_length| cmp::min(prefix_length, seq_length))
            .unwrap_or(seq_length);
        &seq[..prefix_length]
    }

    pub fn insert_single<R: fastx::Record>(&mut self, record: &R, use_revcomp: bool) -> Result<bool, csv::Error> {
        let seq = record.seq();
        let rev_seq;
    
        // determine the canonical sequence (either original or reverse complement)
        let (canonical_seq, is_revcomp) = if use_revcomp {
            rev_seq = revcomp(seq);
            if seq <= rev_seq.as_slice() { 
                (seq, false) // Original sequence is canonical
            } else {
                (rev_seq.as_slice(), true) // Reverse complement is canonical
            }
        } else {
            (seq, false) // Use original sequence
        };
    
        // Compute hash for the canonical sequence
        let mut seq_hasher = DefaultHasher::new();
        Hash::hash_slice(self.get_prefix(canonical_seq), &mut seq_hasher);
        let seq_hash = seq_hasher.finish();
    
        // Ensure `insert_record()` supports `is_revcomp`
        self.insert_record(seq_hash, record.id().to_owned(), is_revcomp)
    }

    pub fn insert_pair<R: fastx::Record>(
        &mut self,
        record: &PairedRecord<R>,
        use_revcomp: bool,
    ) -> Result<bool, csv::Error> {
        let r1_seq = record.r1().seq();
        let r2_seq = record.r2().seq();
        
        let r1_revcomp;
        let r2_revcomp;
        
        // Reverse complement sequences only if use_revcomp is set
        let (r1_canon, r2_canon, is_revcomp) = if use_revcomp {
            r1_revcomp = revcomp(r1_seq);
            r2_revcomp = revcomp(r2_seq);
    
            // Choose the lexicographically smaller pair (canonical)
            if (r1_seq, r2_seq) < (r1_revcomp.as_slice(), r2_revcomp.as_slice()) {
                (r1_revcomp.as_slice(), r2_revcomp.as_slice(), true) // Reverse complement pair is canonical
            } else {
                (r1_seq, r2_seq, false) // Original sequences are canonical
            }
        } else {
            (r1_seq, r2_seq, false) // Use original sequences
        };
    
        let mut seq_hasher = DefaultHasher::new();
        Hash::hash_slice(self.get_prefix(r1_canon), &mut seq_hasher);
        Hash::hash(&0, &mut seq_hasher);
        Hash::hash_slice(self.get_prefix(r2_canon), &mut seq_hasher);
        let seq_hash = seq_hasher.finish();
        
        self.insert_record(seq_hash, record.id().to_owned(), is_revcomp)
    }

    pub fn unique_records(&self) -> u64 {
        self.cluster_map.len() as u64
    }

    pub fn duplicate_records(&self) -> u64 {
        self.total_records - self.unique_records()
    }

    pub fn total_records(&self) -> u64 {
        self.total_records
    }

    pub fn write_sizes<R: std::io::Write>(
        &self,
        csv_writer: &mut csv::Writer<R>,
    ) -> Result<(), csv::Error> {
        csv_writer.write_record(vec!["representative read id", "cluster size"])?;
        for cluster_hash in self.cluster_order.iter() {
            // guaranteed to be present
            let cluster = self.cluster_map.get(cluster_hash).unwrap();
            csv_writer.write_record(vec![&cluster.id, &cluster.size.to_string()])?;
        }
        Ok(())
    }

    pub fn from_writer(
        cluster_output_opt: Option<T>,
        prefix_length_opt: Option<usize>,
        capacity: usize,
    ) -> Result<Self, csv::Error> {
        let cluster_csv_writer_opt = cluster_output_opt.map(csv::Writer::from_writer);
        let cluster_map = HashMap::with_capacity(capacity);
        let cluster_order = Vec::with_capacity(capacity);
        let cluster_csv_writer = cluster_csv_writer_opt
            .map(|mut cluster_csv_writer| {
                cluster_csv_writer
                    .write_record(vec!["representative read id", "read id"])
                    .map(|_| Some(cluster_csv_writer))
            })
            .unwrap_or(Ok(None))?;
        Ok(Clusters {
            cluster_map,
            cluster_order,
            cluster_csv_writer,
            total_records: 0,
            prefix_length_opt,
        })
    }
    }

impl Clusters<File> {
    pub fn from_file<P: AsRef<std::path::Path>>(
        cluster_output_path_opt: Option<P>,
        prefix_length_opt: Option<usize>,
        capacity: usize,
    ) -> Result<Self, csv::Error> {
        cluster_output_path_opt
            .map(|cluster_output_path| {
                File::create(cluster_output_path).map(|cluster_output| Some(cluster_output))
            })
            .unwrap_or(Ok(None))
            .map_err(csv::Error::from)
            .and_then(|cluster_output| {
                Clusters::from_writer(cluster_output, prefix_length_opt, capacity)
            })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use bio::io::fasta;
    use rand::Rng;
    use std::convert::TryFrom;
    use std::io::Cursor;
    use std::str;

    fn random_seq(len: usize) -> Vec<u8> {
        const CHARSET: &[u8] = b"ACTG";
        let mut rng = rand::thread_rng();
        (0..len)
            .map(|_| {
                let idx = rng.gen_range(0, CHARSET.len());
                CHARSET[idx]
            })
            .collect()
    }

    #[test]
    fn test_insert_single() {
        let mut cluster_output = Cursor::new(Vec::new());
        {
            let mut clusters =
                Clusters::from_writer(Some(&mut cluster_output), Some(10), 200).expect("asdasd");
            let seq = random_seq(20);
            let record_1 = fasta::Record::with_attrs("id_a", None, &seq);
            clusters.insert_single(&record_1).expect("don't break");
            let record_2 = fasta::Record::with_attrs("id_b", None, &seq);
            clusters.insert_single(&record_2).expect("don't break");
            assert_eq!(clusters.duplicate_records(), 1);
            assert_eq!(clusters.unique_records(), 1);
            assert_eq!(clusters.total_records(), 2);
        }
        assert_eq!(
            str::from_utf8(cluster_output.into_inner().as_slice()).unwrap(),
            "representative read id,read id\nid_a,id_a\nid_a,id_b\n"
        );
    }

    #[test]
    fn test_insert_pair() {
        let mut cluster_output = Cursor::new(Vec::new());
        {
            let mut clusters =
                Clusters::from_writer(Some(&mut cluster_output), Some(10), 200).expect("asdasd");
            let seq_r1 = random_seq(20);
            let seq_r2 = random_seq(20);
            let record_1_r1 = fasta::Record::with_attrs("id_a", None, &seq_r1);
            let record_1_r2 = fasta::Record::with_attrs("id_a", None, &seq_r2);
            clusters
                .insert_pair(&PairedRecord::try_from((record_1_r1, record_1_r2)).unwrap())
                .expect("don't break");
            let record_2_r1 = fasta::Record::with_attrs("id_b", None, &seq_r1);
            let record_2_r2 = fasta::Record::with_attrs("id_b", None, &seq_r2);
            clusters
                .insert_pair(&PairedRecord::try_from((record_2_r1, record_2_r2)).unwrap())
                .expect("don't break");
            assert_eq!(clusters.duplicate_records(), 1);
            assert_eq!(clusters.unique_records(), 1);
            assert_eq!(clusters.total_records(), 2);
        }
        assert_eq!(
            str::from_utf8(cluster_output.into_inner().as_slice()).unwrap(),
            "representative read id,read id\nid_a,id_a\nid_a,id_b\n"
        );
    }

    #[test]
    fn test_write_cluster_sizes() {
        let mut cluster_output = Cursor::new(Vec::new());
        let mut cluster_sizes_writer = Cursor::new(Vec::new());
        {
            let mut cluster_sizes_output = csv::Writer::from_writer(&mut cluster_sizes_writer);
            let mut clusters =
                Clusters::from_writer(Some(&mut cluster_output), Some(10), 200).expect("asdasd");
            let seq1 = random_seq(20);
            let record_1 = fasta::Record::with_attrs("id_a", None, &seq1);
            clusters.insert_single(&record_1).expect("don't break");
            let record_2 = fasta::Record::with_attrs("id_b", None, &seq1);
            clusters.insert_single(&record_2).expect("don't break");
            let seq2 = random_seq(20);
            let record_3 = fasta::Record::with_attrs("id_c", None, &seq2);
            clusters.insert_single(&record_3).expect("don't break");
            clusters
                .write_sizes(&mut cluster_sizes_output)
                .expect("don't break");
        }
        let cluster_sizes_output_inner = cluster_sizes_writer.into_inner();
        let cluster_sizes = str::from_utf8(cluster_sizes_output_inner.as_slice()).unwrap();
        assert_eq!(
            cluster_sizes,
            "representative read id,cluster size\nid_a,2\nid_c,1\n"
        );
    }
}
