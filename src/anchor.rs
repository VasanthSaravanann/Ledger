//! Off-host chain-head anchoring (V5 Phase 2.5).
//!
//! A local chain proves internal consistency. Publishing the head hash somewhere
//! else — a receipt file copied off the machine, or a git commit in another
//! repository — is what turns "consistent" into "not rewritten since".
//!
//! Per the supply-chain guarantee, the binary makes **no network calls**. Anchoring
//! writes a receipt and, when asked, hands it to `git`. A webhook belongs in a
//! separate script, not in this crate.

use std::collections::BTreeMap;
use std::process::Command;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::log::SendLog;
use crate::operator::{OperatorLog, OperatorNote};
use crate::store::{self, Store};

/// One published chain-head receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor {
    /// When the head was published.
    pub at: DateTime<Utc>,
    /// How many entries the chain had.
    pub entries: usize,
    /// The chain head, which any later verifier can compare against.
    pub head: String,
    /// Who published it.
    pub operator: String,
    /// Where it went: `receipt` or `git`.
    pub destination: String,
}

/// Publishes the current chain head.
///
/// # Errors
///
/// Returns [`Error::LogCorrupt`] when the chain does not verify, and
/// [`Error::Io`] when the receipt cannot be written.
pub fn anchor(store: &Store, operator: &str, git_commit: bool) -> Result<Anchor> {
    let verification = SendLog::open(store)?.verification()?;
    store::ensure_dir(&store.path("anchors"))?;

    let anchor = Anchor {
        at: Utc::now(),
        entries: verification.entries,
        head: verification.head.clone(),
        operator: operator.to_owned(),
        destination: if git_commit { "git" } else { "receipt" }.to_owned(),
    };

    let receipt = store.path("anchors").join(format!(
        "{}-{}.json",
        anchor.at.format("%Y%m%dT%H%M%SZ"),
        &anchor.head[0..12]
    ));
    let mut text = serde_json::to_string_pretty(&anchor)?;
    text.push('\n');
    std::fs::write(&receipt, text).map_err(|source| Error::Io {
        path: receipt.clone(),
        source,
    })?;
    store::restrict(&receipt, 0o600)?;

    if git_commit {
        // Best effort: a workspace that is not a repository still gets its receipt.
        let status = Command::new("git")
            .arg("-C")
            .arg(store.root())
            .args(["add", "-A", "anchors"])
            .status();
        if status.map(|status| status.success()).unwrap_or(false) {
            let message = format!(
                "anchor {} entries head {}",
                anchor.entries,
                &anchor.head[0..12]
            );
            let _ = Command::new("git")
                .arg("-C")
                .arg(store.root())
                .args(["commit", "-q", "-m", &message, "--", "anchors"])
                .status();
        }
    }

    let mut details = BTreeMap::new();
    details.insert("receipt".to_owned(), receipt.display().to_string());
    details.insert("head".to_owned(), anchor.head.clone());
    details.insert("entries".to_owned(), anchor.entries.to_string());
    OperatorLog::open(store).record(OperatorNote {
        at: anchor.at,
        operator: operator.to_owned(),
        command: "anchor".to_owned(),
        note: "chain head published off the send log".to_owned(),
        details,
    })?;

    Ok(anchor)
}

/// Compares a published head against the live chain.
///
/// # Errors
///
/// Returns [`Error::LogCorrupt`] when the current chain fails verification.
pub fn compare(store: &Store, published_head: &str) -> Result<bool> {
    let verification = SendLog::open(store)?.verification()?;
    Ok(verification.head == published_head)
}

/// A receipt's file name pattern, for documentation and tests.
pub fn receipt_prefix() -> &'static str {
    "anchors/"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn workspace(tag: &str) -> (Store, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "ledger-anchor-{tag}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _ = fs::remove_dir_all(&dir);
        let store = Store::open(&dir).expect("workspace");
        crate::demo::seed(&store, false, true).expect("fixture");
        (store, dir)
    }

    #[test]
    fn anchoring_writes_a_receipt_that_matches_the_chain() {
        let (store, dir) = workspace("receipt");
        let anchor = anchor(&store, "qa-patel", false).expect("anchor");
        assert_eq!(anchor.entries, 147);
        assert!(anchor.head.len() == 64);
        assert!(anchor.destination == "receipt");

        let receipt = store.path("anchors").join(format!(
            "{}-{}.json",
            anchor.at.format("%Y%m%dT%H%M%SZ"),
            &anchor.head[0..12]
        ));
        assert!(receipt.is_file());
        assert!(compare(&store, &anchor.head).expect("compare"));
        assert!(!compare(&store, &"0".repeat(64)).expect("compare"));

        let notes = crate::operator::OperatorLog::open(&store)
            .all()
            .expect("operator log");
        assert!(notes.iter().any(|note| note.command == "anchor"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_tampered_chain_cannot_be_anchored() {
        let (store, dir) = workspace("tampered");
        let path = store.path(store::SEND_LOG);
        let text = fs::read_to_string(&path).expect("read");
        fs::write(&path, text.replacen("Dear ", "Dear X", 1)).expect("write");
        assert!(anchor(&store, "qa-patel", false).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
