//! Append-only JSONL storage.
//!
//! Every record the pilot must not rewrite — captured dispositions, sends, blocks,
//! replies — lands in one of these logs. There is no update or delete method, by
//! design: nothing in this crate can rewrite a captured decision.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::{Error, Result};

/// A write-only, one-record-per-line log file.
pub struct AppendLog<T> {
    path: PathBuf,
    marker: PhantomData<fn() -> T>,
}

impl<T> fmt::Debug for AppendLog<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AppendLog")
            .field("path", &self.path)
            .finish()
    }
}

impl<T> AppendLog<T> {
    /// Wraps `path`. The file is created on first append.
    pub fn open(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            marker: PhantomData,
        }
    }

    /// The file this log appends to.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl<T: Serialize> AppendLog<T> {
    /// Appends one record and flushes it to disk.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] when the directory or file cannot be written, and
    /// [`Error::Json`] when the record cannot be serialized.
    pub fn append(&mut self, record: &T) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let line = serde_json::to_string(record)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|source| Error::Io {
                path: self.path.clone(),
                source,
            })?;
        writeln!(file, "{line}").map_err(|source| Error::Io {
            path: self.path.clone(),
            source,
        })?;
        file.sync_data().map_err(|source| Error::Io {
            path: self.path.clone(),
            source,
        })
    }
}

impl<T: DeserializeOwned> AppendLog<T> {
    /// Reads every record, in append order. A missing file is an empty log.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] when the file cannot be read and [`Error::Json`]
    /// when a line is not a valid record.
    pub fn read_all(&self) -> Result<Vec<T>> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(Error::Io {
                    path: self.path.clone(),
                    source,
                });
            }
        };
        let mut records = Vec::new();
        for (index, line) in BufReader::new(file).lines().enumerate() {
            let line = line.map_err(|source| Error::Io {
                path: self.path.clone(),
                source,
            })?;
            if line.trim().is_empty() {
                continue;
            }
            let record = serde_json::from_str(&line).map_err(|error| {
                Error::invalid(
                    "log line",
                    format!("{}:{}: {error}", self.path.display(), index + 1),
                )
            })?;
            records.push(record);
        }
        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Row {
        n: u32,
    }

    #[test]
    fn appends_and_reads_in_order() {
        let dir = std::env::temp_dir().join(format!("ledger-log-test-{}", std::process::id()));
        let path = dir.join("rows.jsonl");
        let _ = fs::remove_dir_all(&dir);
        let mut log = AppendLog::open(&path);
        log.append(&Row { n: 1 }).expect("first append");
        log.append(&Row { n: 2 }).expect("second append");
        assert_eq!(
            log.read_all().expect("read back"),
            vec![Row { n: 1 }, Row { n: 2 }]
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_reads_as_empty() {
        let path = std::env::temp_dir().join("ledger-log-does-not-exist.jsonl");
        let _ = fs::remove_file(&path);
        let log: AppendLog<Row> = AppendLog::open(&path);
        assert_eq!(log.read_all().expect("empty log"), Vec::<Row>::new());
    }
}
