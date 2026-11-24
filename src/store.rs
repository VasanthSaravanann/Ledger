//! The pilot's data directory.
//!
//! One directory holds the approved copy, the requisitions, the captured
//! dispositions, the ATS roster, the two logs, and the review inputs. Keeping the
//! files named and flat makes the audit trail readable without this tool.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use chrono::Utc;
use fs2::FileExt;
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
/// Append-only operator action log.
pub const OPERATOR: &str = "operator.jsonl";
/// The operator registry (public keys and roles).
pub const OPERATORS: &str = "operators.json";
/// Saved ATS column-mapping profiles.
pub const MAPPINGS: &str = "mappings.json";
/// Append-only delivery confirmations (queued draft → sent).
pub const DELIVERY: &str = "delivery.jsonl";
/// The advisory lock file guarding the workspace.
pub const LOCK: &str = ".lock";

/// The pilot workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    root: PathBuf,
}

/// Creates any parent directories with owner-only permissions.
pub fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    restrict(path, 0o700)
}

/// Writes a secret with `0600`, creating the file if needed.
pub fn write_secret(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut file = File::create(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    restrict(path, 0o600)?;
    file.write_all(bytes).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    file.sync_all().map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Sets file permissions to `0600` (or the mode given) on Unix; no-op elsewhere.
///
/// Declared platforms are macOS and Linux, so the Unix path is the real one.
pub fn restrict(path: &Path, mode: u32) -> Result<()> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
        Ok(())
    }
}

/// Holds the workspace lock until dropped.
#[derive(Debug)]
pub struct WorkspaceLock {
    file: File,
    path: PathBuf,
}

impl WorkspaceLock {
    /// The lock file's path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for WorkspaceLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
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
        // The workspace holds candidate PII: nobody else on the machine may read it.
        restrict(&root, 0o700)?;
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

    /// Writes a JSON document atomically: temp file, fsync, rename, fsync dir.
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

        let mut file = File::create(&temp).map_err(|source| Error::Io {
            path: temp.clone(),
            source,
        })?;
        restrict(&temp, 0o600)?;
        file.write_all(text.as_bytes())
            .map_err(|source| Error::Io {
                path: temp.clone(),
                source,
            })?;
        file.sync_all().map_err(|source| Error::Io {
            path: temp.clone(),
            source,
        })?;
        drop(file);

        fs::rename(&temp, &path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        crate::jsonl::sync_dir(&self.root)
    }

    /// Takes the workspace's exclusive advisory lock.
    ///
    /// The lock is held by the OS, so a crashed process releases it automatically;
    /// a leftover `.lock` file is therefore never stale in the blocking sense, and
    /// the PID/timestamp written inside is diagnostic only.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Locked`] when another process holds the lock, and
    /// [`Error::Io`] when the lock file cannot be written.
    pub fn lock(&self) -> Result<WorkspaceLock> {
        let path = self.path(LOCK);
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;

        if let Err(error) = file.try_lock_exclusive() {
            if error.kind() == std::io::ErrorKind::WouldBlock {
                let mut holder = String::new();
                let _ = file.read_to_string(&mut holder);
                let holder = holder.trim().to_owned();
                return Err(Error::Locked {
                    holder: if holder.is_empty() {
                        "another ledger process".to_owned()
                    } else {
                        holder
                    },
                });
            }
            return Err(Error::Io {
                path: path.clone(),
                source: error,
            });
        }

        file.set_len(0).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        let stamp = format!(
            "pid={} at={}\n",
            std::process::id(),
            Utc::now().to_rfc3339()
        );
        file.write_all(stamp.as_bytes())
            .map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;
        file.sync_all().map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;

        Ok(WorkspaceLock { file, path })
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
