//! End-to-end pilot run against the public API.
//!
//! Each test drives the sequence an operator would: seed and sign off copy, open a
//! requisition, import the ATS export, capture dispositions, dispatch under the
//! gate-blocking protocol, then read the metrics and the go/no-go decision.

use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

use chrono::{DateTime, Utc};

use ledger::approval::SignOffRole;
use ledger::ats::Roster;
use ledger::disposition::{DispositionCategory, DispositionRecord, Dispositions};
use ledger::error::Error;
use ledger::gate::{GateAnswer, GateKind, GateLibrary};
use ledger::id::{ApplicationId, Mailbox, RecruiterId, RequisitionId};
use ledger::log::{BlockReason, BlockedLog, SendLog};
use ledger::metrics::{Metrics, Replies, Reply, ReplyTheme};
use ledger::pipeline::{Dispatch, DispatchReport, Mode, dispatch};
use ledger::requisition::{Requisition, RequisitionBook};
use ledger::review::{CriteriaAssessment, GoNoGo, Policy, Recommendation};
use ledger::store::{self, Store};
use ledger::template::Templates;

/// Three applicants; only two of them will get a disposition.
const ATS_CSV: &str = "\
application_id,candidate_name,email,knockout_visa-restrictions
APP-1,Ada Lovelace,ada@example.com,restricted
APP-2,Grace Hopper,grace@example.com,
APP-3,Katherine Johnson,katherine@example.com,
";

const REQUISITION: &str = "REQ-2026-014";

/// A pilot workspace with an imported roster.
struct Pilot {
    store: Store,
    dir: PathBuf,
    requisition: Requisition,
}

impl Pilot {
    /// An unsigned workspace: copy seeded, nothing approved.
    fn unsigned(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "ledger-pilot-{tag}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _ = fs::remove_dir_all(&dir);
        let store = Store::open(&dir).expect("workspace");

        GateLibrary::seed().save(&store).expect("gate library");
        Templates::seed().save(&store).expect("templates");

        let requisition = Requisition::new(
            RequisitionId::try_from(REQUISITION).expect("requisition id"),
            "Example Health",
            "Registered Nurse",
            Mailbox::try_from("careers-feedback@example.com.au").expect("mailbox"),
            [GateKind::VisaRestrictions, GateKind::RightToWork],
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

    /// A workspace with every static copy signed off by HM and Legal.
    fn signed(tag: &str) -> Self {
        let mut pilot = Self::unsigned(tag);
        let at = Utc::now();

        let mut library = GateLibrary::seed();
        library
            .approval
            .record("J. Chen", SignOffRole::HiringManager, at)
            .expect("hm gate sign-off");
        library
            .approval
            .record("M. Okafor", SignOffRole::Legal, at)
            .expect("legal gate sign-off");
        library.save(&pilot.store).expect("save gate library");

        let mut templates = Templates::seed();
        templates
            .approve_all("J. Chen", SignOffRole::HiringManager, at)
            .expect("hm template sign-off");
        templates
            .approve_all("M. Okafor", SignOffRole::Legal, at)
            .expect("legal template sign-off");
        templates.save(&pilot.store).expect("save templates");

        pilot
            .requisition
            .set_comparative(
                "Shortlisted applicants had three years of acute care experience and a \
                 post-graduate qualification in critical care.",
            )
            .expect("comparative paragraph");
        for role in [SignOffRole::HiringManager, SignOffRole::Legal] {
            pilot
                .requisition
                .approve("J. Chen", role, at)
                .expect("requisition sign-off");
            pilot
                .requisition
                .approve_comparative("J. Chen", role, at)
                .expect("paragraph sign-off");
        }
        let mut book = RequisitionBook::default();
        book.insert(pilot.requisition.clone()).expect("insert");
        pilot
            .store
            .save(store::REQUISITIONS, &book)
            .expect("save requisitions");

        pilot
    }

    fn roster(&self) -> Roster {
        Roster::load(&self.store, &self.requisition.id)
            .expect("read roster")
            .expect("imported roster")
    }

    /// Captures a disposition using the ATS export's own name and email.
    fn capture(&self, application: &str, category: DispositionCategory) {
        let roster = self.roster();
        let applicant = roster
            .require(&ApplicationId::try_from(application).expect("application id"))
            .expect("applicant in roster");
        let record = DispositionRecord::try_new(
            &self.requisition,
            applicant.application_id.clone(),
            applicant.email.clone(),
            applicant.name.clone(),
            RecruiterId::try_from("rec-rui").expect("recruiter"),
            Utc::now(),
            category,
        )
        .expect("valid disposition");
        Dispositions::open(&self.store)
            .expect("disposition log")
            .record(record)
            .expect("captured once");
    }

    fn dispatch(&self, mode: Mode) -> DispatchReport {
        let library: GateLibrary = self
            .store
            .load(store::GATE_LIBRARY)
            .expect("read library")
            .expect("seeded library");
        let templates: Templates = self
            .store
            .load(store::TEMPLATES)
            .expect("read templates")
            .expect("seeded templates");
        let roster = self.roster();
        let dispositions = Dispositions::open(&self.store).expect("dispositions");
        let mut send_log = SendLog::open(&self.store).expect("send log");
        let mut blocked_log = BlockedLog::open(&self.store).expect("blocked log");
        dispatch(
            &Dispatch {
                mode,
                requisition: &self.requisition,
                library: &library,
                templates: &templates,
                roster: &roster,
                dispositions: &dispositions,
                now: Utc::now(),
            },
            &mut send_log,
            &mut blocked_log,
        )
        .expect("dispatch")
    }

    fn record_reply(&self, theme: ReplyTheme, count: u32) {
        Replies::open(&self.store)
            .record(
                Reply {
                    requisition: self.requisition.id.clone(),
                    received_at: Utc::now(),
                    theme,
                },
                count,
            )
            .expect("record reply");
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
        let blocked_log = BlockedLog::open(&self.store).expect("blocked log");
        let blocked: Vec<_> = blocked_log
            .attempts()
            .iter()
            .filter(|attempt| attempt.requisition == self.requisition.id)
            .cloned()
            .collect();
        Metrics::compute(&sends, &replies, &blocked)
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

fn sensitive_gate() -> DispositionCategory {
    DispositionCategory::FailedGate {
        gate: GateKind::VisaRestrictions,
        answer: GateAnswer::Restricted,
    }
}

#[test]
fn end_to_end_pilot_run() {
    let pilot = Pilot::signed("e2e");
    pilot.capture("APP-1", sensitive_gate());
    pilot.capture("APP-2", DispositionCategory::MetTheBarStrongerField);

    // Preview renders both emails and reports the block, writing nothing.
    let preview = pilot.dispatch(Mode::Preview);
    assert_eq!(preview.sent_count(), 0);
    assert_eq!(preview.previews.len(), 2);
    assert_eq!(preview.blocked_count(), 1);
    assert_eq!(preview.blocked[0].application_id.as_str(), "APP-3");
    assert_eq!(preview.blocked[0].reason, BlockReason::MissingDisposition);
    assert!(SendLog::open(&pilot.store).expect("log").is_empty());

    // A real run sends the two permitted emails and logs the refusal.
    let run = pilot.dispatch(Mode::Execute);
    assert_eq!(run.sent_count(), 2);
    assert_eq!(run.blocked_count(), 1);

    let log = SendLog::open(&pilot.store).expect("send log");
    let verification = log.verification().expect("chain verifies");
    assert_eq!(verification.entries, 2);
    assert_ne!(verification.head, ledger::log::GENESIS_HASH);

    for entry in pilot.entries() {
        assert!(
            entry.body.contains("careers-feedback@example.com.au"),
            "every email names the monitored human mailbox"
        );
        assert!(entry.body.contains(&entry.candidate_name));
        assert!(
            !entry.body.contains("{{"),
            "no merge slot may survive rendering"
        );
        assert!(
            entry.disposition_captured_at <= entry.sent_at,
            "the log proves capture preceded send"
        );
        assert_eq!(
            entry.category.template_kind(),
            entry.template,
            "the logged template is the one the category requires"
        );
        assert!(
            entry.text_digest
                == ledger::template::RenderedEmail {
                    subject: entry.subject.clone(),
                    body: entry.body.clone()
                }
                .digest()
        );
    }

    // Retrying is idempotent: no second email, no double-counted block.
    let retry = pilot.dispatch(Mode::Execute);
    assert_eq!(retry.sent_count(), 0);
    assert_eq!(retry.blocked_count(), 0);
    assert_eq!(pilot.entries().len(), 2);

    // Metric 1: replies per 1,000 sends. Metric 2: block share.
    pilot.record_reply(ReplyTheme::GateQuestion, 1);
    let metrics = pilot.metrics();
    assert_eq!(metrics.sends, 2);
    assert_eq!(metrics.replies, 1);
    assert_eq!(metrics.blocked, 1);
    assert_eq!(metrics.reply_rate_per_1000, 500.0);
    assert!((metrics.form_block_rate - 1.0 / 3.0).abs() < 1e-9);
    assert_eq!(
        metrics.reply_themes.get(&ReplyTheme::GateQuestion),
        Some(&1)
    );

    // Task 4.3: criteria agreed, but friction is out of policy, so v2 waits.
    let review = GoNoGo::decide(
        pilot.requisition.id.clone(),
        metrics,
        CriteriaAssessment::Agreed,
        Policy::default(),
    );
    assert_eq!(review.recommendation, Recommendation::HoldAndIterate);
    assert_eq!(
        review.breaches.len(),
        2,
        "both the reply volume and the block rate breach policy"
    );
}

#[test]
fn dispatch_refuses_to_run_before_the_copy_is_signed_off() {
    let pilot = Pilot::unsigned("unsigned");
    pilot.capture("APP-1", DispositionCategory::MetTheBarStrongerField);

    let library: GateLibrary = pilot
        .store
        .load(store::GATE_LIBRARY)
        .expect("read library")
        .expect("seeded library");
    let templates: Templates = pilot
        .store
        .load(store::TEMPLATES)
        .expect("read templates")
        .expect("seeded templates");
    let roster = pilot.roster();
    let dispositions = Dispositions::open(&pilot.store).expect("dispositions");
    let mut send_log = SendLog::open(&pilot.store).expect("send log");
    let mut blocked_log = BlockedLog::open(&pilot.store).expect("blocked log");

    let result = dispatch(
        &Dispatch {
            mode: Mode::Execute,
            requisition: &pilot.requisition,
            library: &library,
            templates: &templates,
            roster: &roster,
            dispositions: &dispositions,
            now: Utc::now(),
        },
        &mut send_log,
        &mut blocked_log,
    );

    assert!(matches!(result, Err(Error::RequisitionNotReady { .. })));
    assert_eq!(send_log.len(), 0);
}

#[test]
fn a_form_answer_that_contradicts_the_ats_answer_blocks_the_send() {
    let pilot = Pilot::signed("mismatch");
    // The application says the visa is restricted; the form claims it expired.
    pilot.capture(
        "APP-1",
        DispositionCategory::FailedGate {
            gate: GateKind::VisaRestrictions,
            answer: GateAnswer::Expired,
        },
    );
    assert_eq!(
        pilot
            .roster()
            .require(&ApplicationId::try_from("APP-1").expect("application"))
            .expect("applicant")
            .knockout(GateKind::VisaRestrictions),
        Some(GateAnswer::Restricted)
    );

    let report = pilot.dispatch(Mode::Execute);
    assert_eq!(report.sent_count(), 0);
    let mismatch = report
        .blocked
        .iter()
        .find(|attempt| attempt.application_id.as_str() == "APP-1")
        .expect("APP-1 is blocked");
    assert_eq!(
        mismatch.reason,
        BlockReason::AnswerMismatch {
            gate: GateKind::VisaRestrictions,
            declared: GateAnswer::Restricted,
            recorded: GateAnswer::Expired,
        }
    );
    assert!(
        pilot.entries().is_empty(),
        "a contradiction must not produce an email"
    );
}

#[test]
fn the_log_chain_detects_an_edited_email() {
    let pilot = Pilot::signed("tamper");
    pilot.capture("APP-1", sensitive_gate());
    pilot.capture("APP-2", DispositionCategory::MetTheBarStrongerField);
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
fn timestamps_round_trip_through_the_workspace_files() {
    let dir = std::env::temp_dir().join(format!("ledger-stamp-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let store = Store::open(&dir).expect("workspace");
    let at: DateTime<Utc> = chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 11, 1, 9, 0, 0)
        .single()
        .expect("timestamp");
    store.save("stamp.json", &at).expect("save timestamp");
    let loaded: DateTime<Utc> = store.load("stamp.json").expect("load").expect("present");
    assert_eq!(loaded, at);
    let _ = fs::remove_dir_all(&dir);
}
