//! The deterministic demo fixture (V3 Phase 1).
//!
//! `ledger demo seed` builds a complete pilot state for `REQ-2026-014` from fixed
//! inputs: every timestamp, name, email, and disposition is derived from constants,
//! so two runs produce byte-identical files and identical hash chains.
//!
//! The fixture plants the five failure cases V3 asks for:
//!
//! 1. one application with no disposition record (form-block);
//! 2. one `GATE` disposition whose `GATE_CODE` contradicts its `KO_ANSWER_02`;
//! 3. one send entry left on a superseded `TEXT_VERSION` after a version bump;
//! 4. one sampled `FIELD` application with no second-person QA record;
//! 5. one row past the initial screen, dropped at import.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Duration, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::approval::SignOffRole;
use crate::ats::Roster;
use crate::digest::sha256_hex;
use crate::disposition::{
    AdminReason, Disposition, DispositionKind, Dispositions, FormSubmission, NewDisposition,
};
use crate::error::{Error, Result};
use crate::gate::{FactKey, GateAnswer, GateKind, GateLibrary};
use crate::id::{DecisionMakerId, Mailbox, QaId, RecruiterId, RequisitionId, TextVersion};
use crate::log::{BlockReason, BlockedLog, DeliveryStatus, SendLog};
use crate::pipeline::{Dispatch, Mode, dispatch};
use crate::playbook::Playbook;
use crate::policy::Policy;
use crate::qa::{QaLog, QaRecord, QaScope, sampled};
use crate::requisition::{Requisition, RequisitionBook, RequisitionDraft, Sender};
use crate::store::{self, Store};
use crate::template::Templates;

/// The demo requisition id.
pub const PILOT_REQUISITION: &str = "REQ-2026-014";

/// The first name of the candidate whose `GATE-02` answer contradicts the ATS.
pub const GATE_CSV_CONTRADICTION_APP: &str = "APP-1012";

/// The candidate with no disposition record.
pub const MISSING_DISPOSITION_APP: &str = "APP-1150";

/// The row past the initial screen, dropped at import.
pub const POST_SCREEN_APP: &str = "APP-1999";

/// The applicant count in the fixture (initial screen only).
pub const ROSTER_ROWS: usize = 150;

/// Disposition ranges, as half-open row ranges: GATE-01 rows, GATE-02 rows
/// (one valid, one contradicting its ATS answer), FIELD rows, ADMIN rows.
const GATE01: std::ops::Range<usize> = 0..10;
/// `GATE-02` rows: the valid send and the contradiction.
const GATE02: std::ops::Range<usize> = 10..12;
/// `FIELD` rows.
const FIELD: std::ops::Range<usize> = 12..141;
/// `ADMIN` rows.
const ADMIN: std::ops::Range<usize> = 141..149;

/// First names cycled through the fixture, so emails and copies stay deterministic.
const FIRST_NAMES: [&str; 20] = [
    "Ada",
    "Grace",
    "Alan",
    "Katherine",
    "Edsger",
    "Barbara",
    "Linus",
    "Margaret",
    "Dennis",
    "Frances",
    "Ken",
    "Radia",
    "Tim",
    "Shafi",
    "Vint",
    "Anita",
    "Leslie",
    "Jean",
    "Donald",
    "Sophie",
];

/// What the seed produced, for the CLI and the tests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DemoSummary {
    /// The requisition the fixture targets.
    pub requisition: String,
    /// Rows in the generated ATS export, including the dropped row.
    pub csv_rows: usize,
    /// Applicants that survived the initial-screen filter.
    pub roster_rows: usize,
    /// Rows the hard filter dropped.
    pub dropped_at_import: usize,
    /// Dispositions by kind.
    pub dispositions: BTreeMap<DispositionKind, usize>,
    /// Emails a preview reports before dispatch.
    pub preview_sends: usize,
    /// Attempts a preview reports blocked before dispatch.
    pub preview_blocked: usize,
    /// Whether the fixture also committed the sends (`--record`).
    pub recorded: bool,
    /// Emails committed to the send log (0 for a pre-dispatch seed).
    pub sends: usize,
    /// QA checks recorded.
    pub qa_checks: usize,
    /// Attempts refused by the circuit-breakers, once recorded.
    pub blocked: usize,
    /// Reasons the fixture intends to produce, in order.
    pub blocked_reasons: Vec<String>,
    /// Send entries whose `TEXT_VERSION` is now superseded.
    pub stale_versions: usize,
    /// The head hash of the send log.
    pub log_head: String,
    /// SHA-256 over every workspace file, for determinism checks.
    pub fingerprint: String,
    /// Where the generated CSV lives.
    pub csv_path: PathBuf,
}

/// Seeds the demo workspace.
///
/// # Errors
///
/// Returns [`Error::Invalid`] when the workspace already holds a pilot and
/// `force` is false, and propagates any write or validation error.
pub fn seed(store: &Store, force: bool, record: bool) -> Result<DemoSummary> {
    let base = base_time();
    if !force && has_state(store)? {
        return Err(Error::invalid(
            "workspace",
            format!(
                "{} already holds a pilot; pass --force to reseed it",
                store.root().display()
            ),
        ));
    }
    reset(store)?;

    // Phase 1 governance: named owner, one-business-day SLA, retention on record.
    let mut policy = Policy::default();
    policy.set_escalation_owner("M. Okafor, Employee Relations")?;
    policy.set_retention_days(2_557)?;
    policy.set_escalation_sla_hours(24)?;
    policy.save(store)?;

    // Single-pass sign-off of all four static artefacts.
    let mut library = GateLibrary::seed();
    let mut templates = Templates::seed();
    let mut playbook = Playbook::seed();
    for role in [SignOffRole::HiringManager, SignOffRole::Legal] {
        library.approve("J. Chen", role, base)?;
        templates.approve_all("M. Okafor", role, base)?;
        playbook.approve("M. Okafor", role, base)?;
    }
    library.save(store)?;
    templates.save(store)?;
    playbook.save(store)?;

    // The requisition, with every gate's mandatory facts documented before posting.
    let mut requisition = Requisition::new(
        RequisitionDraft {
            id: RequisitionId::try_from(PILOT_REQUISITION)?,
            employer: "Example Health".to_owned(),
            title: "Registered Nurse".to_owned(),
            reply_to: Mailbox::try_from("careers-feedback@example.com.au")?,
            sender: Sender::try_new("R. Patel", "Recruitment Coordinator")?,
            gates: GateKind::ALL.to_vec(),
            facts: facts(),
        },
        base,
    )?;
    requisition.set_comparative(
        [
            "assessment of a deteriorating patient".to_owned(),
            "escalation to a registrar within their first hour".to_owned(),
            "documentation a covering clinician could act on".to_owned(),
        ],
        "a de-identified case note attached to the application",
        "a post-graduate qualification or a specific employer",
        TextVersion::parse("v1.0")?,
    )?;
    for role in [SignOffRole::HiringManager, SignOffRole::Legal] {
        requisition.approve("J. Chen", role, base)?;
        requisition.approve_comparative("M. Okafor", role, base)?;
    }
    let mut book = RequisitionBook::default();
    book.insert(requisition.clone())?;
    store.save(store::REQUISITIONS, &book)?;

    // The ATS export, written to the workspace and then imported.
    let csv = render_csv();
    let csv_path = store.path("demo-ats.csv");
    fs::write(&csv_path, &csv).map_err(|source| Error::Io {
        path: csv_path.clone(),
        source,
    })?;
    let roster = Roster::from_csv(&requisition.id, csv.as_bytes())?;
    if roster.applicants.len() != ROSTER_ROWS {
        return Err(Error::invalid(
            "demo fixture",
            format!(
                "expected {ROSTER_ROWS} roster rows, built {}",
                roster.applicants.len()
            ),
        ));
    }
    roster.save(store)?;

    // Dispositions: 12 GATE, 129 FIELD, 8 ADMIN, 1 missing.
    let mut dispositions = Dispositions::open(store)?;
    let mut dispositions_by_kind: BTreeMap<DispositionKind, usize> = BTreeMap::new();
    for (index, applicant) in roster.applicants.iter().enumerate() {
        let Some(disposition) = disposition_for(index) else {
            continue;
        };
        let form = NewDisposition::try_new(
            &requisition,
            &library,
            FormSubmission {
                application_id: applicant.application_id.clone(),
                candidate_first_name: applicant.first_name.clone(),
                candidate_email: applicant.email.clone(),
                recruiter: RecruiterId::try_from("rec-rui")?,
                decision_maker: DecisionMakerId::try_from("dm-chen")?,
                disposition,
                supersedes: None,
            },
            base + Duration::minutes(index as i64),
        )?;
        dispositions.record(form)?;
        *dispositions_by_kind
            .entry(disposition_kind(index))
            .or_insert(0) += 1;
    }

    // QA: every GATE row, plus the deterministic 10% sample, minus one gap.
    let mut qa = QaLog::open(store)?;
    let mut gap_used = false;
    for applicant in &roster.applicants {
        let Some(record) = dispositions.head(&requisition.id, &applicant.application_id) else {
            continue;
        };
        let is_gate = matches!(record.form.disposition, Disposition::Gate { .. });
        if !is_gate && !sampled(&applicant.application_id) {
            continue;
        }
        if !is_gate && !gap_used {
            // Failure case 4: a sampled FIELD row with no second-person check.
            gap_used = true;
            continue;
        }
        qa.record(QaRecord {
            application_id: applicant.application_id.clone(),
            requisition: requisition.id.clone(),
            qa: QaId::try_from("qa-patel")?,
            at: base + Duration::hours(2),
            scope: if is_gate {
                QaScope::GateFullCheck
            } else {
                QaScope::RandomTenPercent
            },
        })?;
    }

    // Preview writes nothing, so the walkthrough always starts here.
    let mut send_log = SendLog::open(store)?;
    let mut blocked_log = BlockedLog::open(store)?;
    let preview = dispatch(
        &Dispatch {
            mode: Mode::Preview,
            requisition: &requisition,
            library: &library,
            templates: &templates,
            playbook: &playbook,
            roster: &roster,
            dispositions: &dispositions,
            qa: &qa,
            delivery_status: DeliveryStatus::Sent,
            now: base + Duration::hours(3),
        },
        &mut send_log,
        &mut blocked_log,
    )?;
    let blocked_reasons: Vec<String> = preview
        .blocked
        .iter()
        .map(|attempt| block_label(&attempt.reason))
        .collect();
    let preview_sends = preview.preview_count();
    let preview_blocked = preview.blocked_count();

    let (sends, blocked, stale_versions) = if record {
        let report = dispatch(
            &Dispatch {
                mode: Mode::Execute,
                requisition: &requisition,
                library: &library,
                templates: &templates,
                playbook: &playbook,
                roster: &roster,
                dispositions: &dispositions,
                qa: &qa,
                delivery_status: DeliveryStatus::Sent,
                now: base + Duration::hours(4),
            },
            &mut send_log,
            &mut blocked_log,
        )?;
        // Failure case 3: bump GATE-02 after dispatch, so one entry stays on v1.0.
        library.bump_version(GateKind::WorkRightsDuration, TextVersion::parse("v1.1")?)?;
        for role in [SignOffRole::HiringManager, SignOffRole::Legal] {
            library.approve("J. Chen", role, base + Duration::hours(5))?;
        }
        library.save(store)?;
        let stale = send_log
            .entries()?
            .iter()
            .filter(|entry| {
                entry.text_version.as_str() == "v1.0"
                    && entry.gate_code == Some(GateKind::WorkRightsDuration)
            })
            .count();
        (report.sent_count(), report.blocked_count(), stale)
    } else {
        (0, 0, 0)
    };

    let summary = DemoSummary {
        requisition: PILOT_REQUISITION.to_owned(),
        csv_rows: roster.applicants.len() + roster.excluded_not_initial_screen,
        roster_rows: roster.applicants.len(),
        dropped_at_import: roster.excluded_not_initial_screen,
        dispositions: dispositions_by_kind,
        preview_sends,
        preview_blocked,
        recorded: record,
        sends,
        qa_checks: qa.count_for(&requisition.id),
        blocked,
        blocked_reasons,
        stale_versions,
        log_head: send_log.verification()?.head,
        fingerprint: String::new(),
        csv_path,
    };
    store.save(store::DEMO_SUMMARY, &summary)?;
    let fingerprint = fingerprint(store)?;
    let summary = DemoSummary {
        fingerprint,
        ..summary
    };
    store.save(store::DEMO_SUMMARY, &summary)?;
    Ok(summary)
}

/// Loads the stored fixture summary.
///
/// # Errors
///
/// Returns [`Error::Invalid`] when the workspace has no fixture.
pub fn summary(store: &Store) -> Result<DemoSummary> {
    store.load(store::DEMO_SUMMARY)?.ok_or_else(|| {
        Error::invalid(
            "workspace",
            "no demo fixture; run `ledger demo seed --data <dir>` first",
        )
    })
}

/// The fixed clock the fixture derives every timestamp from.
fn base_time() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 24, 9, 0, 0)
        .single()
        .expect("the fixture clock is a valid instant")
}

/// The requisition facts `GATE-02`..`GATE-05` need.
fn facts() -> BTreeMap<FactKey, String> {
    BTreeMap::from([
        (FactKey::RoleEndOrMinTerm, "30 June 2027".to_owned()),
        (FactKey::MinTerm, "12 months".to_owned()),
        (
            FactKey::RoleDurationReason,
            "The appointment is a fixed-term 12-month contract.".to_owned(),
        ),
        (FactKey::RegulatorUrl, "https://www.ahpra.gov.au".to_owned()),
        (
            FactKey::InherentRequirementReason,
            "the role requires unaccompanied travel between three clinic sites where no public \
             transport operates"
                .to_owned(),
        ),
        (
            FactKey::LocationRequirement,
            "the Townsville clinic".to_owned(),
        ),
        (
            FactKey::LocationSatisfactionText,
            "residence within 45 minutes of the Townsville clinic, or a confirmed relocation date"
                .to_owned(),
        ),
        (
            FactKey::WaiverText,
            "relocation within 90 days of the start date".to_owned(),
        ),
    ])
}

/// Builds the 151-row ATS export deterministically.
fn render_csv() -> String {
    let mut out = String::from(
        "APPLICATION_ID,FIRST_NAME,EMAIL,REQ_ID,ROLE_TITLE,STAGE_AT_REJECTION,\
         KO_ANSWER_01,KO_ANSWER_02,KO_ANSWER_03,KO_ANSWER_04,KO_ANSWER_05,APPLIED_DATE\n",
    );
    for index in 0..ROSTER_ROWS {
        let application = format!("APP-{}", 1001 + index);
        let first_name = FIRST_NAMES[index % FIRST_NAMES.len()];
        let email = format!("app{}@example.com", 1001 + index);
        let (ko1, ko2) = match index {
            i if GATE01.contains(&i) => ("not-held", ""),
            10 => ("", "shorter-than-role"),
            // Failure case 2: the ATS says restricted, the form will say shorter.
            11 => ("", "restricted"),
            _ => ("", ""),
        };
        let applied = format!("2026-11-{:02}", 1 + (index % 20));
        out.push_str(&format!(
            "{application},{first_name},{email},{PILOT_REQUISITION},Registered Nurse,Initial \
             screen,{ko1},{ko2},,,,{applied}\n"
        ));
    }
    // Failure case 5: a row past the initial screen.
    out.push_str(&format!(
        "{POST_SCREEN_APP},Mary,mary@example.com,{PILOT_REQUISITION},Registered \
         Nurse,Interviewed,,,,,2026-11-21\n"
    ));
    out
}

/// The disposition each fixture row receives, or `None` for the missing record.
fn disposition_for(index: usize) -> Option<Disposition> {
    Some(match index {
        i if GATE01.contains(&i) => Disposition::Gate {
            gate: GateKind::RightToWork,
            answer: GateAnswer::NotHeld,
        },
        // 10 is the valid GATE-02 send that later goes stale on v1.0; 11 is
        // failure case 2, contradicting the ATS answer of `restricted`.
        i if GATE02.contains(&i) => Disposition::Gate {
            gate: GateKind::WorkRightsDuration,
            answer: GateAnswer::ShorterThanRole,
        },
        i if FIELD.contains(&i) => Disposition::Field,
        i if ADMIN.contains(&i) => Disposition::Admin {
            reason: match i % 3 {
                0 => AdminReason::RequisitionWithdrawn,
                1 => AdminReason::RoleRescoped,
                _ => AdminReason::FilledInternallyOrRedeployed,
            },
        },
        // Failure case 1: no disposition at all.
        _ => return None,
    })
}

/// The kind of the disposition at `index`.
fn disposition_kind(index: usize) -> DispositionKind {
    match index {
        i if GATE01.contains(&i) || GATE02.contains(&i) => DispositionKind::Gate,
        i if FIELD.contains(&i) => DispositionKind::Field,
        _ => DispositionKind::Admin,
    }
}

/// A stable label for a block reason.
fn block_label(reason: &BlockReason) -> String {
    match reason {
        BlockReason::MissingDisposition => "missing-disposition".to_owned(),
        BlockReason::GateUnavailable { gate } => format!("gate-unavailable:{gate}"),
        BlockReason::AnswerMismatch { gate, .. } => format!("answer-mismatch:{gate}"),
        BlockReason::EmailMismatch { .. } => "email-mismatch".to_owned(),
        BlockReason::AwaitingSecondPersonQa { scope } => {
            format!("awaiting-qa:{}", scope.code())
        }
        BlockReason::GateNotInRequisition { gate } => format!("gate-not-in-requisition:{gate}"),
    }
}

/// Whether the workspace already holds a pilot.
fn has_state(store: &Store) -> Result<bool> {
    for name in STATE_FILES {
        if fs::metadata(store.path(name)).is_ok() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The files the seed owns; `--force` removes exactly these.
const STATE_FILES: [&str; 15] = [
    store::GATE_LIBRARY,
    store::TEMPLATES,
    store::PLAYBOOK,
    store::POLICY,
    store::REQUISITIONS,
    store::DISPOSITIONS,
    store::SEND_LOG,
    store::BLOCKED_LOG,
    store::QA_LOG,
    store::REPLIES,
    store::SIGNALS,
    store::REVIEW,
    store::CHECKLIST,
    store::DEMO_SUMMARY,
    "demo-ats.csv",
];

/// Removes the seed's files, including the roster.
fn reset(store: &Store) -> Result<()> {
    for name in STATE_FILES {
        remove_if_present(&store.path(name))?;
    }
    let root = store.root().to_path_buf();
    let entries = fs::read_dir(&root).map_err(|source| Error::Io {
        path: root.clone(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| Error::Io {
            path: root.clone(),
            source,
        })?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("roster-") && name.ends_with(".json") {
            remove_if_present(&entry.path())?;
        }
    }
    Ok(())
}

fn remove_if_present(path: &std::path::Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(Error::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// SHA-256 over every file in the workspace, name-sorted and content-hashed.
pub fn fingerprint(store: &Store) -> Result<String> {
    let mut files: Vec<_> = fs::read_dir(store.root())
        .map_err(|source| Error::Io {
            path: store.root().to_path_buf(),
            source,
        })?
        .map(|entry| {
            entry.map(|entry| entry.path()).map_err(|source| Error::Io {
                path: store.root().to_path_buf(),
                source,
            })
        })
        .collect::<Result<_>>()?;
    files.sort();

    let mut combined = String::new();
    for path in files {
        if !path.is_file() {
            continue;
        }
        if path
            .file_name()
            .is_some_and(|name| name == store::DEMO_SUMMARY)
        {
            continue;
        }
        let bytes = fs::read(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        combined.push_str(
            &path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default(),
        );
        combined.push(':');
        combined.push_str(&sha256_hex(&bytes));
        combined.push('\n');
    }
    Ok(sha256_hex(combined.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::SendLog;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ledger-demo-{tag}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_seed_is_byte_identical_across_runs() {
        let first = temp_dir("a");
        let second = temp_dir("b");
        let one = seed(&Store::open(&first).expect("a"), false, true).expect("seed a");
        let two = seed(&Store::open(&second).expect("b"), false, true).expect("seed b");
        assert_eq!(one.fingerprint, two.fingerprint);
        assert_eq!(one.log_head, two.log_head);
        std::fs::remove_dir_all(&first).expect("cleanup");
        std::fs::remove_dir_all(&second).expect("cleanup");
    }

    #[test]
    fn the_seed_plants_the_five_failure_cases() {
        let dir = temp_dir("cases");
        let store = Store::open(&dir).expect("workspace");
        let summary = seed(&store, false, true).expect("seed");

        assert_eq!(summary.csv_rows, 151);
        assert_eq!(summary.roster_rows, 150);
        assert_eq!(summary.dropped_at_import, 1);
        assert_eq!(summary.dispositions.get(&DispositionKind::Gate), Some(&12));
        assert_eq!(
            summary.dispositions.get(&DispositionKind::Field),
            Some(&129)
        );
        assert_eq!(summary.dispositions.get(&DispositionKind::Admin), Some(&8));
        assert_eq!(summary.preview_sends, 147);
        assert_eq!(summary.preview_blocked, 3);
        assert_eq!(summary.sends, 147);
        assert_eq!(summary.blocked, 3);
        assert!(summary.recorded);
        assert_eq!(
            summary.blocked_reasons,
            vec![
                "answer-mismatch:GATE-02".to_owned(),
                "awaiting-qa:random-ten-percent".to_owned(),
                "missing-disposition".to_owned(),
            ]
        );
        assert_eq!(summary.stale_versions, 1, "one GATE-02 send stays on v1.0");

        let log = SendLog::open(&store).expect("log");
        assert_eq!(log.verification().expect("chain").entries, 147);
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn a_plain_seed_stops_before_dispatch() {
        let dir = temp_dir("preview");
        let store = Store::open(&dir).expect("workspace");
        let seeded = seed(&store, false, false).expect("seed");
        assert!(!seeded.recorded);
        assert_eq!(seeded.sends, 0);
        assert_eq!(seeded.blocked, 0);
        assert_eq!(seeded.preview_sends, 147);
        assert_eq!(seeded.preview_blocked, 3);
        assert!(SendLog::open(&store).expect("log").is_empty());
        assert_eq!(
            summary(&store).expect("stored summary").fingerprint,
            seeded.fingerprint
        );
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn reseeding_requires_force_and_stays_deterministic() {
        let dir = temp_dir("force");
        let store = Store::open(&dir).expect("workspace");
        let first = seed(&store, false, true).expect("first seed");
        assert!(
            seed(&store, false, true).is_err(),
            "a second seed must refuse without --force"
        );
        let second = seed(&store, true, true).expect("forced reseed");
        assert_eq!(first.fingerprint, second.fingerprint);
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }
}
