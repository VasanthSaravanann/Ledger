//! Operator actions: the audit log and log quarantine (V4 1.4 and 6.4).
//!
//! The candidate send log records what a candidate received. This log records what
//! an operator did to the workspace — starting with quarantine, the only sanctioned
//! way to deal with a torn tail.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::jsonl::{AppendLog, sync_dir};
use crate::store::{self, Store};
use crate::text;

/// One operator action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorNote {
    /// When the action ran.
    pub at: DateTime<Utc>,
    /// Who ran it.
    pub operator: String,
    /// The command, for example `log quarantine`.
    pub command: String,
    /// Why it ran.
    pub note: String,
    /// Action-specific detail, for example the sidecar path.
    #[serde(default)]
    pub details: BTreeMap<String, String>,
}

/// The append-only operator log.
#[derive(Debug)]
pub struct OperatorLog {
    log: AppendLog<OperatorNote>,
}

impl OperatorLog {
    /// Opens the operator log.
    pub fn open(store: &Store) -> Self {
        Self {
            log: store.log(store::OPERATOR),
        }
    }

    /// Appends one note.
    ///
    /// # Errors
    ///
    /// Propagates append errors.
    pub fn record(&mut self, note: OperatorNote) -> Result<()> {
        self.log.append(&note)
    }

    /// Every note, in order.
    ///
    /// # Errors
    ///
    /// Propagates read errors, including [`Error::TornWrite`].
    pub fn all(&self) -> Result<Vec<OperatorNote>> {
        self.log.read_all()
    }
}

/// Resolves the operator identity: `--operator`, then `LEDGER_OPERATOR`, then `USER`.
pub fn identity(explicit: Option<&str>) -> String {
    explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| std::env::var("LEDGER_OPERATOR").ok())
        .or_else(|| std::env::var("USER").ok())
        .unwrap_or_else(|| "unknown".to_owned())
}

/// What quarantine did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuarantineReport {
    /// Where the partial tail was preserved.
    pub sidecar: PathBuf,
    /// How many bytes were moved.
    pub bytes_moved: usize,
    /// How many complete entries remain in the log.
    pub remaining_entries: usize,
    /// Whether the remaining chain verifies.
    pub chain_valid: bool,
}

/// Moves an unterminated trailing tail out of the send log.
///
/// The tail is preserved byte-for-byte in a timestamped sidecar, the log is
/// truncated to its last complete line, and the action is written to
/// `operator.jsonl`. The chain is then re-verified: quarantine can restore a torn
/// append, but it cannot repair a corrupted entry.
///
/// # Errors
///
/// Returns [`Error::Invalid`] when there is nothing to quarantine, and propagates
/// IO errors.
pub fn quarantine(store: &Store, reason: &str, operator: &str) -> Result<QuarantineReport> {
    let reason = text::required("quarantine reason", reason, 500)?;
    let operator = text::required("operator", operator, 120)?;

    let path = store.path(store::SEND_LOG);
    let bytes = fs::read(&path).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    let split = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map(|index| index.saturating_add(1))
        .unwrap_or(0);
    let tail_len = bytes.len().saturating_sub(split);
    if tail_len == 0 {
        return Err(Error::invalid(
            "quarantine",
            "the send log ends with a complete line; nothing to quarantine",
        ));
    }

    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let sidecar = store.path(&format!("send_log.jsonl.{stamp}.quarantine"));
    let tail = bytes.get(split..).unwrap_or_default();
    fs::write(&sidecar, tail).map_err(|source| Error::Io {
        path: sidecar.clone(),
        source,
    })?;
    sync_dir(store.root())?;

    let file = OpenOptions::new()
        .write(true)
        .open(&path)
        .map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
    file.set_len(split as u64).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    file.sync_all().map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    drop(file);
    sync_dir(store.root())?;

    let log = crate::log::SendLog::open(store)?;
    let verification = log.verification()?;

    let mut details = BTreeMap::new();
    details.insert("sidecar".to_owned(), sidecar.display().to_string());
    details.insert("bytes_moved".to_owned(), tail_len.to_string());
    OperatorLog::open(store).record(OperatorNote {
        at: Utc::now(),
        operator,
        command: "log quarantine".to_owned(),
        note: reason,
        details,
    })?;
    sync_dir(store.root())?;

    Ok(QuarantineReport {
        sidecar,
        bytes_moved: tail_len,
        remaining_entries: verification.entries,
        chain_valid: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    fn workspace(tag: &str) -> (Store, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "ledger-quarantine-{tag}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _ = fs::remove_dir_all(&dir);
        (Store::open(&dir).expect("workspace"), dir)
    }

    #[test]
    fn an_intact_log_has_nothing_to_quarantine() {
        let (store, dir) = workspace("intact");
        fs::write(store.path(store::SEND_LOG), b"{\"seq\":0}\n").expect("write");
        assert!(quarantine(&store, "test", "tester").is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_torn_tail_is_preserved_and_the_action_is_logged() {
        let (store, dir) = workspace("torn");
        fs::write(store.path(store::SEND_LOG), b"").expect("write");
        let path = store.path(store::SEND_LOG);
        let mut bytes = fs::read(&path).expect("read");
        bytes.extend_from_slice(b"{\"seq\":");
        fs::write(&path, bytes).expect("write");

        let report = quarantine(&store, "killed mid-append", "qa-patel").expect("quarantine");
        assert_eq!(report.bytes_moved, 7);
        assert!(report.chain_valid);
        assert!(fs::read(&report.sidecar).expect("sidecar") == b"{\"seq\":");
        assert_eq!(fs::read(&path).expect("log").len(), 0);

        let notes = OperatorLog::open(&store).all().expect("operator log");
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].operator, "qa-patel");
        assert_eq!(notes[0].note, "killed mid-append");
        let _ = fs::remove_dir_all(&dir);
    }
}
