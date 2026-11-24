//! The locked disposition form (Task 2.1).
//!
//! A disposition is one of exactly three shapes. There is no free-text field and
//! no "Other" variant: the enum is the whole vocabulary, so the "no free text"
//! constraint cannot be violated by a caller. Captured records are append-only and
//! carry their own timestamp, so they cannot be edited after the fact.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::gate::{GateAnswer, GateKind};
use crate::id::{ApplicationId, CandidateEmail, RecruiterId, RequisitionId, TemplateKind};
use crate::jsonl::AppendLog;
use crate::requisition::Requisition;
use crate::store::{self, Store};
use crate::text;

/// Why an application was closed for an administrative reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClosureReason {
    /// The role changed after the advertisement went out.
    RoleChange,
    /// The position was filled by an internal candidate.
    InternalHire,
    /// The requisition was withdrawn.
    RequisitionWithdrawn,
}

impl ClosureReason {
    /// Every reason the form offers.
    pub const ALL: [Self; 3] = [
        Self::RoleChange,
        Self::InternalHire,
        Self::RequisitionWithdrawn,
    ];

    /// The stable code used in storage and on the command line.
    pub fn code(self) -> &'static str {
        match self {
            Self::RoleChange => "role-change",
            Self::InternalHire => "internal-hire",
            Self::RequisitionWithdrawn => "requisition-withdrawn",
        }
    }

    /// The clause the administrative email quotes.
    pub fn phrase(self) -> &'static str {
        match self {
            Self::RoleChange => "the role changed after the advertisement was published",
            Self::InternalHire => "the position was filled by an internal candidate",
            Self::RequisitionWithdrawn => "the requisition was withdrawn",
        }
    }
}

impl fmt::Display for ClosureReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl FromStr for ClosureReason {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        let needle = value.trim().to_lowercase();
        Self::ALL
            .into_iter()
            .find(|reason| reason.code() == needle)
            .ok_or_else(|| {
                Error::invalid(
                    "closure reason",
                    format!(
                        "expected one of role-change, internal-hire, requisition-withdrawn; \
                         got {value:?}"
                    ),
                )
            })
    }
}

/// The exact disposition category captured on the form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "category", rename_all = "kebab-case")]
pub enum DispositionCategory {
    /// The candidate failed a stated gate; the gate must be one the requisition
    /// screens on, and the answer must be one the gate offers.
    FailedGate {
        /// Which knockout gate was failed.
        gate: GateKind,
        /// The candidate's own answer to that knockout question.
        answer: GateAnswer,
    },
    /// The candidate met the bar and a stronger field was appointed.
    MetTheBarStrongerField,
    /// The requisition was withdrawn, changed, or filled internally.
    AdministrativeClosure {
        /// Why the application was closed.
        reason: ClosureReason,
    },
}

impl DispositionCategory {
    /// The email template this category uses.
    pub fn template_kind(&self) -> TemplateKind {
        match self {
            Self::FailedGate { .. } => TemplateKind::FailedGate,
            Self::MetTheBarStrongerField => TemplateKind::Comparative,
            Self::AdministrativeClosure { .. } => TemplateKind::Administrative,
        }
    }

    /// The stable category code used in reports and logs.
    pub fn code(&self) -> &'static str {
        match self {
            Self::FailedGate { .. } => "failed-gate",
            Self::MetTheBarStrongerField => "met-the-bar-stronger-field",
            Self::AdministrativeClosure { .. } => "administrative-closure",
        }
    }

    /// Checks the category against the requisition it was captured for.
    ///
    /// # Errors
    ///
    /// Returns [`Error::GateNotInRequisition`] when the gate is not one of the
    /// requisition's knockouts, and [`Error::DisallowedAnswer`] when the answer is
    /// not one the gate's conditional dropdown offers.
    pub fn validate_against(&self, requisition: &Requisition) -> Result<()> {
        let Self::FailedGate { gate, answer } = self else {
            return Ok(());
        };
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
        Ok(())
    }
}

impl fmt::Display for DispositionCategory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FailedGate { gate, answer } => write!(formatter, "failed-gate:{gate}:{answer}"),
            Self::MetTheBarStrongerField => formatter.write_str(self.code()),
            Self::AdministrativeClosure { reason } => {
                write!(formatter, "administrative-closure:{reason}")
            }
        }
    }
}

/// One completed disposition form submission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispositionRecord {
    /// The ATS application id, which joins the roster to the form.
    pub application_id: ApplicationId,
    /// The candidate's email, as captured on the form.
    pub candidate_email: CandidateEmail,
    /// The candidate's name, as captured on the form.
    pub candidate_name: String,
    /// The requisition the application is for.
    pub requisition: RequisitionId,
    /// The recruiter who captured the form.
    pub recruiter: RecruiterId,
    /// When the recruiter submitted the form. This is the timestamp of record.
    pub captured_at: DateTime<Utc>,
    /// The exact category selected.
    pub category: DispositionCategory,
}

impl DispositionRecord {
    /// Builds a record and checks it against its requisition.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] when the name is blank or over long, and
    /// propagates [`DispositionCategory::validate_against`] errors.
    pub fn try_new(
        requisition: &Requisition,
        application_id: ApplicationId,
        candidate_email: CandidateEmail,
        candidate_name: impl Into<String>,
        recruiter: RecruiterId,
        captured_at: DateTime<Utc>,
        category: DispositionCategory,
    ) -> Result<Self> {
        let record = Self {
            application_id,
            candidate_email,
            candidate_name: text::required("candidate name", candidate_name.into(), 120)?,
            requisition: requisition.id.clone(),
            recruiter,
            captured_at,
            category,
        };
        record.category.validate_against(requisition)?;
        Ok(record)
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

    /// Captures one disposition. Repeated capture for the same application is
    /// refused, because the form is completed once, at the moment of rejection.
    ///
    /// # Errors
    ///
    /// Returns [`Error::DuplicateDisposition`] when the application already has a
    /// record, and propagates write errors.
    pub fn record(&mut self, record: DispositionRecord) -> Result<&DispositionRecord> {
        if self
            .find(&record.requisition, &record.application_id)
            .is_some()
        {
            return Err(Error::DuplicateDisposition {
                requisition: record.requisition.clone(),
                application: record.application_id.clone(),
            });
        }
        self.log.append(&record)?;
        self.records.push(record);
        self.records
            .last()
            .ok_or_else(|| Error::invalid("disposition", "captured record disappeared"))
    }

    /// Returns the captured record for one application, if any.
    pub fn find(
        &self,
        requisition: &RequisitionId,
        application: &ApplicationId,
    ) -> Option<&DispositionRecord> {
        self.records.iter().find(|record| {
            &record.requisition == requisition && &record.application_id == application
        })
    }

    /// Every captured record, in capture order.
    pub fn records(&self) -> &[DispositionRecord] {
        &self.records
    }

    /// How many dispositions have been captured for `requisition`.
    pub fn count_for(&self, requisition: &RequisitionId) -> usize {
        self.records
            .iter()
            .filter(|record| &record.requisition == requisition)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate::GateKind;
    use crate::id::Mailbox;
    use chrono::TimeZone;

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 1, 9, 0, 0)
            .single()
            .expect("valid timestamp")
    }

    fn requisition() -> Requisition {
        Requisition::new(
            RequisitionId::try_from("REQ-1").expect("id"),
            "Example Health",
            "Registered Nurse",
            Mailbox::try_from("careers-feedback@example.com.au").expect("mailbox"),
            [GateKind::VisaRestrictions],
            at(),
        )
        .expect("valid requisition")
    }

    fn record(category: DispositionCategory) -> DispositionRecord {
        DispositionRecord::try_new(
            &requisition(),
            ApplicationId::try_from("APP-1").expect("application id"),
            CandidateEmail::try_from("ada@example.com").expect("email"),
            "Ada Lovelace",
            RecruiterId::try_from("rec-rui").expect("recruiter"),
            at(),
            category,
        )
        .expect("valid record")
    }

    #[test]
    fn the_three_categories_map_to_the_three_templates() {
        assert_eq!(
            DispositionCategory::FailedGate {
                gate: GateKind::RightToWork,
                answer: GateAnswer::NotHeld
            }
            .template_kind(),
            TemplateKind::FailedGate
        );
        assert_eq!(
            DispositionCategory::MetTheBarStrongerField.template_kind(),
            TemplateKind::Comparative
        );
        assert_eq!(
            DispositionCategory::AdministrativeClosure {
                reason: ClosureReason::InternalHire
            }
            .template_kind(),
            TemplateKind::Administrative
        );
    }

    #[test]
    fn a_gate_the_requisition_does_not_screen_on_is_rejected() {
        let result = DispositionRecord::try_new(
            &requisition(),
            ApplicationId::try_from("APP-2").expect("application id"),
            CandidateEmail::try_from("grace@example.com").expect("email"),
            "Grace Hopper",
            RecruiterId::try_from("rec-rui").expect("recruiter"),
            at(),
            DispositionCategory::FailedGate {
                gate: GateKind::Licence,
                answer: GateAnswer::NotHeld,
            },
        );
        assert!(matches!(result, Err(Error::GateNotInRequisition { .. })));
    }

    #[test]
    fn an_answer_the_gate_does_not_offer_is_rejected() {
        let result = DispositionRecord::try_new(
            &requisition(),
            ApplicationId::try_from("APP-3").expect("application id"),
            CandidateEmail::try_from("katherine@example.com").expect("email"),
            "Katherine Johnson",
            RecruiterId::try_from("rec-rui").expect("recruiter"),
            at(),
            DispositionCategory::FailedGate {
                gate: GateKind::VisaRestrictions,
                answer: GateAnswer::OutsideRegion,
            },
        );
        assert!(matches!(result, Err(Error::DisallowedAnswer { .. })));
    }

    #[test]
    fn capture_is_append_only_and_single_shot() {
        let dir = std::env::temp_dir().join(format!("ledger-disp-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::open(&dir).expect("workspace");
        let mut dispositions = Dispositions::open(&store).expect("log");
        dispositions
            .record(record(DispositionCategory::MetTheBarStrongerField))
            .expect("first capture");
        let duplicate = dispositions.record(record(DispositionCategory::MetTheBarStrongerField));
        assert!(matches!(duplicate, Err(Error::DuplicateDisposition { .. })));

        let reopened = Dispositions::open(&store).expect("reopen");
        assert_eq!(
            reopened.count_for(&RequisitionId::try_from("REQ-1").expect("id")),
            1
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
