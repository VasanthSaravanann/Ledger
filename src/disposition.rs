//! The locked disposition form: `GATE`, `FIELD`, `ADMIN` (Task 2.1).
//!
//! Three options, no free text, no "Other", no "skills mismatch". Each record
//! carries the recruiter, the named decision maker, the exact disposition, and a
//! capture timestamp; records are append-only and cannot be edited. A wrong record
//! is corrected by submitting a new record that `SUPERSEDES` it, which keeps the
//! original on file (APP accuracy, and a contemporaneous trace for *Barclay*).

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::gate::{GateAnswer, GateKind, GateLibrary};
use crate::id::{
    ApplicationId, CandidateEmail, DecisionMakerId, DispositionId, RecruiterId, RequisitionId,
    TemplateKind,
};
use crate::jsonl::AppendLog;
use crate::requisition::Requisition;
use crate::store::{self, Store};
use crate::text;

/// The three exact administrative strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AdminReason {
    /// The requisition was withdrawn.
    RequisitionWithdrawn,
    /// The role was rescoped and will be re-advertised.
    RoleRescoped,
    /// The position was filled internally or by redeployment.
    FilledInternallyOrRedeployed,
}

impl AdminReason {
    /// Every reason the form offers.
    pub const ALL: [Self; 3] = [
        Self::RequisitionWithdrawn,
        Self::RoleRescoped,
        Self::FilledInternallyOrRedeployed,
    ];

    /// The stable code used in storage and on the command line.
    pub fn code(self) -> &'static str {
        match self {
            Self::RequisitionWithdrawn => "requisition-withdrawn",
            Self::RoleRescoped => "role-rescoped",
            Self::FilledInternallyOrRedeployed => "filled-internally",
        }
    }

    /// The exact pre-approved string rendered into `Template C`.
    pub fn sentence(self) -> &'static str {
        match self {
            Self::RequisitionWithdrawn => "the requisition was withdrawn",
            Self::RoleRescoped => "the role was rescoped and will be re-advertised",
            Self::FilledInternallyOrRedeployed => {
                "the position was filled internally or by redeployment"
            }
        }
    }
}

impl fmt::Display for AdminReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl FromStr for AdminReason {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        let needle = value.trim().to_lowercase();
        Self::ALL
            .into_iter()
            .find(|reason| reason.code() == needle)
            .ok_or_else(|| {
                Error::invalid(
                    "admin reason",
                    format!(
                        "expected requisition-withdrawn, role-rescoped or filled-internally; \
                         got {value:?}"
                    ),
                )
            })
    }
}

/// The `DISPOSITION` column: exactly three options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DispositionKind {
    /// The candidate's own answers show they do not meet a stated requirement.
    Gate,
    /// The candidate met the requirements; others presented a stronger case.
    Field,
    /// Not progressed for a reason unrelated to this candidate.
    Admin,
}

impl DispositionKind {
    /// Every option.
    pub const ALL: [Self; 3] = [Self::Gate, Self::Field, Self::Admin];

    /// The exact code used in the form, the logs, and the metrics.
    pub fn code(self) -> &'static str {
        match self {
            Self::Gate => "GATE",
            Self::Field => "FIELD",
            Self::Admin => "ADMIN",
        }
    }

    /// The email template this option uses.
    pub fn template_kind(self) -> TemplateKind {
        match self {
            Self::Gate => TemplateKind::Gate,
            Self::Field => TemplateKind::Field,
            Self::Admin => TemplateKind::Admin,
        }
    }
}

impl fmt::Display for DispositionKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl FromStr for DispositionKind {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        let needle = value.trim().to_uppercase();
        Self::ALL
            .into_iter()
            .find(|kind| kind.code() == needle)
            .ok_or_else(|| {
                Error::invalid(
                    "disposition",
                    format!("expected GATE, FIELD or ADMIN, got {value:?}"),
                )
            })
    }
}

/// The exact disposition selected on the form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "disposition", rename_all = "kebab-case")]
pub enum Disposition {
    /// `GATE`: a stated mandatory requirement is not met on the candidate's answers.
    Gate {
        /// `GATE_CODE`: one of the requisition's active knockouts.
        gate: GateKind,
        /// The candidate's own answer to that knockout question.
        answer: GateAnswer,
    },
    /// `FIELD`: requirements met; other applicants presented a stronger case.
    Field,
    /// `ADMIN`: closed for a reason unrelated to this candidate.
    Admin {
        /// One of the three exact administrative strings.
        reason: AdminReason,
    },
}

impl Disposition {
    /// The `DISPOSITION` code.
    pub fn kind(&self) -> DispositionKind {
        match self {
            Self::Gate { .. } => DispositionKind::Gate,
            Self::Field => DispositionKind::Field,
            Self::Admin { .. } => DispositionKind::Admin,
        }
    }

    /// The `GATE_CODE`, for `GATE` records only.
    pub fn gate(&self) -> Option<GateKind> {
        match self {
            Self::Gate { gate, .. } => Some(*gate),
            _ => None,
        }
    }

    /// Checks the row against the requisition and the copy library.
    ///
    /// # Errors
    ///
    /// Returns [`Error::GateNotInRequisition`] when the gate is not screened on,
    /// [`Error::DisallowedAnswer`] when the answer is not offered, and
    /// [`Error::GateUnavailable`] when the gate's requisition facts are missing —
    /// the caller must record `FIELD` instead.
    pub fn validate_against(&self, requisition: &Requisition, library: &GateLibrary) -> Result<()> {
        match self {
            Self::Gate { gate, answer } => {
                if !requisition.gates.contains(gate) {
                    return Err(Error::GateNotInRequisition {
                        gate: *gate,
                        requisition: requisition.id.clone(),
                    });
                }
                if !gate.allowed_answers().contains(answer) {
                    return Err(Error::DisallowedAnswer {
                        gate: *gate,
                        answer: *answer,
                    });
                }
                let missing = library.missing_facts(*gate, &requisition.facts);
                if !missing.is_empty() {
                    return Err(Error::GateUnavailable {
                        gate: *gate,
                        missing,
                    });
                }
                Ok(())
            }
            Self::Field => {
                if requisition.comparative.is_none() {
                    return Err(Error::invalid(
                        "disposition",
                        "FIELD needs the role-level comparative paragraph to exist first",
                    ));
                }
                Ok(())
            }
            Self::Admin { .. } => Ok(()),
        }
    }
}

impl fmt::Display for Disposition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Gate { gate, answer } => write!(formatter, "GATE:{gate}:{answer}"),
            Self::Field => formatter.write_str("FIELD"),
            Self::Admin { reason } => write!(formatter, "ADMIN:{reason}"),
        }
    }
}

/// A validated form submission, before the log assigns it a record id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewDisposition {
    /// `APPLICATION_ID`: must match the ATS export exactly.
    pub application_id: ApplicationId,
    /// `FIRST_NAME`, used only as a merge value.
    pub candidate_first_name: String,
    /// `EMAIL`.
    pub candidate_email: CandidateEmail,
    /// `REQ_ID`.
    pub requisition: RequisitionId,
    /// `RECRUITER_ID`: a named individual.
    pub recruiter: RecruiterId,
    /// `DECISION_MAKER_ID`: the named person who made the adverse-action call.
    pub decision_maker: DecisionMakerId,
    /// `CAPTURE_TIMESTAMP`.
    pub captured_at: DateTime<Utc>,
    /// `DISPOSITION` + `GATE_CODE`.
    pub disposition: Disposition,
    /// `SUPERSEDES`: set only when correcting an existing record.
    pub supersedes: Option<DispositionId>,
    /// Ed25519 signature over [`NewDisposition::digest`], when an operator key is
    /// registered. Absent only in bootstrap or demo workspaces with no registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<crate::identity::Signed>,
}

impl NewDisposition {
    /// The canonical digest this record's signature covers.
    ///
    /// Deliberately excludes `signature`, and includes the instant at second
    /// precision so the digest is stable across serialisation.
    pub fn digest(&self) -> String {
        let gate = self
            .disposition
            .gate()
            .map(|gate| gate.code().to_owned())
            .unwrap_or_default();
        let supersedes = self
            .supersedes
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default();
        crate::identity::digest_fields([
            ("application_id", self.application_id.to_string()),
            ("requisition", self.requisition.to_string()),
            ("candidate_first_name", self.candidate_first_name.clone()),
            ("candidate_email", self.candidate_email.to_string()),
            ("recruiter", self.recruiter.to_string()),
            ("decision_maker", self.decision_maker.to_string()),
            (
                "captured_at",
                crate::digest::epoch_seconds(self.captured_at).to_string(),
            ),
            ("disposition", self.disposition.kind().code().to_owned()),
            ("gate_code", gate),
            ("supersedes", supersedes),
        ])
    }
}

/// One raw form submission, before it is checked against its requisition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormSubmission {
    /// `APPLICATION_ID`.
    pub application_id: ApplicationId,
    /// `FIRST_NAME`.
    pub candidate_first_name: String,
    /// `EMAIL`.
    pub candidate_email: CandidateEmail,
    /// `RECRUITER_ID`.
    pub recruiter: RecruiterId,
    /// `DECISION_MAKER_ID`.
    pub decision_maker: DecisionMakerId,
    /// `DISPOSITION` + `GATE_CODE`.
    pub disposition: Disposition,
    /// `SUPERSEDES`.
    pub supersedes: Option<DispositionId>,
}

impl NewDisposition {
    /// Builds and validates a submission.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] for a blank name, and propagates
    /// [`Disposition::validate_against`] errors.
    pub fn try_new(
        requisition: &Requisition,
        library: &GateLibrary,
        form: FormSubmission,
        captured_at: DateTime<Utc>,
    ) -> Result<Self> {
        form.disposition.validate_against(requisition, library)?;
        Ok(Self {
            application_id: form.application_id,
            candidate_first_name: text::required(
                "candidate first name",
                form.candidate_first_name,
                120,
            )?,
            candidate_email: form.candidate_email,
            requisition: requisition.id.clone(),
            recruiter: form.recruiter,
            decision_maker: form.decision_maker,
            captured_at,
            supersedes: form.supersedes,
            disposition: form.disposition,
            signature: None,
        })
    }
}

impl NewDisposition {
    /// Attaches an operator signature over [`NewDisposition::digest`].
    ///
    /// # Errors
    ///
    /// Propagates signature-creation errors.
    pub fn signed(
        mut self,
        registry: &crate::identity::Registry,
        store: &Store,
        operator: &str,
    ) -> Result<Self> {
        let digest = self.digest();
        self.signature = Some(crate::identity::sign(registry, store, operator, &digest)?);
        Ok(self)
    }

    /// Whether the attached signature covers this record and verifies.
    pub fn signature_valid(&self) -> bool {
        self.signature
            .as_ref()
            .is_some_and(|signature| signature.verify(&self.digest()))
    }
}

/// One stored form submission. Locked, timestamped, append-only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispositionRecord {
    /// The record id used by `SUPERSEDES`.
    pub id: DispositionId,
    /// `LOCKED`: always true; the field exists so the log matches the schema.
    #[serde(default = "locked")]
    pub locked: bool,
    /// The captured row.
    #[serde(flatten)]
    pub form: NewDisposition,
}

/// Always-true default for the `LOCKED` column.
fn locked() -> bool {
    true
}

impl DispositionRecord {
    /// Whether the record may be edited. It never can.
    pub fn is_locked(&self) -> bool {
        self.locked
    }

    /// The stable display form, for reports.
    pub fn summary(&self) -> String {
        format!(
            "{} {} {}",
            self.id, self.form.application_id, self.form.disposition
        )
    }
}

/// The append-only collection of captured dispositions.
#[derive(Debug)]
pub struct Dispositions {
    log: AppendLog<DispositionRecord>,
    records: Vec<DispositionRecord>,
}

impl Dispositions {
    /// Opens the disposition log and loads what has been captured.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] or [`Error::Json`] when the log cannot be read.
    pub fn open(store: &Store) -> Result<Self> {
        let log = store.log(store::DISPOSITIONS);
        let records = log.read_all()?;
        Ok(Self { log, records })
    }

    /// Captures one disposition, or one correction.
    ///
    /// A submission without `supersedes` is refused when the application already
    /// has a current record; a submission with `supersedes` must name the current
    /// head of that application's chain.
    ///
    /// # Errors
    ///
    /// Returns [`Error::DuplicateDisposition`] for an unlinked second capture,
    /// [`Error::UnknownDisposition`] for an unknown `supersedes` target,
    /// [`Error::AlreadySuperseded`] when the target is not the head, and
    /// [`Error::SupersedeMismatch`] when the target belongs to another candidate.
    pub fn record(&mut self, form: NewDisposition) -> Result<&DispositionRecord> {
        match &form.supersedes {
            Some(previous) => {
                let target = self
                    .find(previous)
                    .ok_or_else(|| Error::UnknownDisposition(previous.clone()))?;
                if target.form.requisition != form.requisition {
                    return Err(Error::invalid(
                        "supersedes",
                        format!(
                            "{} belongs to requisition {}, not {}",
                            previous, target.form.requisition, form.requisition
                        ),
                    ));
                }
                if target.form.application_id != form.application_id {
                    return Err(Error::SupersedeMismatch {
                        record: previous.clone(),
                        owner: target.form.application_id.clone(),
                        application: form.application_id.clone(),
                    });
                }
                if self
                    .records
                    .iter()
                    .any(|record| record.form.supersedes.as_ref() == Some(previous))
                {
                    return Err(Error::AlreadySuperseded(previous.clone()));
                }
            }
            None => {
                if let Some(existing) = self.head(&form.requisition, &form.application_id) {
                    return Err(Error::DuplicateDisposition {
                        requisition: form.requisition.clone(),
                        application: form.application_id.clone(),
                        existing: existing.id.clone(),
                    });
                }
            }
        }

        let id = DispositionId::try_from(format!("DSP-{:04}", self.records.len() + 1))?;
        let record = DispositionRecord {
            id,
            locked: true,
            form,
        };
        self.log.append(&record)?;
        self.records.push(record);
        self.records
            .last()
            .ok_or_else(|| Error::invalid("disposition", "captured record disappeared"))
    }

    /// The current (non-superseded) record for one application.
    pub fn head(
        &self,
        requisition: &RequisitionId,
        application: &ApplicationId,
    ) -> Option<&DispositionRecord> {
        self.records
            .iter()
            .filter(|record| {
                &record.form.requisition == requisition
                    && &record.form.application_id == application
            })
            .find(|record| {
                !self
                    .records
                    .iter()
                    .any(|other| other.form.supersedes.as_ref() == Some(&record.id))
            })
    }

    /// Every record for one application, oldest first.
    pub fn history(&self, application: &ApplicationId) -> Vec<&DispositionRecord> {
        self.records
            .iter()
            .filter(|record| &record.form.application_id == application)
            .collect()
    }

    /// Returns the record with `id`.
    pub fn find(&self, id: &DispositionId) -> Option<&DispositionRecord> {
        self.records.iter().find(|record| &record.id == id)
    }

    /// Every captured record, in capture order.
    pub fn records(&self) -> &[DispositionRecord] {
        &self.records
    }

    /// How many records, including superseded ones.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether nothing has been captured.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate::{FactKey, GateKind};
    use crate::id::{Mailbox, TextVersion};
    use crate::requisition::Sender;
    use chrono::TimeZone;
    use std::collections::BTreeMap;

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 24, 9, 0, 0)
            .single()
            .expect("timestamp")
    }

    fn library() -> GateLibrary {
        GateLibrary::seed()
    }

    fn requisition() -> Requisition {
        let mut requisition = Requisition::new(
            crate::requisition::RequisitionDraft {
                id: RequisitionId::try_from("REQ-1").expect("id"),
                employer: "Example Health".to_owned(),
                title: "Registered Nurse".to_owned(),
                reply_to: Mailbox::try_from("careers-feedback@example.com.au").expect("mailbox"),
                sender: Sender::try_new("R. Patel", "Recruitment Coordinator").expect("sender"),
                gates: vec![GateKind::RightToWork, GateKind::WorkRightsDuration],
                facts: BTreeMap::from([
                    (FactKey::RoleEndOrMinTerm, "30 June 2027".to_owned()),
                    (FactKey::MinTerm, "12 months".to_owned()),
                    (
                        FactKey::RoleDurationReason,
                        "The appointment is fixed-term.".to_owned(),
                    ),
                ]),
            },
            at(),
        )
        .expect("requisition");
        requisition
            .set_comparative(
                [
                    "assessment of deteriorating patients".to_owned(),
                    "timely escalation".to_owned(),
                    "clear documentation".to_owned(),
                ],
                "a de-identified case note",
                "a post-graduate qualification",
                TextVersion::parse("v1.0").expect("version"),
            )
            .expect("comparative");
        requisition
    }

    fn form(application: &str, disposition: Disposition) -> NewDisposition {
        let requisition = requisition();
        NewDisposition::try_new(
            &requisition,
            &library(),
            FormSubmission {
                application_id: ApplicationId::try_from(application).expect("application"),
                candidate_first_name: "Ada".to_owned(),
                candidate_email: CandidateEmail::try_from("ada@example.com").expect("email"),
                recruiter: RecruiterId::try_from("rec-rui").expect("recruiter"),
                decision_maker: DecisionMakerId::try_from("dm-chen").expect("decision maker"),
                disposition,
                supersedes: None,
            },
            at(),
        )
        .expect("valid form")
    }

    fn temp_store(tag: &str) -> (Store, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "ledger-disp-{tag}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        (Store::open(&dir).expect("workspace"), dir)
    }

    #[test]
    fn a_gate_without_facts_must_be_recorded_as_field() {
        let mut requisition = requisition();
        let library = library();
        requisition.facts.remove(&FactKey::MinTerm);
        let result = NewDisposition::try_new(
            &requisition,
            &library,
            FormSubmission {
                application_id: ApplicationId::try_from("APP-1").expect("application"),
                candidate_first_name: "Ada".to_owned(),
                candidate_email: CandidateEmail::try_from("ada@example.com").expect("email"),
                recruiter: RecruiterId::try_from("rec-rui").expect("recruiter"),
                decision_maker: DecisionMakerId::try_from("dm-chen").expect("decision maker"),
                disposition: Disposition::Gate {
                    gate: GateKind::WorkRightsDuration,
                    answer: GateAnswer::ShorterThanRole,
                },
                supersedes: None,
            },
            at(),
        );
        assert!(matches!(result, Err(Error::GateUnavailable { .. })));
    }

    #[test]
    fn a_second_capture_must_supersede_and_corrections_chain() {
        let (store, dir) = temp_store("chain");
        let mut dispositions = Dispositions::open(&store).expect("log");

        let first = dispositions
            .record(form("APP-1", Disposition::Field))
            .expect("first capture")
            .id
            .clone();
        assert_eq!(first.as_str(), "DSP-0001");

        let duplicate = dispositions.record(form("APP-1", Disposition::Field));
        assert!(matches!(duplicate, Err(Error::DuplicateDisposition { .. })));

        let correction = NewDisposition {
            supersedes: Some(first.clone()),
            disposition: Disposition::Admin {
                reason: AdminReason::RequisitionWithdrawn,
            },
            signature: None,
            ..form("APP-1", Disposition::Field)
        };
        let corrected = dispositions
            .record(correction)
            .expect("correction")
            .id
            .clone();
        assert_eq!(
            dispositions
                .head(
                    &RequisitionId::try_from("REQ-1").expect("id"),
                    &ApplicationId::try_from("APP-1").expect("application")
                )
                .expect("head")
                .id,
            corrected
        );
        assert_eq!(
            dispositions
                .history(&ApplicationId::try_from("APP-1").expect("application"))
                .len(),
            2
        );

        let second_correction = NewDisposition {
            supersedes: Some(first.clone()),
            ..form("APP-1", Disposition::Field)
        };
        assert!(matches!(
            dispositions.record(second_correction),
            Err(Error::AlreadySuperseded(_))
        ));

        let reopened = Dispositions::open(&store).expect("reopen");
        assert_eq!(reopened.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_correction_cannot_cross_candidates() {
        let (store, dir) = temp_store("cross");
        let mut dispositions = Dispositions::open(&store).expect("log");
        let first = dispositions
            .record(form("APP-1", Disposition::Field))
            .expect("capture")
            .id
            .clone();
        let other = NewDisposition {
            supersedes: Some(first),
            ..form("APP-2", Disposition::Field)
        };
        assert!(matches!(
            dispositions.record(other),
            Err(Error::SupersedeMismatch { .. })
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn records_are_locked_by_construction() {
        let (store, dir) = temp_store("locked");
        let mut dispositions = Dispositions::open(&store).expect("log");
        let record = dispositions
            .record(form("APP-1", Disposition::Field))
            .expect("capture");
        assert!(record.is_locked());
        assert!(record.locked);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_three_admin_strings_are_exact() {
        assert_eq!(
            AdminReason::RequisitionWithdrawn.sentence(),
            "the requisition was withdrawn"
        );
        assert_eq!(
            AdminReason::RoleRescoped.sentence(),
            "the role was rescoped and will be re-advertised"
        );
        assert_eq!(
            AdminReason::FilledInternallyOrRedeployed.sentence(),
            "the position was filled internally or by redeployment"
        );
    }
}
