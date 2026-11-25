//! Property tests for the three invariants that hand-written examples cannot cover
//! (P2-1).
//!
//! 1. No send without a disposition: any application id, any candidate, empty
//!    disposition log → one block, zero sends.
//! 2. The chain stays linked under arbitrary appends: any sequence of subjects and
//!    bodies, reopened from disk, verifies and every `prev_hash` points backwards.
//! 3. Supersede never rewrites history: any correction leaves the earlier line
//!    byte-for-byte intact and appends exactly one record.
//!
//! Cases are deliberately few (24): every case fsyncs a workspace, and the point is
//! generated input, not volume. The whole file runs in a couple of seconds.

use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

use chrono::{DateTime, TimeZone, Utc};
use proptest::prelude::*;

use ledger::approval::SignOffRole;
use ledger::ats::Roster;
use ledger::digest::sha256_hex;
use ledger::disposition::{
    Disposition, DispositionRecord, Dispositions, FormSubmission, NewDisposition,
};
use ledger::gate::{GateAnswer, GateKind, GateLibrary};
use ledger::id::{
    ApplicationId, CandidateEmail, DecisionMakerId, DispositionId, Mailbox, RecruiterId,
    RequisitionId, TextVersion,
};
use ledger::log::{BlockReason, BlockedLog, DeliveryStatus, GENESIS_HASH, SendDraft, SendLog};
use ledger::pipeline::{Dispatch, Mode, dispatch};
use ledger::playbook::Playbook;
use ledger::policy::Policy;
use ledger::qa::QaLog;
use ledger::requisition::{Requisition, RequisitionBook, RequisitionDraft, Sender};
use ledger::store::{self, Store};
use ledger::template::{RenderedEmail, Templates};

const REQUISITION: &str = "REQ-PROP";

fn at(minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 24, 9, minute, 0)
        .single()
        .expect("timestamp")
}

/// A workspace with signed-off copy, one requisition and one imported applicant.
struct Rig {
    store: Store,
    dir: PathBuf,
    requisition: Requisition,
}

fn rig(tag: &str, application: &str, first_name: &str, email: &str) -> Rig {
    let dir = std::env::temp_dir().join(format!(
        "ledger-prop-{tag}-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let _ = fs::remove_dir_all(&dir);
    let store = Store::open(&dir).expect("workspace");

    GateLibrary::seed().save(&store).expect("gate library");
    Templates::seed().save(&store).expect("templates");
    Playbook::seed().save(&store).expect("playbook");
    let mut policy = Policy::default();
    policy
        .set_escalation_owner("M. Okafor, Employee Relations")
        .expect("owner");
    policy.save(&store).expect("policy");

    let mut requisition = Requisition::new(
        RequisitionDraft {
            id: RequisitionId::try_from(REQUISITION).expect("id"),
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
    requisition
        .set_comparative(
            [
                "assessment of deteriorating patients".to_owned(),
                "escalation to a registrar within their first hour".to_owned(),
                "documentation a covering clinician could act on".to_owned(),
            ],
            "a de-identified case note attached to the application",
            "a post-graduate qualification or a specific employer",
            TextVersion::parse("v1.0").expect("version"),
        )
        .expect("comparative");

    let mut library = GateLibrary::seed();
    let mut templates = Templates::seed();
    let mut playbook = Playbook::seed();
    for role in [SignOffRole::HiringManager, SignOffRole::Legal] {
        library.approve("J. Chen", role, at(1)).expect("library");
        templates
            .approve_all("M. Okafor", role, at(1))
            .expect("templates");
        playbook
            .approve("M. Okafor", role, at(1))
            .expect("playbook");
        requisition.approve("J. Chen", role, at(1)).expect("copy");
        requisition
            .approve_comparative("M. Okafor", role, at(1))
            .expect("paragraph");
    }
    library.save(&store).expect("save library");
    templates.save(&store).expect("save templates");
    playbook.save(&store).expect("save playbook");

    let mut book = RequisitionBook::default();
    book.insert(requisition.clone()).expect("insert");
    store
        .save(store::REQUISITIONS, &book)
        .expect("save requisitions");

    let csv = format!(
        "APPLICATION_ID,FIRST_NAME,EMAIL,REQ_ID,ROLE_TITLE,STAGE_AT_REJECTION,KO_ANSWER_01,KO_ANSWER_02,APPLIED_DATE\n\
         {application},{first_name},{email},{REQUISITION},Registered Nurse,Initial screen,,,2026-11-01\n"
    );
    let roster = Roster::from_csv(&requisition.id, Cursor::new(csv)).expect("roster");
    roster.save(&store).expect("save roster");

    Rig {
        store,
        dir,
        requisition,
    }
}

fn application_id() -> impl Strategy<Value = String> {
    "[A-Z]{2,4}-[0-9]{1,4}"
}

fn person() -> impl Strategy<Value = String> {
    "[A-Z][a-z]{2,8}"
}

fn mailbox_local() -> impl Strategy<Value = String> {
    "[a-z]{3,10}"
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, ..ProptestConfig::default() })]

    /// Invariant: a gate-blocking protocol with no captured decision sends nothing.
    #[test]
    fn proptest_no_send_without_a_disposition(
        application in application_id(),
        first_name in person(),
        local in mailbox_local(),
    ) {
        let email = format!("{local}@example.com");
        let rig = rig("missing", &application, &first_name, &email);
        let library = rig.store.load::<GateLibrary>(store::GATE_LIBRARY).expect("library").expect("seeded");
        let templates = rig.store.load::<Templates>(store::TEMPLATES).expect("templates").expect("seeded");
        let playbook = Playbook::load(&rig.store).expect("playbook").expect("seeded");
        let roster = Roster::load(&rig.store, &rig.requisition.id).expect("roster").expect("imported");
        let dispositions = Dispositions::open(&rig.store).expect("dispositions");
        let qa = QaLog::open(&rig.store).expect("qa");
        let mut send_log = SendLog::open(&rig.store).expect("send log");
        let mut blocked = BlockedLog::open(&rig.store).expect("blocked");

        let report = dispatch(
            &Dispatch {
                mode: Mode::Execute,
                requisition: &rig.requisition,
                library: &library,
                templates: &templates,
                playbook: &playbook,
                roster: &roster,
                dispositions: &dispositions,
                qa: &qa,
                delivery_status: DeliveryStatus::Sent,
                now: at(2),
            },
            &mut send_log,
            &mut blocked,
        )
        .expect("dispatch");

        prop_assert_eq!(report.sent_count(), 0);
        prop_assert_eq!(report.blocked_count(), 1);
        prop_assert!(matches!(
            report.blocked.first().map(|attempt| &attempt.reason),
            Some(BlockReason::MissingDisposition)
        ));
        prop_assert!(send_log.is_empty());
        prop_assert_eq!(SendLog::open(&rig.store).expect("reopen").entries().expect("entries").len(), 0);
        prop_assert_eq!(blocked.attempts().len(), 1);

        let _ = fs::remove_dir_all(&rig.dir);
    }

    /// Invariant: appending arbitrary rendered text keeps the chain linked.
    #[test]
    fn proptest_chain_stays_linked_under_arbitrary_appends(
        subjects in prop::collection::vec("[A-Za-z0-9 ]{1,40}", 0..6),
    ) {
        let rig = rig("chain", "APP-1", "Ada", "ada@example.com");
        let mut log = SendLog::open(&rig.store).expect("send log");
        for (index, subject) in subjects.iter().enumerate() {
            log.append(
                SendDraft {
                    disposition: disposition(),
                    text_version: TextVersion::parse("v1.0").expect("version"),
                    rendered: RenderedEmail {
                        subject: subject.clone(),
                        body: format!("Dear Ada,\nthe department said:\n{subject}"),
                    },
                    delivery_status: DeliveryStatus::Sent,
                },
                at(2 + u32::try_from(index).expect("index")),
            )
            .expect("append");
        }

        let entries = SendLog::open(&rig.store).expect("reopen").entries().expect("entries");
        prop_assert_eq!(entries.len(), subjects.len());
        for (index, entry) in entries.iter().enumerate() {
            prop_assert_eq!(entry.seq, index as u64);
            let expected_prev = if index == 0 {
                GENESIS_HASH.to_owned()
            } else {
                entries
                    .get(index - 1)
                    .map(|previous| previous.entry_hash.clone())
                    .expect("previous entry")
            };
            prop_assert_eq!(&entry.prev_hash, &expected_prev);
            prop_assert_eq!(
                entry.entry_hash.clone(),
                sha256_hex(entry.canonical_payload().as_bytes())
            );
        }
        let verification = log.verification().expect("verifies");
        prop_assert_eq!(verification.entries, subjects.len());
        let expected_head = entries
            .last()
            .map_or_else(|| GENESIS_HASH.to_owned(), |entry| entry.entry_hash.clone());
        prop_assert_eq!(verification.head, expected_head);

        let _ = fs::remove_dir_all(&rig.dir);
    }

    /// Invariant: a correction appends; it never edits the record it supersedes.
    #[test]
    fn proptest_supersede_never_rewrites_history(
        recruiter in "[a-z]{3,10}",
        decision_maker in "[a-z]{3,10}",
    ) {
        let rig = rig("supersede", "APP-1", "Ada", "ada@example.com");
        let path = rig.store.path(store::DISPOSITIONS);

        let first = Dispositions::open(&rig.store)
            .expect("dispositions")
            .record(field(&rig, "rec-rui", "dm-chen", None))
            .expect("captured")
            .clone();
        let bytes_before = fs::read(&path).expect("read");

        let second = Dispositions::open(&rig.store)
            .expect("dispositions")
            .record(field(
                &rig,
                &recruiter,
                &decision_maker,
                Some(first.id.clone()),
            ))
            .expect("correction")
            .clone();
        let bytes_after = fs::read(&path).expect("read");

        prop_assert!(
            bytes_after.starts_with(&bytes_before),
            "the correction rewrote an earlier line"
        );
        prop_assert!(bytes_after.len() > bytes_before.len());

        let reopened = Dispositions::open(&rig.store).expect("dispositions");
        let history = reopened.history(&first.form.application_id);
        prop_assert_eq!(history.len(), 2);
        prop_assert_eq!(history.first().map(|record| &record.id), Some(&first.id));
        prop_assert_eq!(history.get(1).map(|record| &record.id), Some(&second.id));
        prop_assert_eq!(
            history.get(1).and_then(|record| record.form.supersedes.as_ref()),
            Some(&first.id)
        );
        prop_assert!(history.iter().all(|record| record.locked));
        prop_assert_eq!(
            history.first().map(|record| record.form.candidate_first_name.as_str()),
            Some("Ada")
        );

        let _ = fs::remove_dir_all(&rig.dir);
    }
}

/// The form a FIELD disposition needs, for a fixed applicant.
fn field(
    rig: &Rig,
    recruiter: &str,
    decision_maker: &str,
    supersedes: Option<DispositionId>,
) -> NewDisposition {
    let library = rig
        .store
        .load::<GateLibrary>(store::GATE_LIBRARY)
        .expect("library")
        .expect("seeded");
    NewDisposition::try_new(
        &rig.requisition,
        &library,
        FormSubmission {
            application_id: ApplicationId::try_from("APP-1").expect("application"),
            candidate_first_name: "Ada".to_owned(),
            candidate_email: CandidateEmail::try_from("ada@example.com").expect("email"),
            recruiter: RecruiterId::try_from(recruiter).expect("recruiter"),
            decision_maker: DecisionMakerId::try_from(decision_maker).expect("decision maker"),
            disposition: Disposition::Field,
            supersedes,
        },
        at(1),
    )
    .expect("form")
}

/// One send-log record, independent of the import path.
fn disposition() -> DispositionRecord {
    let requisition = Requisition::new(
        RequisitionDraft {
            id: RequisitionId::try_from(REQUISITION).expect("id"),
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
        &GateLibrary::seed(),
        FormSubmission {
            application_id: ApplicationId::try_from("APP-1").expect("application"),
            candidate_first_name: "Ada".to_owned(),
            candidate_email: CandidateEmail::try_from("ada@example.com").expect("email"),
            recruiter: RecruiterId::try_from("rec-rui").expect("recruiter"),
            decision_maker: DecisionMakerId::try_from("dm-chen").expect("decision maker"),
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
