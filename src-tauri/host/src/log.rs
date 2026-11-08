//! A session's log: every record (its output, its error output, what was written to it, its exit), numbered, on
//! disk in the session's folder, so whatever happened while no Cue was attached can be read back in order.
//!
//! Bounded by size: past `limit` the log becomes `log.1.jsonl` (dropping the one before) and a new one starts, so
//! between `limit` and twice it is kept. A record torn by a crash mid-write is skipped when read.

use crate::proto::Record;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// How much one session's log keeps, at least: 50 MB.
pub const LIMIT: u64 = 50 * 1024 * 1024;

pub struct Log {
    dir: PathBuf,
    file: File,
    size: u64,
    limit: u64,
    /// The first record kept (in the rolled-over file if there is one), the first in the current file, the last.
    first: Option<u64>,
    first_current: Option<u64>,
    last: Option<u64>,
}

impl Log {
    /// The log in `dir` (made if new), appended to from where it ends. A last record torn by a crash is ended
    /// first, so the next record is a line of its own.
    pub fn open(dir: &Path, limit: u64) -> std::io::Result<Log> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join("log.jsonl");
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        let mut size = file.metadata()?.len();
        if size > 0 && !ends_with_newline(&path)? {
            file.write_all(b"\n")?;
            size += 1;
        }
        let (old, current) = (read_file(&dir.join("log.1.jsonl")), read_file(&path));
        let first_current = current.first().map(|r| r.seq);
        let first = old.first().map(|r| r.seq).or(first_current);
        let last = current.last().or(old.last()).map(|r| r.seq);
        Ok(Log { dir: dir.to_path_buf(), file, size, limit, first, first_current, last })
    }

    /// The first record it still has, and the last.
    pub fn first(&self) -> Option<u64> {
        self.first
    }

    pub fn last(&self) -> Option<u64> {
        self.last
    }

    /// One more record. Kept whole: written as one line, then the log rolls over if it's past its limit.
    pub fn append(&mut self, r: &Record) -> std::io::Result<()> {
        let mut line = serde_json::to_string(r).unwrap_or_default();
        line.push('\n');
        self.file.write_all(line.as_bytes())?;
        if self.size == 0 {
            self.first_current = Some(r.seq);
        }
        self.first = self.first.or(Some(r.seq));
        self.last = Some(r.seq);
        self.size += line.len() as u64;
        if self.size > self.limit {
            self.roll()?;
        }
        Ok(())
    }

    fn roll(&mut self) -> std::io::Result<()> {
        std::fs::rename(self.dir.join("log.jsonl"), self.dir.join("log.1.jsonl"))?;
        self.file = OpenOptions::new().create(true).append(true).open(self.dir.join("log.jsonl"))?;
        self.size = 0;
        // What was current is now the older file: the oldest kept starts there.
        self.first = self.first_current.take();
        Ok(())
    }

    /// Every record it still has, oldest first.
    pub fn records(&self) -> Vec<Record> {
        read_all(&self.dir)
    }
}

fn ends_with_newline(path: &Path) -> std::io::Result<bool> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = File::open(path)?;
    f.seek(SeekFrom::End(-1))?;
    let mut last = [0u8; 1];
    f.read_exact(&mut last)?;
    Ok(last[0] == b'\n')
}

/// The records kept in `dir`, oldest first (the rolled-over file, then the current one), torn ones left out.
pub fn read_all(dir: &Path) -> Vec<Record> {
    ["log.1.jsonl", "log.jsonl"].iter().flat_map(|name| read_file(&dir.join(name))).collect()
}

fn read_file(path: &Path) -> Vec<Record> {
    let Ok(f) = File::open(path) else { return vec![] };
    let mut out = vec![];
    let mut reader = BufReader::new(f);
    let mut buf = Vec::new();
    while reader.read_until(b'\n', &mut buf).is_ok_and(|n| n > 0) {
        if let Ok(r) = serde_json::from_slice::<Record>(&buf) {
            out.push(r);
        }
        buf.clear();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::What;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cue-host-log-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn out(seq: u64, s: &str) -> Record {
        Record { seq, what: What::Out(s.into()) }
    }

    #[test]
    fn records_read_back_in_order_and_a_torn_last_one_is_skipped() {
        let d = dir("torn");
        let mut log = Log::open(&d, LIMIT).unwrap();
        log.append(&out(1, "a")).unwrap();
        log.append(&Record { seq: 2, what: What::In("b".into()) }).unwrap();
        log.append(&Record { seq: 3, what: What::Exited(Some(0)) }).unwrap();
        drop(log);
        OpenOptions::new().append(true).open(d.join("log.jsonl")).unwrap().write_all(br#"{"seq":4,"out":"hal"#).unwrap();
        let seqs: Vec<u64> = read_all(&d).iter().map(|r| r.seq).collect();
        assert_eq!(seqs, [1, 2, 3]);
        // Opened again (the host started anew): the next record is a line of its own, not glued to the torn one.
        let mut log = Log::open(&d, LIMIT).unwrap();
        log.append(&out(4, "again")).unwrap();
        assert_eq!(log.records().iter().map(|r| r.seq).collect::<Vec<_>>(), [1, 2, 3, 4]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn past_its_limit_it_keeps_the_last_two_files_worth() {
        let d = dir("roll");
        let mut log = Log::open(&d, 100).unwrap();
        for n in 1..=20 {
            log.append(&out(n, "0123456789")).unwrap();
        }
        let seqs: Vec<u64> = log.records().iter().map(|r| r.seq).collect();
        assert_eq!(seqs.last(), Some(&20));
        assert_eq!((log.first(), log.last()), (seqs.first().copied(), Some(20)), "it knows its ends without reading");
        let again = Log::open(&d, 100).unwrap();
        assert_eq!((again.first(), again.last()), (log.first(), log.last()), "and finds them again when opened");
        assert!(seqs.first() > Some(&1), "the oldest went: {seqs:?}");
        assert!(seqs.windows(2).all(|w| w[1] == w[0] + 1), "what's kept has no holes: {seqs:?}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
