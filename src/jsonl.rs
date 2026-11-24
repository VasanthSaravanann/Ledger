//! Append-only JSONL storage.
//!
//! Every record the pilot must not rewrite — captured dispositions, sends, blocks,
//! replies — lands in one of these logs. There is no update or delete method, by
//! design: nothing in this crate can rewrite a captured decision.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
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
    /// Appends one complete, newline-terminated record and fsyncs it.
    ///
    /// The whole line is written with a single `write_all`, then the file and its
    /// parent directory are fsynced, so a crash either publishes the complete line
    /// or leaves a detectable partial one — never a silently truncated record.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] when the directory or file cannot be written, and
    /// [`Error::Json`] when the record cannot be serialized.
    pub fn append(&mut self, record: &T) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            crate::store::ensure_dir(parent)?;
        }
        let line = serde_json::to_string(record)?;
        let mut buffer = Vec::with_capacity(line.len().saturating_add(1));
        buffer.extend_from_slice(line.as_bytes());
        buffer.push(b'\n');

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|source| Error::Io {
                path: self.path.clone(),
                source,
            })?;
        // Candidate PII lives in these lines: owner-only, regardless of umask.
        crate::store::restrict(&self.path, 0o600)?;
        file.write_all(&buffer).map_err(|source| Error::Io {
            path: self.path.clone(),
            source,
        })?;
        file.sync_all().map_err(|source| Error::Io {
            path: self.path.clone(),
            source,
        })?;
        if let Some(parent) = self.path.parent() {
            sync_dir(parent)?;
        }
        Ok(())
    }
}

impl<T: Serialize> AppendLog<T> {
    /// Appends several records in one write and one fsync.
    ///
    /// All-or-nothing at the syscall level: either the whole batch is written or
    /// the write fails. A crash after the write leaves complete lines, and a crash
    /// during it leaves a detectable torn tail.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Json`] when any record cannot be serialized (nothing is
    /// written in that case), and [`Error::Io`] when the write fails.
    pub fn append_batch(&mut self, records: &[T]) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        if let Some(parent) = self.path.parent() {
            crate::store::ensure_dir(parent)?;
        }
        let mut buffer = Vec::new();
        for record in records {
            let line = serde_json::to_string(record)?;
            buffer.extend_from_slice(line.as_bytes());
            buffer.push(b'\n');
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|source| Error::Io {
                path: self.path.clone(),
                source,
            })?;
        crate::store::restrict(&self.path, 0o600)?;
        file.write_all(&buffer).map_err(|source| Error::Io {
            path: self.path.clone(),
            source,
        })?;
        file.sync_all().map_err(|source| Error::Io {
            path: self.path.clone(),
            source,
        })?;
        if let Some(parent) = self.path.parent() {
            sync_dir(parent)?;
        }
        Ok(())
    }
}

impl<T: DeserializeOwned> AppendLog<T> {
    /// Reads every record, in append order. A missing file is an empty log.
    ///
    /// A trailing partial line is a torn write and is reported, never dropped: the
    /// caller has to quarantine it deliberately.
    ///
    /// # Errors
    ///
    /// Returns [`Error::TornWrite`] when the file does not end with a newline,
    /// [`Error::Io`] when it cannot be read, and [`Error::Invalid`] when a line is
    /// not a valid record.
    pub fn read_all(&self) -> Result<Vec<T>> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(Error::Io {
                    path: self.path.clone(),
                    source,
                });
            }
        };
        if let Some(last_newline) = bytes.iter().rposition(|byte| *byte == b'\n') {
            let tail = bytes.len().saturating_sub(last_newline + 1);
            if tail > 0 {
                return Err(Error::TornWrite {
                    path: self.path.clone(),
                    bytes: tail,
                });
            }
        } else if !bytes.is_empty() {
            return Err(Error::TornWrite {
                path: self.path.clone(),
                bytes: bytes.len(),
            });
        }

        let text = String::from_utf8(bytes).map_err(|_| {
            Error::invalid("log", format!("{} is not valid UTF-8", self.path.display()))
        })?;
        let mut records = Vec::new();
        for (index, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let record = serde_json::from_str(line).map_err(|error| {
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

/// fsyncs a directory so a rename or create is durable.
///
/// Declared platforms are macOS and Linux (see README); Windows is unsupported.
pub(crate) fn sync_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        let directory = File::open(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        directory.sync_all().map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

/// Reads a file's bytes, or `None` when it does not exist.
pub fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

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
    fn a_batch_is_one_write_of_complete_lines() {
        let dir = std::env::temp_dir().join(format!("ledger-batch-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("rows.jsonl");
        let mut log = AppendLog::open(&path);
        log.append_batch(&[Row { n: 1 }, Row { n: 2 }, Row { n: 3 }])
            .expect("batch");
        assert_eq!(
            log.read_all().expect("read"),
            vec![Row { n: 1 }, Row { n: 2 }, Row { n: 3 }]
        );
        let text = fs::read_to_string(&path).expect("read");
        assert_eq!(text.matches('\n').count(), 3);
        assert!(text.ends_with('\n'));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_torn_trailing_line_is_detected_not_dropped() {
        let dir = std::env::temp_dir().join(format!("ledger-torn-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("rows.jsonl");
        let mut log = AppendLog::open(&path);
        log.append(&Row { n: 1 }).expect("append");
        let mut bytes = fs::read(&path).expect("read");
        bytes.extend_from_slice(b"{\"n\":2}");
        fs::write(&path, bytes).expect("write");
        assert!(matches!(log.read_all(), Err(Error::TornWrite { .. })));
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
