use bio::io::{fasta, fastq};
use std::fs::File;
use std::io::{BufReader, Read, Write};
use flate2::read::MultiGzDecoder;

pub trait Record {
    fn id(&self) -> &str;
    fn seq(&self) -> &[u8];
    fn check(&self) -> Result<(), &str>;
}


pub fn read_gz<P: AsRef<std::path::Path>>(path: P) -> Box<dyn Read> {
    let file = File::open(&path).expect("failed to open input file");
    let buf = BufReader::new(file);
    let path_str = path.as_ref().to_string_lossy();

    if path_str.ends_with(".gz") {
        Box::new(MultiGzDecoder::new(buf))
    } else {
        Box::new(buf)
    }
}

impl Record for fasta::Record {
    fn id(&self) -> &str {
        self.id()
    }

    fn seq(&self) -> &[u8] {
        self.seq()
    }

    fn check(&self) -> Result<(), &str> {
        self.check()
    }
}

impl Record for fastq::Record {
    fn id(&self) -> &str {
        self.id()
    }

    fn seq(&self) -> &[u8] {
        self.seq()
    }

    fn check(&self) -> Result<(), &str> {
        self.check()
    }
}

pub trait Writer<T: Record> {
    fn write_record(&mut self, record: &T) -> Result<(), std::io::Error>;
}

impl<T: Write> Writer<fasta::Record> for fasta::Writer<T> {
    fn write_record(&mut self, record: &fasta::Record) -> Result<(), std::io::Error> {
        self.write_record(&record)
    }
}

impl<T: Write> Writer<fastq::Record> for fastq::Writer<T> {
    fn write_record(&mut self, record: &fastq::Record) -> Result<(), std::io::Error> {
        self.write_record(&record)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum FastxType {
    Fastq,
    Fasta,
    Invalid,
}

pub fn fastx_type<P: AsRef<std::path::Path>>(path: P) -> Result<FastxType, std::io::Error> {
    let reader: Box<dyn Read> = read_gz(&path);
    let mut buf_reader = BufReader::new(reader);
    let mut byte = [0u8; 1];
    buf_reader.read_exact(&mut byte)?;

    match byte[0] as char {
        '>' => Ok(FastxType::Fasta),
        '@' => Ok(FastxType::Fastq),
        _ => Ok(FastxType::Invalid),
    }
}


impl std::fmt::Display for FastxType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            FastxType::Fasta => "fasta",
            FastxType::Fastq => "fastq",
            FastxType::Invalid => "invalid",
        };
        write!(f, "{}", s)
    }
}
