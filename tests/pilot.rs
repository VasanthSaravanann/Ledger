//! End-to-end pilot run against the public API.
//!
//! Each test drives the sequence V2 describes: sign off the static copy, open a
//! requisition with documented facts, import the ATS export through the
//! initial-screen filter, capture dispositions with a named decision maker, record
//! the second-person QA check, dispatch, then read the five metrics and route v2.

use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

use chrono::{DateTime, Utc};

use ledger::approval::SignOffRole;
use ledger::ats::Roster;
use ledger::disposition::{
    AdminReason, Disposition, DispositionKind, Dispositions, FormSubmission, NewDisposition,
};
use ledger::error::Error;
use ledger::gate::{FactKey, GateAnswer, GateKind, GateLibrary};
use ledger::id::{
    ApplicationId, CandidateEmail, DecisionMakerId, Mailbox, QaId, RecruiterId, RequisitionId,
    TemplateKind, TextVersion,
};
use ledger::log::{BlockReason, BlockedLog, DeliveryStatus, SendLog};
use ledger::metrics::{Metrics, Replies, Reply, Signals};
use ledger::pipeline::{Dispatch, DispatchReport, Mode, dispatch};
use ledger::playbook::Playbook;
use ledger::policy::Policy;
use ledger::qa::{QaLog, QaRecord, QaScope};
use ledger::requisition::{Requisition, RequisitionBook, Sender};
use ledger::review::{CriteriaFeasibility, Routing};
use ledger::store::{self, Store};
use ledger::template::Templates;

/// Three initial-screen applicants and one who was interviewed.
const ATS_CSV: &str = "\
APPLICATION_ID,FIRST_NAME,EMAIL,REQ_ID,ROLE_TITLE,STAGE_AT_REJECTION,KO_ANSWER_01,KO_ANSWER_02,APPLIED_DATE
APP-1,Ada,ada@example.com,REQ-2026-014,Registered Nurse,Initial screen,not-held,shorter-than-role,2026-11-01
APP-2,Grace,grace@example.com,REQ-2026-014,Registered Nurse,Initial screen,,,2026-11-02
APP-3,Katherine,katherine@example.com,REQ-2026-014,Registered Nurse,Initial screen,,,2026-11-03
APP-4,Mary,mary@example.com,REQ-2026-014,Registered Nurse,Interviewed,,,2026-11-04
";

const REQUISITION: &str = "REQ-2026-014";

/// A pilot workspace with signed-off copy, facts, and an imported roster.
struct Pilot {
    store: Store,
    dir: PathBuf,
    requisition: Requisition,
}

impl Pilot {
    fn workspace(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "ledger-v2-{tag}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _ = fs::remove_dir_all(&dir);
        let store = Store::open(&dir).expect("workspace");

        GateLibrary::seed().save(&store).expect("gate library");
        Templates::seed().save(&store).expect("templates");
        Playbook::seed().save(&store).expect("playbook");
        Policy::default().save(&store).expect("policy");

        let requisition = Requisition::new(
            ledger::requisition::RequisitionDraft {
                id: RequisitionId::try_from(REQUISITION).expect("requisition id"),
                employer: "Example Health".to_owned(),
                title: "Registered Nurse".to_owned(),
                reply_to: Mailbox::try_from("careers-feedback@example.com.au").expect("mailbox"),
                sender: Sender::try_new("R. Patel", "Recruitment Coordinator").expect("sender"),
                gates: vec![GateKind::RightToWork, GateKind::WorkRightsDuration],
                facts: facts(),
            },
            Utc::now(),
        )
        .expect("requisition");

        let mut book = RequisitionBook::default();
        book.insert(requisition.clone()).expect("insert");
        store
            .save(store::REQUISITIONS, &book)
            .expect("save requisitions");

        let roster = Roster::from_csv(&requisition.id, Cursor::new(ATS_CSV)).expect("roster");
        roster.save(&store).expect("save roster");

        Self {
            store,
            dir,
            requisition,
        }
    }

    /// Signs off the gate library, templates, playbook, requisition copy, and
    /// comparative paragraph, and names the escalation owner.
    fn signed(tag: &str) -> Self {
        let mut pilot = Self::workspace(tag);
        let at = Utc::now();
        let approvals = [SignOffRole::HiringManager, SignOffRole::Legal];

        let mut library = GateLibrary::seed();
        let mut templates = Templates::seed();
        let mut playbook = Playbook::seed();
        for role in approvals {
            library.approve("J. Chen", role, at).expect("gate sign-off");
            templates
                .approve_all("M. Okafor", role, at)
                .expect("template sign-off");
            playbook
                .approve("M. Okafor", role, at)
                .expect("playbook sign-off");
        }
        library.save(&pilot.store).expect("save library");
        templates.save(&pilot.store).expect("save templates");
        playbook.save(&pilot.store).expect("save playbook");

        // Ellicit the paragraph first: writing it clears the requisition's sign-off.
        pilot
            .requisition
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
            .expect("comparative paragraph");
        for role in approvals {
            pilot
                .requisition
                .approve("J. Chen", role, at)
                .expect("requisition sign-off");
            pilot
                .requisition
                .approve_comparative("M. Okafor", role, at)
                .expect("paragraph sign-off");
        }

        let mut book = RequisitionBook::default();
        book.insert(pilot.requisition.clone()).expect("insert");
        pilot
            .store
            .save(store::REQUISITIONS, &book)
            .expect("save requisitions");

        let mut policy = Policy::default();
        policy
            .set_escalation_owner("M. Okafor, Employee Relations")
            .expect("owner");
        policy.save(&pilot.store).expect("save policy");

        pilot
    }

    fn roster(&self) -> Roster {
        Roster::load(&self.store, &self.requisition.id)
            .expect("read roster")
            .expect("imported roster")
    }

    fn library(&self) -> GateLibrary {
        self.store
            .load(store::GATE_LIBRARY)
            .expect("read library")
            .expect("seeded library")
    }

    /// Captures a disposition using the ATS export's own name and email.
    fn capture(&self, application: &str, disposition: Disposition) -> String {
        let roster = self.roster();
        let applicant = roster
            .require(&ApplicationId::try_from(application).expect("application id"))
            .expect("applicant in roster");
        let form = NewDisposition::try_new(
            &self.requisition,
            &self.library(),
            FormSubmission {
                application_id: applicant.application_id.clone(),
                candidate_first_name: applicant.first_name.clone(),
                candidate_email: applicant.email.clone(),
                recruiter: RecruiterId::try_from("rec-rui").expect("recruiter"),
                decision_maker: DecisionMakerId::try_from("dm-chen").expect("decision maker"),
                disposition,
                supersedes: None,
            },
            Utc::now(),
        )
        .expect("valid disposition");
        Dispositions::open(&self.store)
            .expect("disposition log")
            .record(form)
            .expect("captured once")
            .id
            .to_string()
    }

    fn correct(&self, application: &str, disposition: Disposition, supersedes: &str) {
        let roster = self.roster();
        let applicant = roster
            .require(&ApplicationId::try_from(application).expect("application id"))
            .expect("applicant in roster");
        let form = NewDisposition::try_new(
            &self.requisition,
            &self.library(),
            FormSubmission {
                application_id: applicant.application_id.clone(),
                candidate_first_name: applicant.first_name.clone(),
                candidate_email: applicant.email.clone(),
                recruiter: RecruiterId::try_from("rec-rui").expect("recruiter"),
                decision_maker: DecisionMakerId::try_from("dm-chen").expect("decision maker"),
                disposition,
                supersedes: Some(
                    ledger::id::DispositionId::try_from(supersedes).expect("record id"),
                ),
            },
            Utc::now(),
        )
        .expect("valid correction");
        Dispositions::open(&self.store)
            .expect("disposition log")
            .record(form)
            .expect("correction captured");
    }

    fn qa(&self, application: &str) {
        let application_id = ApplicationId::try_from(application).expect("application");
        let dispositions = Dispositions::open(&self.store).expect("dispositions");
        let record = dispositions
            .head(&self.requisition.id, &application_id)
            .expect("head record");
        let scope = match record.form.disposition {
            Disposition::Gate { .. } => QaScope::GateFullCheck,
            _ => QaScope::RandomTenPercent,
        };
        QaLog::open(&self.store)
            .expect("qa log")
            .record(QaRecord {
                application_id,
                requisition: self.requisition.id.clone(),
                qa: QaId::try_from("qa-patel").expect("qa id"),
                at: Utc::now(),
                scope,
            })
            .expect("qa recorded");
    }

    fn dispatch(&self, mode: Mode) -> DispatchReport {
        let templates: Templates = self
            .store
            .load(store::TEMPLATES)
            .expect("read templates")
            .expect("seeded templates");
        let playbook = Playbook::load(&self.store)
            .expect("read playbook")
            .expect("seeded playbook");
        let roster = self.roster();
        let dispositions = Dispositions::open(&self.store).expect("dispositions");
        let qa = QaLog::open(&self.store).expect("qa log");
        let mut send_log = SendLog::open(&self.store).expect("send log");
        let mut blocked_log = BlockedLog::open(&self.store).expect("blocked log");
        dispatch(
            &Dispatch {
                mode,
                requisition: &self.requisition,
                library: &self.library(),
                templates: &templates,
                playbook: &playbook,
                roster: &roster,
                dispositions: &dispositions,
                qa: &qa,
                delivery_status: DeliveryStatus::Sent,
                now: Utc::now(),
            },
            &mut send_log,
            &mut blocked_log,
        )
        .expect("dispatch")
    }

    fn record_reply(&self, application: &str) {
        Replies::open(&self.store)
            .record(
                Reply {
                    requisition: self.requisition.id.clone(),
                    received_at: Utc::now(),
                    scenario: Some(ledger::playbook::Scenario::NextTimeGuidance),
                    application_id: Some(ApplicationId::try_from(application).expect("id")),
                },
                1,
            )
            .expect("reply");
    }

    fn record_signal(&self, value: &str, count: u32) {
        Signals::open(&self.store)
            .record(
                ledger::metrics::Signal {
                    requisition: self.requisition.id.clone(),
                    value: value.parse().expect("signal value"),
                    at: Utc::now(),
                },
                count,
            )
            .expect("signal");
    }

    fn metrics(&self) -> Metrics {
        let sends: Vec<_> = SendLog::open(&self.store)
            .expect("send log")
            .entries()
            .expect("entries")
            .into_iter()
            .filter(|entry| entry.requisition == self.requisition.id)
            .collect();
        let replies = Replies::open(&self.store)
            .for_requisition(&self.requisition.id)
            .expect("replies");
        let blocked: Vec<_> = BlockedLog::open(&self.store)
            .expect("blocked log")
            .attempts()
            .iter()
            .filter(|attempt| attempt.requisition == self.requisition.id)
            .cloned()
            .collect();
        let signals = Signals::open(&self.store)
            .for_requisition(&self.requisition.id)
            .expect("signals");
        Metrics::compute(&sends, &replies, &blocked, &signals, &[])
    }

    fn entries(&self) -> Vec<ledger::log::SendEntry> {
        SendLog::open(&self.store)
            .expect("send log")
            .entries()
            .expect("entries")
    }
}

impl Drop for Pilot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn facts() -> BTreeMap<FactKey, String> {
    BTreeMap::from([
        (FactKey::RoleEndOrMinTerm, "30 June 2027".to_owned()),
        (FactKey::MinTerm, "12 months".to_owned()),
        (
            FactKey::RoleDurationReason,
            "The appointment is a fixed-term 12-month contract.".to_owned(),
        ),
    ])
}

#[test]
fn initial_screen_filter_excludes_interviewed_applicants() {
    let pilot = Pilot::workspace("filter");
    let roster = pilot.roster();
    assert_eq!(roster.applicants.len(), 3);
    assert_eq!(roster.excluded_not_initial_screen, 1);
    assert!(
        roster
            .require(&ApplicationId::try_from("APP-4").expect("id"))
            .is_err(),
        "an interviewed candidate must never enter the roster"
    );
}

#[test]
fn end_to_end_v2_pilot_run() {
    let pilot = Pilot::signed("e2e");
    let gate_record = pilot.capture(
        "APP-1",
        Disposition::Gate {
            gate: GateKind::WorkRightsDuration,
            answer: GateAnswer::ShorterThanRole,
        },
    );
    pilot.capture("APP-2", Disposition::Field);
    pilot.capture(
        "APP-3",
        Disposition::Admin {
            reason: AdminReason::RoleRescoped,
        },
    );

    // Preview refuses the GATE send until the coordinator has checked it.
    let preview = pilot.dispatch(Mode::Preview);
    assert_eq!(preview.preview_count(), 2);
    assert_eq!(preview.blocked_count(), 1);
    assert!(matches!(
        preview.blocked[0].reason,
        BlockReason::AwaitingSecondPersonQa { .. }
    ));
    assert_eq!(preview.blocked[0].application_id.as_str(), "APP-1");
    assert!(SendLog::open(&pilot.store).expect("log").is_empty());

    // QA every applicant, then send.
    for application in ["APP-1", "APP-2", "APP-3"] {
        pilot.qa(application);
    }
    let run = pilot.dispatch(Mode::Execute);
    assert_eq!(run.sent_count(), 3);
    assert_eq!(run.blocked_count(), 0);

    let entries = pilot.entries();
    assert_eq!(entries.len(), 3);
    let gate_entry = entries
        .iter()
        .find(|entry| entry.application_id.as_str() == "APP-1")
        .expect("gate entry");
    assert_eq!(gate_entry.disposition, DispositionKind::Gate);
    assert_eq!(gate_entry.gate_code, Some(GateKind::WorkRightsDuration));
    assert_eq!(gate_entry.disposition_id.as_str(), gate_record);
    assert_eq!(gate_entry.decision_maker.as_str(), "dm-chen");
    assert_eq!(gate_entry.text_version.as_str(), "v1.0");
    assert_eq!(gate_entry.delivery_status, DeliveryStatus::Sent);
    assert!(
        gate_entry.body.contains("30 June 2027") && gate_entry.body.contains("12 months"),
        "gate facts must be substituted, got: {}",
        gate_entry.body
    );

    let field_entry = entries
        .iter()
        .find(|entry| entry.disposition == DispositionKind::Field)
        .expect("field entry");
    assert!(
        field_entry
            .body
            .contains("assessment of deteriorating patients")
    );
    assert!(
        field_entry
            .body
            .contains("We do not provide individual feedback")
    );
    assert!(field_entry.body.contains("ledger-signal-yes"));

    let admin_entry = entries
        .iter()
        .find(|entry| entry.disposition == DispositionKind::Admin)
        .expect("admin entry");
    assert!(
        admin_entry
            .body
            .contains("the role was rescoped and will be re-advertised")
    );

    for entry in &entries {
        assert!(
            !entry.body.contains("{{"),
            "unmerged slot in {}",
            entry.body
        );
        assert!(entry.body.contains("careers-feedback@example.com.au"));
        assert!(entry.captured_at <= entry.sent_at);
        assert_eq!(entry.template, entry.disposition.template_kind());
        assert!(
            entry.text_digest
                == ledger::template::RenderedEmail {
                    subject: entry.subject.clone(),
                    body: entry.body.clone(),
                }
                .digest()
        );
    }
    assert_eq!(
        SendLog::open(&pilot.store)
            .expect("log")
            .verification()
            .expect("chain")
            .entries,
        3
    );

    // Idempotent: a second run adds nothing.
    let retry = pilot.dispatch(Mode::Execute);
    assert_eq!(retry.sent_count(), 0);
    assert_eq!(retry.blocked_count(), 0);

    // Metrics: 3 sends, 1 reply, 1 value signal out of 3 answered.
    pilot.record_reply("APP-2");
    pilot.record_signal("yes", 1);
    pilot.record_signal("somewhat", 1);
    pilot.record_signal("no", 1);
    let metrics = pilot.metrics();
    assert_eq!(metrics.sends, 3);
    assert_eq!(metrics.replies, 1);
    assert_eq!(metrics.gate_sends, 1);
    assert_eq!(metrics.gate_mismatches, 0);
    assert!((metrics.reply_volume_per_1000 - 1000.0 / 3.0).abs() < 1e-9);
    assert_eq!(metrics.candidate_value_yes_share, Some(1.0 / 3.0));

    // Routing: red reply volume, amber candidate value, criteria agreed in one
    // session, no other threshold breached.
    let routing = Routing::route(
        pilot.requisition.id.clone(),
        metrics,
        CriteriaFeasibility::AgreedOneSession,
    );
    // V2 orders the consequences: a green block rate with agreed criteria raises the
    // thin-middleware question before red reply volume scopes triage.
    assert_eq!(
        routing
            .decisions
            .iter()
            .map(|decision| decision.action)
            .collect::<Vec<ledger::review::Action>>(),
        vec![
            ledger::review::Action::ReassessVentureViability,
            ledger::review::Action::ScopeReplyTriage,
        ]
    );
}

#[test]
fn a_gate_answer_that_contradicts_the_ats_answer_blocks_and_routes_to_exposure() {
    let pilot = Pilot::signed("mismatch");
    pilot.capture(
        "APP-1",
        Disposition::Gate {
            gate: GateKind::WorkRightsDuration,
            answer: GateAnswer::Restricted,
        },
    );
    pilot.qa("APP-1");

    let report = pilot.dispatch(Mode::Execute);
    assert_eq!(report.sent_count(), 0);
    let blocked = report
        .blocked
        .iter()
        .find(|attempt| attempt.application_id.as_str() == "APP-1")
        .expect("APP-1 blocked");
    assert_eq!(
        blocked.reason,
        BlockReason::AnswerMismatch {
            gate: GateKind::WorkRightsDuration,
            declared: GateAnswer::ShorterThanRole,
            recorded: GateAnswer::Restricted,
        }
    );
    assert!(pilot.entries().is_empty());

    let metrics = pilot.metrics();
    assert_eq!(metrics.gate_mismatches, 1);
    assert_eq!(metrics.ratings.gate_mismatch, ledger::metrics::Rating::Red);
    let routing = Routing::route(
        pilot.requisition.id.clone(),
        metrics,
        CriteriaFeasibility::AgreedOneSession,
    );
    assert_eq!(
        routing.primary().expect("primary").action,
        ledger::review::Action::FlagDiscriminationExposure
    );
}

#[test]
fn a_gate_without_requisition_facts_cannot_be_captured() {
    let pilot = Pilot::workspace("facts");
    let bare = Requisition::new(
        ledger::requisition::RequisitionDraft {
            id: pilot.requisition.id.clone(),
            employer: "Example Health".to_owned(),
            title: "Registered Nurse".to_owned(),
            reply_to: Mailbox::try_from("careers-feedback@example.com.au").expect("mailbox"),
            sender: Sender::try_new("R. Patel", "Recruitment Coordinator").expect("sender"),
            gates: vec![GateKind::WorkRightsDuration],
            facts: BTreeMap::new(),
        },
        Utc::now(),
    )
    .expect("bare requisition");
    let result = NewDisposition::try_new(
        &bare,
        &pilot.library(),
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
        Utc::now(),
    );
    assert!(matches!(result, Err(Error::GateUnavailable { .. })));

    // The same requisition is still usable for FIELD once the paragraph exists.
    let mut requisition = pilot.requisition.clone();
    requisition
        .set_comparative(
            [
                "assessment".to_owned(),
                "escalation".to_owned(),
                "documentation".to_owned(),
            ],
            "a case note",
            "a post-graduate qualification",
            TextVersion::parse("v1.0").expect("version"),
        )
        .expect("paragraph");
    assert!(
        NewDisposition::try_new(
            &requisition,
            &pilot.library(),
            FormSubmission {
                application_id: ApplicationId::try_from("APP-1").expect("application"),
                candidate_first_name: "Ada".to_owned(),
                candidate_email: CandidateEmail::try_from("ada@example.com").expect("email"),
                recruiter: RecruiterId::try_from("rec-rui").expect("recruiter"),
                decision_maker: DecisionMakerId::try_from("dm-chen").expect("decision maker"),
                disposition: Disposition::Field,
                supersedes: None,
            },
            Utc::now(),
        )
        .is_ok()
    );
}

#[test]
fn a_correction_supersedes_and_the_head_renders() {
    let pilot = Pilot::signed("correction");
    let original = pilot.capture("APP-2", Disposition::Field);
    pilot.correct(
        "APP-2",
        Disposition::Admin {
            reason: AdminReason::RequisitionWithdrawn,
        },
        &original,
    );
    pilot.qa("APP-2");

    let report = pilot.dispatch(Mode::Execute);
    assert_eq!(report.sent_count(), 1);
    let entry = &report.sent[0];
    assert_eq!(entry.disposition, DispositionKind::Admin);
    assert!(
        entry.body.contains("the requisition was withdrawn"),
        "the correction, not the superseded record, renders"
    );

    let dispositions = Dispositions::open(&pilot.store).expect("dispositions");
    let history = dispositions.history(&ApplicationId::try_from("APP-2").expect("id"));
    assert_eq!(history.len(), 2);
    assert!(history[0].locked && history[1].locked);
}

#[test]
fn the_second_person_check_is_required_for_gate_emails() {
    let pilot = Pilot::signed("qa");
    pilot.capture(
        "APP-1",
        Disposition::Gate {
            gate: GateKind::RightToWork,
            answer: GateAnswer::NotHeld,
        },
    );
    let report = pilot.dispatch(Mode::Execute);
    assert_eq!(report.sent_count(), 0);
    let gate_block = report
        .blocked
        .iter()
        .find(|attempt| attempt.application_id.as_str() == "APP-1")
        .expect("APP-1 is blocked");
    assert!(matches!(
        gate_block.reason,
        BlockReason::AwaitingSecondPersonQa {
            scope: QaScope::GateFullCheck
        }
    ));
}

#[test]
fn the_chain_detects_an_edited_email() {
    let pilot = Pilot::signed("tamper");
    pilot.capture("APP-2", Disposition::Field);
    pilot.qa("APP-2");
    pilot.dispatch(Mode::Execute);

    let path = pilot.store.path(store::SEND_LOG);
    let text = fs::read_to_string(&path).expect("read send log");
    assert!(text.contains("careers-feedback@example.com.au"));
    fs::write(
        &path,
        text.replace("careers-feedback@example.com.au", "no-reply@example.com"),
    )
    .expect("write send log");

    assert!(matches!(
        SendLog::open(&pilot.store),
        Err(Error::LogCorrupt { .. })
    ));
}

#[test]
fn templates_and_playbook_must_be_signed_off_before_dispatch() {
    let pilot = Pilot::workspace("unsigned");
    let requisition = &pilot.requisition;
    assert!(!requisition.is_ready(&pilot.library()));

    let templates: Templates = pilot
        .store
        .load(store::TEMPLATES)
        .expect("read")
        .expect("seeded");
    let playbook = Playbook::load(&pilot.store).expect("read").expect("seeded");
    let roster = pilot.roster();
    let dispositions = Dispositions::open(&pilot.store).expect("dispositions");
    let qa = QaLog::open(&pilot.store).expect("qa");
    let mut send_log = SendLog::open(&pilot.store).expect("log");
    let mut blocked_log = BlockedLog::open(&pilot.store).expect("blocks");

    let result = dispatch(
        &Dispatch {
            mode: Mode::Execute,
            requisition,
            library: &pilot.library(),
            templates: &templates,
            playbook: &playbook,
            roster: &roster,
            dispositions: &dispositions,
            qa: &qa,
            delivery_status: DeliveryStatus::Sent,
            now: Utc::now(),
        },
        &mut send_log,
        &mut blocked_log,
    );
    assert!(matches!(result, Err(Error::RequisitionNotReady { .. })));
    assert_eq!(send_log.len(), 0);
}

#[test]
fn templates_carry_their_version_and_the_admin_strings_are_exact() {
    let pilot = Pilot::signed("versions");
    let templates: Templates = pilot
        .store
        .load(store::TEMPLATES)
        .expect("read")
        .expect("seeded");
    for kind in TemplateKind::ALL {
        let template = templates.require(kind).expect("template");
        assert_eq!(template.version.as_str(), "v1.0");
        assert!(template.approval.is_approved());
    }
    assert_eq!(
        AdminReason::RequisitionWithdrawn.sentence(),
        "the requisition was withdrawn"
    );
}

#[test]
fn timestamps_round_trip_through_the_workspace_files() {
    let dir = std::env::temp_dir().join(format!("ledger-v2-stamp-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let store = Store::open(&dir).expect("workspace");
    let at: DateTime<Utc> = chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 11, 24, 9, 0, 0)
        .single()
        .expect("timestamp");
    // State documents are objects so they can carry schema_version.
    #[derive(serde::Serialize, serde::Deserialize)]
    struct Stamp {
        at: DateTime<Utc>,
    }
    store
        .save("stamp.json", &Stamp { at })
        .expect("save timestamp");
    let loaded: Stamp = store.load("stamp.json").expect("load").expect("present");
    assert_eq!(loaded.at, at);
    let _ = fs::remove_dir_all(&dir);
}
