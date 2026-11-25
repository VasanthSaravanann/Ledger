//! Delivery confirmation: the only path from a queued draft to a confirmed send.
//!
//! `send_log.jsonl` is append-only, so the `DELIVERY_STATUS` it records at dispatch
//! time can never be edited afterwards. A fact learned later — the mail provider
//! accepted the draft — is therefore a **new chained entry** in `delivery.jsonl`,
//! keyed by `SEND_ID`, recording who confirmed it and when. A workspace that never
//! confirms a send keeps exactly the send-log head it had before this module
//! existed; `tests/golden.rs` pins both states.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::digest::{
    CanonicalValue, DIGEST_VERSION, canonical_bytes, epoch_seconds, sha256_hex, truncate_to_seconds,
};
use crate::error::{Error, Result};
use crate::id::SendId;
use crate::jsonl::AppendLog;
use crate::log::{DeliveryStatus, GENESIS_HASH, SendEntry, SendLog};
use crate::store::{self, Store};

/// The entry type every confirmation carries.
pub const CONFIRMED: &str = "DELIVERY_CONFIRMED";

/// One confirmation that a recorded draft reached the mail provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryConfirmation {
    /// Zero-based position in the delivery chain.
    pub seq: u64,
    /// The previous confirmation's `entry_hash`, or [`GENESIS_HASH`].
    pub prev_hash: String,
    /// SHA-256 over the canonical payload; see `FORMAT_SPEC.md` §11.
    pub entry_hash: String,
    /// Always [`CONFIRMED`]; stored so a reader can tell entry types apart.
    #[serde(rename = "type")]
    pub entry_type: String,
    /// The send-log entry this confirmation refers to.
    pub send_id: SendId,
    /// The operator who confirmed the delivery.
    pub confirmed_by: String,
    /// The confirmer's Ed25519 public key, lowercase hex, so the name is bound to a
    /// key even after the operator registry changes.
    pub confirmed_by_key: String,
    /// When the confirmation was recorded, truncated to whole seconds.
    pub confirmed_at: DateTime<Utc>,
}

impl DeliveryConfirmation {
    /// The canonical digest payload, exactly the allowlist in `FORMAT_SPEC.md` §11.
    fn payload(&self) -> CanonicalValue {
        CanonicalValue::object([
            ("digest_version", CanonicalValue::Int(DIGEST_VERSION)),
            ("seq", CanonicalValue::Int(self.seq as i64)),
            ("prev_hash", CanonicalValue::text(&self.prev_hash)),
            ("type", CanonicalValue::text(&self.entry_type)),
            ("send_id", CanonicalValue::text(self.send_id.as_str())),
            ("confirmed_by", CanonicalValue::text(&self.confirmed_by)),
            (
                "confirmed_by_key",
                CanonicalValue::text(&self.confirmed_by_key),
            ),
            (
                "confirmed_at",
                CanonicalValue::Int(epoch_seconds(self.confirmed_at)),
            ),
        ])
    }

    /// The exact canonical bytes this entry's hash is taken over, so an independent
    /// verifier can recompute it without trusting this binary.
    pub fn canonical_payload(&self) -> String {
        String::from_utf8(canonical_bytes(&self.payload())).unwrap_or_default()
    }

    /// Recomputes the chain hash from the canonical payload.
    fn compute_hash(&self) -> String {
        sha256_hex(&canonical_bytes(&self.payload()))
    }
}

/// The status a reader should believe for one send.
///
/// A confirmation always wins: it is the later, chained fact. Otherwise the status
/// recorded in the immutable send entry stands.
pub fn effective_status(entry: &SendEntry, confirmed: bool) -> DeliveryStatus {
    if confirmed {
        DeliveryStatus::Sent
    } else {
        entry.delivery_status
    }
}

/// The append-only, hash-chained log of delivery confirmations.
#[derive(Debug)]
pub struct DeliveryLog {
    path: PathBuf,
    log: AppendLog<DeliveryConfirmation>,
    tail: Option<DeliveryConfirmation>,
    confirmed: BTreeSet<SendId>,
}

impl DeliveryLog {
    /// Opens and verifies the delivery chain before anything is appended.
    ///
    /// # Errors
    ///
    /// Returns [`Error::LogCorrupt`] when the chain does not verify, and
    /// [`Error::TornWrite`] when the file ends mid-line.
    pub fn open(store: &Store) -> Result<Self> {
        let log = store.log(store::DELIVERY);
        let entries: Vec<DeliveryConfirmation> = log.read_all()?;
        verify_chain(log.path(), &entries)?;
        let confirmed = entries.iter().map(|entry| entry.send_id.clone()).collect();
        Ok(Self {
            path: log.path().to_path_buf(),
            log,
            tail: entries.last().cloned(),
            confirmed,
        })
    }

    /// Records that `send_id` was delivered, as a new chained entry.
    ///
    /// The referenced send must exist in the send log, must not be confirmed
    /// already, and the confirmation cannot predate the send it confirms.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] when the send is unknown, already confirmed, or
    /// `at` is earlier than the send; [`Error::Io`] when the entry cannot be
    /// written.
    pub fn confirm(
        &mut self,
        sends: &SendLog,
        send_id: &SendId,
        confirmed_by: &str,
        confirmed_by_key: &str,
        at: DateTime<Utc>,
    ) -> Result<DeliveryConfirmation> {
        let send = sends
            .entries()?
            .into_iter()
            .find(|entry| &entry.send_id == send_id)
            .ok_or_else(|| {
                Error::invalid(
                    "send id",
                    format!("{} is not in the send log", send_id.as_str()),
                )
            })?;
        if self.confirmed.contains(send_id) {
            return Err(Error::invalid(
                "delivery",
                format!(
                    "{} is already confirmed; a confirmation is appended once",
                    send_id.as_str()
                ),
            ));
        }
        let at = truncate_to_seconds(at);
        if at < send.sent_at {
            return Err(Error::invalid(
                "confirmed_at",
                format!(
                    "cannot confirm {} before it was recorded as sent ({})",
                    send_id.as_str(),
                    send.sent_at.to_rfc3339()
                ),
            ));
        }
        if confirmed_by.trim().is_empty() || confirmed_by_key.trim().is_empty() {
            return Err(Error::invalid(
                "confirmed_by",
                "a confirmation must name the operator and their key",
            ));
        }

        let prev_hash = self
            .tail
            .as_ref()
            .map_or_else(|| GENESIS_HASH.to_owned(), |tail| tail.entry_hash.clone());
        let seq = self.tail.as_ref().map_or(0, |tail| tail.seq + 1);
        let mut entry = DeliveryConfirmation {
            seq,
            prev_hash,
            entry_hash: String::new(),
            entry_type: CONFIRMED.to_owned(),
            send_id: send_id.clone(),
            confirmed_by: confirmed_by.trim().to_owned(),
            confirmed_by_key: confirmed_by_key.trim().to_lowercase(),
            confirmed_at: at,
        };
        entry.entry_hash = entry.compute_hash();
        self.log.append(&entry)?;
        self.confirmed.insert(entry.send_id.clone());
        self.tail = Some(entry.clone());
        Ok(entry)
    }

    /// Every confirmation, in order.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the file cannot be read.
    pub fn entries(&self) -> Result<Vec<DeliveryConfirmation>> {
        self.log.read_all()
    }

    /// Whether `send_id` has a confirmation on file.
    pub fn is_confirmed(&self, send_id: &SendId) -> bool {
        self.confirmed.contains(send_id)
    }

    /// The confirmation for `send_id`, when one exists.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the file cannot be read.
    pub fn confirmation_for(&self, send_id: &SendId) -> Result<Option<DeliveryConfirmation>> {
        Ok(self
            .entries()?
            .into_iter()
            .find(|entry| &entry.send_id == send_id))
    }

    /// The number of confirmations known to this handle.
    pub fn len(&self) -> usize {
        self.tail.as_ref().map_or(0, |tail| tail.seq as usize + 1)
    }

    /// Whether nothing has been confirmed yet.
    pub fn is_empty(&self) -> bool {
        self.tail.is_none()
    }

    /// The number of confirmations in the chain.
    ///
    /// # Errors
    ///
    /// Returns [`Error::LogCorrupt`] when the chain does not verify.
    pub fn verification(&self) -> Result<LogVerification> {
        let entries = self.entries()?;
        verify_chain(&self.path, &entries)?;
        Ok(LogVerification {
            entries: entries.len(),
            head: entries
                .last()
                .map_or_else(|| GENESIS_HASH.to_owned(), |entry| entry.entry_hash.clone()),
        })
    }
}

/// Whether the delivery chain is intact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogVerification {
    /// How many confirmations were checked.
    pub entries: usize,
    /// The hash of the newest confirmation, or [`GENESIS_HASH`] when empty.
    pub head: String,
}

/// Checks sequence numbers, chaining, recomputed hashes and time order.
fn verify_chain(path: &Path, entries: &[DeliveryConfirmation]) -> Result<()> {
    let mut expected_prev = GENESIS_HASH.to_owned();
    let mut previous_at: Option<DateTime<Utc>> = None;
    let mut seen = BTreeSet::new();
    for (index, entry) in entries.iter().enumerate() {
        let seq = index as u64;
        let corrupt = |reason: String| Error::LogCorrupt {
            path: path.to_path_buf(),
            seq,
            reason,
        };
        if entry.seq != seq {
            return Err(corrupt(format!(
                "expected sequence {seq}, found {}",
                entry.seq
            )));
        }
        if entry.prev_hash != expected_prev {
            return Err(corrupt(
                "prev_hash does not match the previous entry".to_owned(),
            ));
        }
        if entry.compute_hash() != entry.entry_hash {
            return Err(corrupt(
                "entry hash does not cover the stored fields".to_owned(),
            ));
        }
        if entry.entry_type != CONFIRMED {
            return Err(corrupt(format!(
                "unknown entry type {:?}; this build writes {CONFIRMED}",
                entry.entry_type
            )));
        }
        if !seen.insert(entry.send_id.clone()) {
            return Err(corrupt(format!(
                "{} is confirmed more than once",
                entry.send_id.as_str()
            )));
        }
        if let Some(previous) = previous_at {
            if entry.confirmed_at < previous {
                return Err(corrupt(
                    "confirmed_at is earlier than the previous confirmation".to_owned(),
                ));
            }
        }
        previous_at = Some(entry.confirmed_at);
        expected_prev = entry.entry_hash.clone();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::disposition::{Disposition, DispositionRecord, NewDisposition};
    use crate::gate::{GateAnswer, GateKind};
    use crate::id::{
        ApplicationId, CandidateEmail, DispositionId, Mailbox, RecruiterId, TextVersion,
    };
    use crate::log::{SendDraft, SendLog};
    use crate::requisition::{Requisition, Sender};
    use crate::template::RenderedEmail;
    use chrono::TimeZone;
    use std::collections::BTreeMap;
    use std::fs;

    fn at(minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 24, 9, minute, 0)
            .single()
            .expect("timestamp")
    }

    fn disposition() -> DispositionRecord {
        let requisition = Requisition::new(
            crate::requisition::RequisitionDraft {
                id: crate::id::RequisitionId::try_from("REQ-1").expect("id"),
                employer: "Example Health".to_owned(),
                title: "Registered Nurse".to_owned(),
                reply_to: Mailbox::try_from("careers-feedback@example.com.au").expect("mailbox"),
                sender: Sender::try_new("R. Patel", "Coordinator").expect("sender"),
                gates: vec![GateKind::RightToWork],
                facts: BTreeMap::new(),
            },
            at(0),
        )
        .expect("requisition");
        let form = NewDisposition::try_new(
            &requisition,
            &crate::gate::GateLibrary::seed(),
            crate::disposition::FormSubmission {
                application_id: ApplicationId::try_from("APP-1").expect("application"),
                candidate_first_name: "Ada".to_owned(),
                candidate_email: CandidateEmail::try_from("ada@example.com").expect("email"),
                recruiter: RecruiterId::try_from("rec-rui").expect("recruiter"),
                decision_maker: crate::id::DecisionMakerId::try_from("dm-chen")
                    .expect("decision maker"),
                disposition: Disposition::Gate {
                    gate: GateKind::RightToWork,
                    answer: GateAnswer::NotHeld,
                },
                supersedes: None,
            },
            at(1),
        )
        .expect("form");
        DispositionRecord {
            id: DispositionId::try_from("DSP-0001").expect("id"),
            locked: true,
            form,
        }
    }

    fn temp_store(tag: &str) -> (Store, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "ledger-delivery-{tag}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _ = fs::remove_dir_all(&dir);
        (Store::open(&dir).expect("workspace"), dir)
    }

    fn queued_send(store: &Store) -> SendEntry {
        let mut log = SendLog::open(store).expect("send log");
        log.append(
            SendDraft {
                disposition: disposition(),
                text_version: TextVersion::parse("v1.0").expect("version"),
                rendered: RenderedEmail {
                    subject: "Update on your application".to_owned(),
                    body: "Dear Ada,\n\nYou indicated ...".to_owned(),
                },
                delivery_status: DeliveryStatus::Queued,
            },
            at(2),
        )
        .expect("send")
    }

    #[test]
    fn a_queued_send_is_confirmed_only_by_a_new_chained_entry() {
        let (store, dir) = temp_store("confirm");
        let send = queued_send(&store);
        let mut deliveries = DeliveryLog::open(&store).expect("delivery log");
        assert!(deliveries.is_empty());

        let confirmation = deliveries
            .confirm(
                &SendLog::open(&store).expect("send log"),
                &send.send_id,
                "M. Okafor",
                &"ab".repeat(32),
                at(3),
            )
            .expect("confirm");
        assert_eq!(confirmation.entry_type, CONFIRMED);
        assert_eq!(confirmation.seq, 0);
        assert_eq!(confirmation.prev_hash, GENESIS_HASH);
        assert_eq!(confirmation.confirmed_by, "M. Okafor");
        assert_eq!(confirmation.confirmed_at, at(3));
        assert_eq!(confirmation.entry_hash, confirmation.compute_hash());

        let reopened = DeliveryLog::open(&store).expect("reopen");
        assert!(reopened.is_confirmed(&send.send_id));
        assert_eq!(reopened.verification().expect("verify").entries, 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_same_send_cannot_be_confirmed_twice_and_time_cannot_run_backwards() {
        let (store, dir) = temp_store("twice");
        let send = queued_send(&store);
        let mut deliveries = DeliveryLog::open(&store).expect("delivery log");
        let sends = SendLog::open(&store).expect("send log");
        let key = "ab".repeat(32);
        assert!(
            deliveries
                .confirm(&sends, &send.send_id, "M. Okafor", &key, at(2))
                .is_ok()
        );
        assert!(
            deliveries
                .confirm(&sends, &send.send_id, "M. Okafor", &key, at(4))
                .is_err(),
            "a second confirmation must be refused"
        );
        let other = SendId::try_from("SND-000099").expect("id");
        assert!(
            deliveries
                .confirm(&sends, &other, "M. Okafor", &key, at(4))
                .is_err(),
            "an unknown send must be refused"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn editing_a_confirmation_breaks_the_delivery_chain() {
        let (store, dir) = temp_store("edit");
        let send = queued_send(&store);
        let mut deliveries = DeliveryLog::open(&store).expect("delivery log");
        deliveries
            .confirm(
                &SendLog::open(&store).expect("send log"),
                &send.send_id,
                "M. Okafor",
                &"ab".repeat(32),
                at(3),
            )
            .expect("confirm");
        drop(deliveries);

        let path = store.path(store::DELIVERY);
        let text = fs::read_to_string(&path).expect("read");
        fs::write(&path, text.replace("\"M. Okafor\"", "\"Someone Else\"")).expect("edit");
        assert!(DeliveryLog::open(&store).is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_confirmation_must_not_predate_the_send() {
        let (store, dir) = temp_store("early");
        let send = queued_send(&store);
        let mut deliveries = DeliveryLog::open(&store).expect("delivery log");
        assert!(
            deliveries
                .confirm(
                    &SendLog::open(&store).expect("send log"),
                    &send.send_id,
                    "M. Okafor",
                    &"ab".repeat(32),
                    at(1)
                )
                .is_err()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn effective_status_puts_a_confirmation_first() {
        let (store, dir) = temp_store("status");
        let send = queued_send(&store);
        assert_eq!(effective_status(&send, false), DeliveryStatus::Queued);
        assert_eq!(effective_status(&send, true), DeliveryStatus::Sent);
        let _ = fs::remove_dir_all(&dir);
    }
}
