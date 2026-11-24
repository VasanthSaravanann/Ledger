//! The pilot's data directory.
//!
//! One directory holds the approved copy, the requisitions, the captured
//! dispositions, the ATS roster, the two logs, and the review inputs. Keeping the
//! files named and flat makes the audit trail readable without this tool.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::{Error, Result};
use crate::jsonl::AppendLog;

/// File holding the Tier 0 copy library.
pub const GATE_LIBRARY: &str = "gate_library.json";
/// File holding every requisition.
pub const REQUISITIONS: &str = "requisitions.json";
/// File holding the three master email templates.
pub const TEMPLATES: &str = "templates.json";
/// Append-only log of captured dispositions.
pub const DISPOSITIONS: &str = "dispositions.jsonl";
/// Append-only hash-chained log of dispatched emails.
pub const SEND_LOG: &str = "send_log.jsonl";
/// Append-only log of sends blocked by the gate-blocking protocol.
pub const BLOCKED_LOG: &str = "blocked.jsonl";
/// Append-only log of candidate replies.
pub const REPLIES: &str = "replies.jsonl";
/// Append-only log of dispositions captured through the form.
pub const QA_LOG: &str = "qa.jsonl";
/// Append-only log of candidate-value signals.
pub const SIGNALS: &str = "signals.jsonl";
/// File holding the five reply-playbook scripts.
pub const PLAYBOOK: &str = "playbook.json";
/// File holding the criteria-feasibility assessments.
pub const REVIEW: &str = "review.json";
/// File holding the plan-task sign-off sheet.
pub const CHECKLIST: &str = "checklist.json";
/// File holding the go/no-go thresholds.
pub const POLICY: &str = "policy.json";
/// File holding the deterministic fixture's summary.
pub const DEMO_SUMMARY: &str = "demo-summary.json";

/// The pilot workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// Opens (and creates) the workspace directory.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] when the directory cannot be created.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root).map_err(|source| Error::Io {
            path: root.clone(),
            source,
        })?;
        Ok(Self { root })
    }

    /// The workspace directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The absolute path of a named file in the workspace.
    pub fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    /// Loads a JSON document, or `None` when it does not exist yet.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] when the file exists but cannot be read, and
    /// [`Error::Json`] when it is not valid JSON.
    pub fn load<T: DeserializeOwned>(&self, name: &str) -> Result<Option<T>> {
        let path = self.path(name);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(Error::Io { path, source }),
        };
        Ok(Some(serde_json::from_str(&text)?))
    }

    /// Writes a JSON document atomically (temp file, then rename).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] when the file cannot be written and
    /// [`Error::Json`] when the document cannot be serialized.
    pub fn save<T: Serialize>(&self, name: &str, value: &T) -> Result<()> {
        let path = self.path(name);
        let temp = path.with_extension("json.tmp");
        let mut text = serde_json::to_string_pretty(value)?;
        text.push('\n');
        fs::write(&temp, text).map_err(|source| Error::Io {
            path: temp.clone(),
            source,
        })?;
        fs::rename(&temp, &path).map_err(|source| Error::Io { path, source })
    }

    /// Opens an append-only log in the workspace.
    pub fn log<T>(&self, name: &str) -> AppendLog<T> {
        AppendLog::open(self.path(name))
    }

    /// The per-requisition ATS roster file name.
    pub fn roster_file(requisition: &crate::id::RequisitionId) -> String {
        format!("roster-{}.json", requisition.as_str())
    }
}
