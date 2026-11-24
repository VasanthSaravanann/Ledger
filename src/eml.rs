//! RFC 5322 draft export (V5 Phase 4.5).
//!
//! The pilot's dispatch stays manual. What this module does is hand a coordinator
//! files they can open in Outlook or Apple Mail without copy-pasting, and record
//! them honestly: every send written this way is `queued`, not `sent`, and the
//! manifest pins the SHA-256 of each draft. A later edit in a mail client is
//! therefore visible as a mismatch, not silently absorbed into the log.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::digest::sha256_hex;
use crate::error::{Error, Result};
use crate::log::SendEntry;
use crate::store;

/// One draft file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmlFile {
    /// The application the draft is for.
    pub application_id: String,
    /// The send-log entry it belongs to.
    pub send_id: String,
    /// The file name, relative to the batch directory.
    pub file: String,
    /// SHA-256 of the file's exact bytes.
    pub sha256: String,
    /// The digest of the merged text inside it.
    pub text_digest: String,
}

/// A batch of drafts plus its manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmlBatch {
    /// When the batch was written.
    pub at: chrono::DateTime<Utc>,
    /// The requisition.
    pub requisition: String,
    /// The drafts.
    pub files: Vec<EmlFile>,
    /// SHA-256 over the concatenation of the file hashes, for a single comparison.
    pub batch_digest: String,
}

/// Writes one `.eml` per send entry into `directory`.
///
/// # Errors
///
/// Returns [`Error::Io`] when the directory or a file cannot be written.
pub fn write_batch(
    directory: impl AsRef<Path>,
    requisition: &str,
    sender: &str,
    reply_to: &str,
    entries: &[SendEntry],
) -> Result<EmlBatch> {
    let directory = directory.as_ref().to_path_buf();
    store::ensure_dir(&directory)?;

    let mut files = Vec::new();
    for entry in entries {
        let name = format!("{}.eml", entry.application_id.as_str());
        let path = directory.join(&name);
        let message = render_message(entry, sender, reply_to);
        store::write_secret(&path, message.as_bytes())?;
        files.push(EmlFile {
            application_id: entry.application_id.to_string(),
            send_id: entry.send_id.to_string(),
            file: name,
            sha256: sha256_hex(message.as_bytes()),
            text_digest: entry.text_digest.clone(),
        });
    }

    let mut concatenated = String::new();
    for file in &files {
        concatenated.push_str(&file.sha256);
    }
    let batch = EmlBatch {
        at: Utc::now(),
        requisition: requisition.to_owned(),
        batch_digest: sha256_hex(concatenated.as_bytes()),
        files,
    };

    let manifest = directory.join("manifest.json");
    let mut text = serde_json::to_string_pretty(&batch)?;
    text.push('\n');
    store::write_secret(&manifest, text.as_bytes())?;
    Ok(batch)
}

/// Renders one draft, CRLF-terminated as RFC 5322 requires.
fn render_message(entry: &SendEntry, sender: &str, reply_to: &str) -> String {
    let mut headers: Vec<(String, String)> = vec![
        ("From".to_owned(), format!("{sender} <{reply_to}>")),
        ("Reply-To".to_owned(), reply_to.to_owned()),
        (
            "To".to_owned(),
            format!("{} <{}>", entry.candidate_first_name, entry.candidate_email),
        ),
        ("Subject".to_owned(), entry.subject.clone()),
        ("Date".to_owned(), entry.sent_at.to_rfc2822()),
        ("MIME-Version".to_owned(), "1.0".to_owned()),
        (
            "Content-Type".to_owned(),
            "text/plain; charset=utf-8".to_owned(),
        ),
        ("X-Ledger-Send-Id".to_owned(), entry.send_id.to_string()),
        (
            "X-Ledger-Disposition-Id".to_owned(),
            entry.disposition_id.to_string(),
        ),
        (
            "X-Ledger-Disposition".to_owned(),
            entry.disposition.code().to_owned(),
        ),
        ("X-Ledger-Text-Digest".to_owned(), entry.text_digest.clone()),
        (
            "X-Ledger-Delivery-Status".to_owned(),
            entry.delivery_status.code().to_owned(),
        ),
    ];
    headers.sort_by(|left, right| left.0.cmp(&right.0));

    let mut message = String::new();
    for (name, value) in headers {
        message.push_str(&format!("{name}: {value}\r\n"));
    }
    message.push_str("\r\n");
    for line in entry.body.split('\n') {
        message.push_str(line.trim_end_matches('\r'));
        message.push_str("\r\n");
    }
    message
}

/// Reads a manifest back.
///
/// # Errors
///
/// Returns [`Error::Io`] or [`Error::Json`] when it cannot be read.
pub fn read_manifest(directory: impl AsRef<Path>) -> Result<EmlBatch> {
    let path = directory.as_ref().join("manifest.json");
    let text = std::fs::read_to_string(&path).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    Ok(serde_json::from_str(&text)?)
}

/// Verifies every file against the manifest.
///
/// # Errors
///
/// Returns [`Error::Invalid`] listing the first mismatch, and propagates IO errors.
pub fn verify_batch(directory: impl AsRef<Path>) -> Result<usize> {
    let directory = directory.as_ref();
    let batch = read_manifest(directory)?;
    let mut checked = 0;
    for file in &batch.files {
        let path = directory.join(&file.file);
        let bytes = std::fs::read(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        if sha256_hex(&bytes) != file.sha256 {
            return Err(Error::invalid(
                "eml batch",
                format!("{} no longer matches its manifest hash", file.file),
            ));
        }
        checked += 1;
    }
    Ok(checked)
}

/// Counts and paths, for a CLI summary.
pub fn summary(batch: &EmlBatch, directory: impl AsRef<Path>) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("files".to_owned(), batch.files.len().to_string()),
        (
            "directory".to_owned(),
            directory.as_ref().display().to_string(),
        ),
        ("batch_digest".to_owned(), batch.batch_digest.clone()),
    ])
}

/// A path helper for tests and CLI output.
pub fn manifest_path(directory: impl AsRef<Path>) -> PathBuf {
    directory.as_ref().join("manifest.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::{DeliveryStatus, SendDraft, SendLog};
    use crate::store::Store;
    use crate::template::RenderedEmail;
    use chrono::TimeZone;

    #[test]
    fn a_draft_carries_its_headers_and_body() {
        let (store, dir) = workspace("headers");
        let mut log = SendLog::open(&store).expect("log");
        let entry = log
            .append(
                SendDraft {
                    disposition: disposition(),
                    text_version: crate::id::TextVersion::parse("v1.0").expect("version"),
                    rendered: RenderedEmail {
                        subject: "Update on your application".to_owned(),
                        body: "Dear Ada,\r\n\r\nBody.\r\n".to_owned(),
                    },
                    delivery_status: DeliveryStatus::Queued,
                },
                Utc.with_ymd_and_hms(2026, 11, 20, 9, 0, 0)
                    .single()
                    .expect("time"),
            )
            .expect("append");

        let batch = write_batch(
            dir.join("eml"),
            "REQ-1",
            "R. Patel",
            "careers-feedback@example.com.au",
            &[entry],
        )
        .expect("batch");
        assert_eq!(batch.files.len(), 1);
        assert_eq!(verify_batch(dir.join("eml")).expect("verify"), 1);

        let text = std::fs::read_to_string(dir.join("eml").join("APP-1.eml")).expect("eml");
        assert!(text.contains("To: Ada <ada@example.com>"));
        assert!(text.contains("X-Ledger-Delivery-Status: queued"));
        assert!(text.contains("X-Ledger-Disposition: GATE"));
        assert!(text.contains("\r\n\r\nDear Ada,"));
        assert!(text.contains("X-Ledger-Text-Digest: "));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn editing_a_draft_is_detected() {
        let (store, dir) = workspace("tamper");
        let mut log = SendLog::open(&store).expect("log");
        let entry = log
            .append(
                SendDraft {
                    disposition: disposition(),
                    text_version: crate::id::TextVersion::parse("v1.0").expect("version"),
                    rendered: RenderedEmail {
                        subject: "Update".to_owned(),
                        body: "Body".to_owned(),
                    },
                    delivery_status: DeliveryStatus::Queued,
                },
                Utc::now(),
            )
            .expect("append");
        write_batch(
            dir.join("eml"),
            "REQ-1",
            "R. Patel",
            "careers-feedback@example.com.au",
            &[entry],
        )
        .expect("batch");

        let path = dir.join("eml").join("APP-1.eml");
        let mut bytes = std::fs::read(&path).expect("read");
        bytes.push(b'!');
        std::fs::write(&path, bytes).expect("write");
        assert!(verify_batch(dir.join("eml")).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn workspace(tag: &str) -> (Store, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "ledger-eml-{tag}-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        (Store::open(&dir).expect("workspace"), dir)
    }

    fn disposition() -> crate::disposition::DispositionRecord {
        let requisition = crate::requisition::Requisition::new(
            crate::requisition::RequisitionDraft {
                id: crate::id::RequisitionId::try_from("REQ-1").expect("id"),
                employer: "Example Health".to_owned(),
                title: "Registered Nurse".to_owned(),
                reply_to: crate::id::Mailbox::try_from("careers-feedback@example.com.au")
                    .expect("mailbox"),
                sender: crate::requisition::Sender::try_new("R. Patel", "Coordinator")
                    .expect("sender"),
                gates: vec![crate::gate::GateKind::RightToWork],
                facts: Default::default(),
            },
            Utc::now(),
        )
        .expect("requisition");
        let form = crate::disposition::NewDisposition::try_new(
            &requisition,
            &crate::gate::GateLibrary::seed(),
            crate::disposition::FormSubmission {
                application_id: crate::id::ApplicationId::try_from("APP-1").expect("app"),
                candidate_first_name: "Ada".to_owned(),
                candidate_email: crate::id::CandidateEmail::try_from("ada@example.com")
                    .expect("email"),
                recruiter: crate::id::RecruiterId::try_from("rec-rui").expect("recruiter"),
                decision_maker: crate::id::DecisionMakerId::try_from("dm-chen")
                    .expect("decision maker"),
                disposition: crate::disposition::Disposition::Gate {
                    gate: crate::gate::GateKind::RightToWork,
                    answer: crate::gate::GateAnswer::NotHeld,
                },
                supersedes: None,
            },
            Utc::now(),
        )
        .expect("form");
        crate::disposition::DispositionRecord {
            id: crate::id::DispositionId::try_from("DSP-0001").expect("id"),
            locked: true,
            form,
        }
    }
}
