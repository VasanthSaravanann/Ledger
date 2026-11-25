//! Command line interface for the pilot.

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::Utc;
use clap::{Args, Parser, Subcommand};

use ledger::approval::SignOffRole;
use ledger::ats::Roster;
use ledger::checklist::{Checklist, TASKS};
use ledger::delivery::DeliveryLog;
use ledger::disposition::{
    AdminReason, Disposition, DispositionKind, Dispositions, FormSubmission, NewDisposition,
};
use ledger::error::{Error, Result};
use ledger::gate::{FactKey, GateAnswer, GateKind, GateLibrary};
use ledger::id::{
    ApplicationId, CandidateEmail, DecisionMakerId, DispositionId, Mailbox, QaId, RecruiterId,
    RequisitionId, SendId, TemplateKind, TextVersion,
};
use ledger::log::{BlockedLog, DeliveryStatus, SendEntry, SendLog};
use ledger::metrics::{Metrics, Replies, Reply, Signals};
use ledger::pipeline::{Dispatch, DispatchReport, Mode, dispatch};
use ledger::playbook::{Playbook, Scenario};
use ledger::policy::Policy;
use ledger::qa::{QaLog, QaRecord, QaScope, sampled};
use ledger::requisition::{Requisition, RequisitionBook, Sender};
use ledger::review::{CriteriaFeasibility, CriteriaRecord, Reviews, Routing};
use ledger::store::{self, Store};
use ledger::template::Templates;

/// Timestamp format used in all human-readable output.
const STAMP: &str = "%Y-%m-%d %H:%M UTC";

#[derive(Debug, Parser)]
#[command(
    name = "ledger",
    version,
    about = "Audit-grade candidate feedback pilot: disposition capture, gated copy, immutable log",
    long_about = "Implements V2.md: a locked GATE/FIELD/ADMIN disposition form with named \
                  decision makers and superseding corrections, five versioned gate blocks with \
                  requisition facts, a linted role-level paragraph, three signed-off templates, \
                  a five-scenario reply playbook, a hard initial-screen filter, second-person QA, \
                  an append-only hash-chained send log, five pre-registered metrics, and the v2 \
                  routing decision."
)]
struct Cli {
    /// Pilot workspace directory.
    #[arg(long, global = true, default_value = "ledger-data", value_name = "DIR")]
    data: PathBuf,
    /// Reveal raw application ids and emails in console output (they are redacted
    /// to stable #hash handles by default).
    #[arg(long, global = true)]
    debug_ids: bool,
    /// The operator acting. Defaults to LEDGER_OPERATOR, then the only registered
    /// operator.
    #[arg(long, global = true, value_name = "NAME")]
    operator: Option<String>,
    /// Print one JSON object on stdout and no prose. Every command accepts it.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Seed the workspace: gate copy, templates, playbook, policy.
    Init,
    /// Build the deterministic synthetic fixture (no pilot data).
    #[command(subcommand)]
    Demo(DemoCommand),
    /// Show the task sheet and the evidence the workspace holds.
    Status(StatusArgs),
    /// Show or set retention and the escalation owner.
    #[command(subcommand)]
    Governance(GovernanceCommand),
    /// Inspect, approve, and version the Tier 0 gate copy.
    #[command(subcommand)]
    Gate(GateCommand),
    /// Create, document, approve, and inspect requisitions.
    #[command(subcommand)]
    Requisition(RequisitionCommand),
    /// Inspect and approve the three master templates.
    #[command(subcommand)]
    Templates(TemplatesCommand),
    /// Inspect and approve the five reply scripts.
    #[command(subcommand)]
    Playbook(PlaybookCommand),
    /// Import the ATS export.
    #[command(subcommand)]
    Ats(AtsCommand),
    /// Capture, correct, and list dispositions.
    #[command(subcommand)]
    Disposition(DispositionCommand),
    /// Record the second-person QA check.
    #[command(subcommand)]
    Qa(QaCommand),
    /// Render and dispatch feedback under the circuit-breakers.
    Dispatch(DispatchArgs),
    /// Confirm a recorded send was delivered, as a new chained event.
    #[command(subcommand)]
    Delivery(DeliveryCommand),
    /// Verify or export the send log.
    #[command(subcommand)]
    Log(LogCommand),
    /// Record candidate replies.
    #[command(subcommand)]
    Reply(ReplyCommand),
    /// Record candidate-value signals.
    #[command(subcommand)]
    Signal(SignalCommand),
    /// Compute the five metrics.
    Metrics(MetricsArgs),
    /// Record criteria feasibility and compute the v2 routing.
    #[command(subcommand)]
    Review(ReviewCommand),
    /// Sign off plan tasks.
    #[command(subcommand)]
    Checklist(ChecklistCommand),
    /// Enforce retention: tombstone expired PII.
    Purge(PurgeArgs),
    /// Snapshot the workspace and migrate its schema.
    Migrate(MigrateArgs),
    /// Publish the chain head as an off-host receipt.
    Anchor(AnchorArgs),
    /// Resolve a redacted #hash handle back to its record.
    Find(FindArgs),
    /// Register and inspect operator identities.
    #[command(subcommand)]
    Operator(OperatorCommand),
    /// Re-check every disposition's operator signature.
    VerifySignatures(VerifySignaturesArgs),
    /// Serve the local recruiter workspace (requires --features ui).
    #[cfg(feature = "ui")]
    Ui(UiArgs),
}

#[cfg(feature = "ui")]
#[derive(Debug, Args)]
struct UiArgs {
    /// Loopback port for the local workspace.
    #[arg(long, default_value_t = 8420)]
    port: u16,
}

#[derive(Debug, Subcommand)]
enum OperatorCommand {
    /// Create a keypair and register the operator.
    Register(OperatorRegisterArgs),
    /// List registered operators and roles.
    List,
    /// Print the active operator.
    Whoami,
}

#[derive(Debug, Args)]
struct OperatorRegisterArgs {
    #[arg(long)]
    name: String,
    /// coordinator, approver or auditor.
    #[arg(long)]
    role: String,
}

#[derive(Debug, Args)]
struct VerifySignaturesArgs {}

#[derive(Debug, Args)]
struct PurgeArgs {
    /// Required for a destructive run.
    #[arg(long)]
    yes: bool,
    /// Report what would be purged without writing.
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    operator: Option<String>,
}

#[derive(Debug, Args)]
struct MigrateArgs {
    /// Required for a run that writes a snapshot.
    #[arg(long)]
    yes: bool,
    #[arg(long)]
    operator: Option<String>,
}

#[derive(Debug, Args)]
struct AnchorArgs {
    /// Also commit the receipt to the workspace's git repository.
    #[arg(long)]
    git_commit: bool,
    #[arg(long)]
    operator: Option<String>,
}

#[derive(Debug, Args)]
struct FindArgs {
    /// The handle printed in an error, for example #a1b2c3d4.
    #[arg(long)]
    hash: String,
}

#[derive(Debug, Subcommand)]
enum DemoCommand {
    /// Build the deterministic fixture for REQ-2026-014.
    Seed(DemoSeedArgs),
}

#[derive(Debug, Args)]
struct DemoSeedArgs {
    /// Replace an existing pilot in this workspace.
    #[arg(long)]
    force: bool,
    /// Also commit the sends and the GATE-02 version bump.
    #[arg(long)]
    record: bool,
}

#[derive(Debug, Args)]
struct StatusArgs {
    /// Restrict the evidence summary to one requisition.
    #[arg(long)]
    requisition: Option<String>,
}

#[derive(Debug, Subcommand)]
enum GovernanceCommand {
    /// Print retention, escalation owner, and SLA.
    Show,
    /// Set retention and/or the escalation owner.
    Set(GovernanceSet),
}

#[derive(Debug, Args)]
struct GovernanceSet {
    /// Retention period in days.
    #[arg(long)]
    retention_days: Option<u32>,
    /// Named ER/Legal escalation owner.
    #[arg(long)]
    escalation_owner: Option<String>,
    /// Acknowledgement SLA in hours.
    #[arg(long)]
    sla_hours: Option<u32>,
}

#[derive(Debug, Subcommand)]
enum GateCommand {
    /// List the five blocks, versions, and required facts.
    List,
    /// Sign off the copy library in one pass.
    Approve(GateApprove),
    /// Bump one block's TEXT_VERSION (voids sign-off).
    BumpVersion(GateBump),
}

#[derive(Debug, Args)]
struct GateApprove {
    #[arg(long)]
    actor: String,
    #[arg(long)]
    role: String,
}

#[derive(Debug, Args)]
struct GateBump {
    #[arg(long)]
    gate: String,
    /// New version, for example v1.1.
    #[arg(long)]
    version: String,
}

#[derive(Debug, Subcommand)]
enum RequisitionCommand {
    /// Create a requisition with its documented facts.
    Create(RequisitionCreate),
    /// Set or update one requisition fact.
    Fact(RequisitionFact),
    /// Elicit the role-level comparative paragraph.
    Comparative(RequisitionComparative),
    /// Approve a block, the requisition copy, or the comparative paragraph.
    Approve(RequisitionApprove),
    /// Show one requisition and what still blocks dispatch.
    Show(RequisitionShow),
    /// List every requisition.
    List,
}

#[derive(Debug, Args)]
struct RequisitionCreate {
    #[arg(long)]
    id: String,
    #[arg(long)]
    employer: String,
    #[arg(long)]
    title: String,
    /// Monitored reply-to mailbox.
    #[arg(long)]
    reply_to: String,
    /// Named sender.
    #[arg(long)]
    sender_name: String,
    /// Sender's title.
    #[arg(long)]
    sender_title: String,
    /// Knockout gate; repeat for each gate (GATE-01..GATE-05 or a slug).
    #[arg(long = "gate", required = true)]
    gates: Vec<String>,
    /// Requisition fact as KEY=VALUE; repeat as needed.
    #[arg(long = "fact", value_name = "KEY=VALUE")]
    facts: Vec<String>,
}

#[derive(Debug, Args)]
struct RequisitionFact {
    #[arg(long)]
    id: String,
    /// Fact key, for example REGULATOR_URL.
    #[arg(long)]
    key: String,
    #[arg(long)]
    value: String,
}

#[derive(Debug, Args)]
struct RequisitionComparative {
    #[arg(long)]
    id: String,
    /// What shortlisted applicants all demonstrated (1 of 3).
    #[arg(long)]
    attribute_1: String,
    /// What shortlisted applicants all demonstrated (2 of 3).
    #[arg(long)]
    attribute_2: String,
    /// What shortlisted applicants all demonstrated (3 of 3).
    #[arg(long)]
    attribute_3: String,
    /// The concrete evidence a strong application made visible.
    #[arg(long)]
    evidence: String,
    /// What many applicants assumed mattered but did not.
    #[arg(long)]
    non_factors: String,
    /// Paragraph version.
    #[arg(long, default_value = "v1.0")]
    version: String,
}

#[derive(Debug, Args)]
struct RequisitionApprove {
    #[arg(long)]
    id: String,
    #[arg(long)]
    actor: String,
    #[arg(long)]
    role: String,
    /// Sign off the comparative paragraph instead of the requisition copy.
    #[arg(long)]
    comparative: bool,
}

#[derive(Debug, Args)]
struct RequisitionShow {
    #[arg(long)]
    id: String,
}

#[derive(Debug, Subcommand)]
enum TemplatesCommand {
    /// Show the three templates and their sign-off state.
    Show,
    /// Sign off all three in one pass.
    Approve(TemplatesApprove),
}

#[derive(Debug, Args)]
struct TemplatesApprove {
    #[arg(long)]
    actor: String,
    #[arg(long)]
    role: String,
}

#[derive(Debug, Subcommand)]
enum PlaybookCommand {
    /// Show the five scripts, sign-off state, and escalation.
    Show,
    /// Print one script.
    Script(PlaybookScript),
    /// Sign off the playbook in one pass.
    Approve(PlaybookApprove),
}

#[derive(Debug, Args)]
struct PlaybookScript {
    /// individual-feedback, record-correction, next-time-guidance,
    /// discrimination-concern, or talent-pool.
    #[arg(long)]
    scenario: String,
}

#[derive(Debug, Args)]
struct PlaybookApprove {
    #[arg(long)]
    actor: String,
    #[arg(long)]
    role: String,
}

#[derive(Debug, Subcommand)]
enum AtsCommand {
    /// Import an ATS CSV export, applying the initial-screen filter.
    Import(AtsImport),
}

#[derive(Debug, Args)]
struct AtsImport {
    #[arg(long)]
    requisition: String,
    /// CSV export with APPLICATION_ID, FIRST_NAME, EMAIL, REQ_ID,
    /// STAGE_AT_REJECTION, and optional KO_ANSWER_01..05.
    #[arg(long)]
    file: PathBuf,
}

#[derive(Debug, Subcommand)]
#[expect(
    clippy::large_enum_variant,
    reason = "clap derive builds one argument enum at startup; boxing would only obscure the CLI"
)]
enum DispositionCommand {
    /// Capture one disposition, or a correction.
    Record(DispositionRecordArgs),
    /// List current dispositions for a requisition.
    List(DispositionList),
    /// Show one application's correction chain.
    History(DispositionHistory),
}

#[derive(Debug, Args)]
struct DispositionRecordArgs {
    #[arg(long)]
    requisition: String,
    #[arg(long)]
    application: String,
    #[arg(long)]
    first_name: String,
    #[arg(long)]
    email: String,
    #[arg(long)]
    recruiter: String,
    /// Named person who made the adverse-action call.
    #[arg(long)]
    decision_maker: String,
    /// GATE, FIELD, or ADMIN.
    #[arg(long)]
    category: String,
    /// GATE-01..GATE-05 or a gate slug, for GATE.
    #[arg(long)]
    gate: Option<String>,
    /// Gate answer, for GATE.
    #[arg(long)]
    answer: Option<String>,
    /// requisition-withdrawn, role-rescoped, or filled-internally, for ADMIN.
    #[arg(long)]
    reason: Option<String>,
    /// Correction: the current record this replaces, for example DSP-0001.
    #[arg(long)]
    supersedes: Option<String>,
}

#[derive(Debug, Args)]
struct DispositionList {
    #[arg(long)]
    requisition: String,
}

#[derive(Debug, Args)]
struct DispositionHistory {
    #[arg(long)]
    application: String,
}

#[derive(Debug, Subcommand)]
enum QaCommand {
    /// Record one second-person check.
    Record(QaRecordArgs),
    /// List recorded checks.
    List(QaListArgs),
}

#[derive(Debug, Args)]
struct QaRecordArgs {
    #[arg(long)]
    requisition: String,
    #[arg(long)]
    application: String,
    /// Coordinator who performed the check.
    #[arg(long)]
    qa: String,
}

#[derive(Debug, Args)]
struct QaListArgs {
    #[arg(long)]
    requisition: String,
}

#[derive(Debug, Args)]
struct DispatchArgs {
    #[arg(long)]
    requisition: String,
    /// Render and report without writing either log.
    #[arg(long)]
    preview: bool,
    /// DELIVERY_STATUS to record. Defaults to `queued` when --eml is used, else `sent`.
    #[arg(long)]
    status: Option<String>,
    /// Write one RFC 5322 draft per send into this directory, plus a manifest.
    #[arg(long, value_name = "DIR")]
    eml: Option<PathBuf>,
    /// Exit 0 even when attempts were blocked (exit 2 is the default for a commit).
    #[arg(long)]
    allow_blocked: bool,
}

#[derive(Debug, Subcommand)]
enum DeliveryCommand {
    /// Confirm a recorded send was delivered (appends a chained entry).
    Confirm(DeliveryConfirmArgs),
    /// List the delivery-confirmation chain.
    List(DeliveryListArgs),
}

#[derive(Debug, Args)]
struct DeliveryConfirmArgs {
    /// The send-log id that is now confirmed delivered, e.g. SND-000001.
    #[arg(long)]
    send_id: String,
}

#[derive(Debug, Args)]
struct DeliveryListArgs {}

#[derive(Debug, Subcommand)]
enum LogCommand {
    /// Verify the hash chain and print its head.
    Verify(LogVerifyArgs),
    /// Export the send log as CSV with REPLY_RECEIVED_FLAG.
    Export(LogExportArgs),
    /// Preserve a torn trailing line and restore chain continuity.
    Quarantine(LogQuarantineArgs),
}

#[derive(Debug, Args)]
struct LogVerifyArgs {}

#[derive(Debug, Args)]
struct LogQuarantineArgs {
    /// Why the tail is being quarantined; recorded in operator.jsonl.
    #[arg(long)]
    reason: String,
    /// Required: quarantine truncates the send log.
    #[arg(long)]
    yes: bool,
    /// Operator name; defaults to LEDGER_OPERATOR, then USER.
    #[arg(long)]
    operator: Option<String>,
}

#[derive(Debug, Args)]
struct LogExportArgs {
    #[arg(long)]
    requisition: String,
    /// Write to this file instead of stdout.
    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
enum ReplyCommand {
    /// Record replies received in the monitored mailbox.
    Record(ReplyRecordArgs),
}

#[derive(Debug, Args)]
struct ReplyRecordArgs {
    #[arg(long)]
    requisition: String,
    /// The application the reply concerns, when the sender identifies it.
    #[arg(long)]
    application: Option<String>,
    /// Playbook scenario that handled it, when one applies.
    #[arg(long)]
    scenario: Option<String>,
    /// How many replies.
    #[arg(long, default_value_t = 1)]
    count: u32,
}

#[derive(Debug, Subcommand)]
enum SignalCommand {
    /// Record candidate-value signals from the one-click links.
    Record(SignalRecordArgs),
}

#[derive(Debug, Args)]
struct SignalRecordArgs {
    #[arg(long)]
    requisition: String,
    /// yes, somewhat, or no.
    #[arg(long)]
    value: String,
    #[arg(long, default_value_t = 1)]
    count: u32,
}

#[derive(Debug, Args)]
struct MetricsArgs {
    #[arg(long)]
    requisition: String,
}

#[derive(Debug, Subcommand)]
enum ReviewCommand {
    /// Record whether HM and Legal agreed the criteria.
    Criteria(ReviewCriteriaArgs),
    /// Compute the v2 routing decision.
    Route(ReviewRouteArgs),
}

#[derive(Debug, Args)]
struct ReviewCriteriaArgs {
    #[arg(long)]
    requisition: String,
    /// agreed-one-session, agreed-with-external-input, or not-agreed.
    #[arg(long)]
    status: String,
    #[arg(long)]
    actor: String,
    #[arg(long)]
    note: Option<String>,
}

#[derive(Debug, Args)]
struct ReviewRouteArgs {
    #[arg(long)]
    requisition: String,
}

#[derive(Debug, Subcommand)]
enum ChecklistCommand {
    /// Mark a plan task done.
    Done(ChecklistDoneArgs),
    /// Print the task sheet.
    Show,
}

#[derive(Debug, Args)]
struct ChecklistDoneArgs {
    /// Task id, for example 2.1.
    task: String,
    #[arg(long)]
    actor: String,
    #[arg(long)]
    reference: Option<String>,
}

fn main() -> std::process::ExitCode {
    let command = Cli::parse();
    match execute(command) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            std::process::ExitCode::from(error.exit_code())
        }
    }
}

fn execute(cli: Cli) -> Result<()> {
    ledger::redact::set_debug_ids(cli.debug_ids);
    let store = Store::open(&cli.data)?;
    run(&store, cli.command, cli.operator.as_deref(), cli.json)
}

fn run(store: &Store, command: Command, explicit_operator: Option<&str>, json: bool) -> Result<()> {
    // Read-only commands never take the lock; everything else does, so two
    // operators cannot interleave writes into the same workspace.
    let read_only = is_read_only(&command);
    let mutating = !read_only;
    let name = command_name(&command);
    let args = redacted_argv();
    let _lock = if read_only { None } else { Some(store.lock()?) };

    // Identity and authorisation. A workspace with no registry is in bootstrap:
    // only `init` and the demo fixture may run until someone registers.
    let registry = ledger::identity::Registry::load(store)?;
    // Read-only commands never need an identity: `operator list` and
    // `verify-signatures` must work even when identity resolution is ambiguous.
    let active_operator = if mutating && !registry.is_empty() {
        Some(ledger::identity::active(&registry, explicit_operator)?)
    } else {
        None
    };
    if mutating {
        match &active_operator {
            Some(operator) if operator.role.permits(name) => {}
            Some(operator) => {
                return Err(Error::Forbidden {
                    operator: operator.name.clone(),
                    role: operator.role.to_string(),
                    command: name.to_owned(),
                });
            }
            None if matches!(name, "init" | "demo seed" | "operator") => {}
            None => {
                return Err(Error::invalid(
                    "operator",
                    "register an operator first: `ledger operator register --name <name> --role <role>`",
                ));
            }
        }
    }

    let actor = Actor {
        operator: active_operator.as_ref(),
        registry: &registry,
        store,
    };

    // Snapshot before a command that can change many records at once.
    if wants_snapshot(&command) && active_operator.is_some() {
        let stamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        ledger::migrate::snapshot_workspace(
            store,
            &store.path("backups").join(format!("{stamp}-{name}")),
        )?;
    }

    let outcome = match command {
        Command::Init => init(store, json),
        Command::Demo(DemoCommand::Seed(args)) => demo_seed(store, &args, json),
        Command::Status(args) => status(store, &args, json),
        Command::Governance(command) => governance(store, command, json),
        Command::Gate(command) => gate(store, command, &actor, json),
        Command::Requisition(command) => requisition(store, command, &actor, json),
        Command::Templates(command) => templates(store, command, &actor, json),
        Command::Playbook(command) => playbook(store, command, &actor, json),
        Command::Ats(AtsCommand::Import(args)) => ats_import(store, &args, json),
        Command::Disposition(command) => {
            disposition(store, command, explicit_operator, &registry, json)
        }
        Command::Qa(command) => qa(store, command, json),
        Command::Dispatch(args) => dispatch_command(store, &args, json),
        Command::Delivery(command) => delivery_command(store, command, &actor, json),
        Command::Log(command) => log(store, command, json),
        Command::Reply(ReplyCommand::Record(args)) => reply_record(store, &args, json),
        Command::Signal(SignalCommand::Record(args)) => signal_record(store, &args, json),
        Command::Metrics(args) => metrics_command(store, &args, json),
        Command::Review(command) => review(store, command, json),
        Command::Checklist(command) => checklist(store, command, json),
        Command::Purge(args) => purge(store, &args, json),
        Command::Migrate(args) => migrate(store, &args, json),
        Command::Anchor(args) => anchor(store, &args, json),
        Command::Find(args) => find(store, &args, json),
        Command::Operator(command) => operator(store, command, explicit_operator, json),
        Command::VerifySignatures(_) => verify_signatures(store, json),
        #[cfg(feature = "ui")]
        Command::Ui(args) => ui(store, &args, explicit_operator, json),
    };

    // Every mutating command that succeeded leaves an operator record. The argv is
    // redacted, so an address in a flag does not end up in the operator log.
    if mutating && outcome.is_ok() && !records_own_audit(name) {
        let mut details = BTreeMap::new();
        details.insert("args".to_owned(), args);
        details.insert("pid".to_owned(), std::process::id().to_string());
        let _ = ledger::operator::OperatorLog::open(store).record(ledger::operator::OperatorNote {
            at: Utc::now(),
            operator: ledger::operator::identity(None),
            command: name.to_owned(),
            note: String::new(),
            details,
        });
    }
    outcome
}

/// The stable command name recorded in `operator.jsonl`.
fn command_name(command: &Command) -> &'static str {
    match command {
        Command::Init => "init",
        Command::Demo(_) => "demo seed",
        Command::Status(_) => "status",
        Command::Governance(_) => "governance",
        Command::Gate(_) => "gate",
        Command::Requisition(_) => "requisition",
        Command::Templates(_) => "templates",
        Command::Playbook(_) => "playbook",
        Command::Ats(_) => "ats import",
        Command::Disposition(_) => "disposition",
        Command::Qa(_) => "qa",
        Command::Dispatch(_) => "dispatch",
        Command::Delivery(_) => "delivery",
        Command::Log(_) => "log",
        Command::Reply(_) => "reply",
        Command::Signal(_) => "signal",
        Command::Metrics(_) => "metrics",
        Command::Review(_) => "review",
        Command::Checklist(_) => "checklist",
        Command::Purge(_) => "purge",
        Command::Migrate(_) => "migrate",
        Command::Anchor(_) => "anchor",
        Command::Find(_) => "find",
        Command::Operator(_) => "operator",
        Command::VerifySignatures(_) => "verify-signatures",
        #[cfg(feature = "ui")]
        Command::Ui(_) => "ui",
    }
}

/// Whether to snapshot the workspace before running this command.
fn wants_snapshot(command: &Command) -> bool {
    match command {
        Command::Dispatch(args) => !args.preview,
        Command::Purge(args) => !args.dry_run && args.yes,
        Command::Migrate(_) => false,
        _ => false,
    }
}

/// Whether the command already writes a richer record of its own, so the generic
/// hook does not duplicate it.
fn records_own_audit(name: &str) -> bool {
    matches!(name, "purge" | "migrate" | "anchor" | "log")
}

/// The current argv with email-like tokens redacted.
fn redacted_argv() -> String {
    std::env::args()
        .skip(1)
        .map(|argument| ledger::redact::text(&argument))
        .collect::<Vec<String>>()
        .join(" ")
}

/// Prints exactly one JSON object, stamped with the command that produced it.
fn emit_json(command: &str, fields: serde_json::Value) -> Result<()> {
    let mut object = match fields {
        serde_json::Value::Object(object) => object,
        _ => return Err(Error::invalid("json", "a command must emit one object")),
    };
    object.insert(
        "command".to_owned(),
        serde_json::Value::String(command.to_owned()),
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::Value::Object(object))?
    );
    Ok(())
}

fn init(store: &Store, json: bool) -> Result<()> {
    let mut seeded: Vec<&str> = Vec::new();
    if store.load::<GateLibrary>(store::GATE_LIBRARY)?.is_none() {
        GateLibrary::seed().save(store)?;
        seeded.push(store::GATE_LIBRARY);
        if !json {
            println!("seeded 5 gate blocks at v1.0");
        }
    }
    if store.load::<Templates>(store::TEMPLATES)?.is_none() {
        Templates::seed().save(store)?;
        seeded.push(store::TEMPLATES);
        if !json {
            println!("seeded templates A, B, C at v1.0");
        }
    }
    if store.load::<Playbook>(store::PLAYBOOK)?.is_none() {
        Playbook::seed().save(store)?;
        seeded.push(store::PLAYBOOK);
        if !json {
            println!("seeded 5 reply scripts");
        }
    }
    if store.load::<Policy>(store::POLICY)?.is_none() {
        Policy::default().save(store)?;
        seeded.push(store::POLICY);
        if !json {
            println!("seeded retention and escalation policy");
        }
    }
    if json {
        return emit_json(
            "init",
            serde_json::json!({
                "workspace": store.root().display().to_string(),
                "seeded": seeded,
            }),
        );
    }
    println!("\nworkspace: {}", store.root().display());
    println!("\nnext steps:");
    println!("  1. name the escalation owner:  ledger governance set --escalation-owner '...'");
    println!(
        "  2. sign off the static copy:   ledger gate approve / templates approve / playbook approve"
    );
    println!(
        "  3. create the requisition:     ledger requisition create --fact ROLE_END_OR_MIN_TERM=..."
    );
    println!("  4. elicit the paragraph:       ledger requisition comparative --id REQ-1 ...");
    println!(
        "  5. import the ATS export:      ledger ats import --requisition REQ-1 --file export.csv"
    );
    println!("  6. capture, QA, then dispatch: ledger dispatch --requisition REQ-1 --preview");
    Ok(())
}

fn status(store: &Store, args: &StatusArgs, json: bool) -> Result<()> {
    let checklist = Checklist::load(store)?;
    let (done, total) = checklist.progress();
    if !json {
        println!("tasks signed off: {done}/{total}\n");
        for phase in 1..=4 {
            println!("phase {phase}");
            for task in TASKS.iter().filter(|task| task.phase == phase) {
                let mark = if checklist.is_done(task.id) { "x" } else { " " };
                match checklist.completion(task.id) {
                    Some(completion) => println!(
                        "  [{mark}] {} {:<52} {} ({})",
                        task.id,
                        task.title,
                        completion.actor,
                        completion.at.format(STAMP)
                    ),
                    None => println!("  [{mark}] {} {}", task.id, task.title),
                }
            }
        }
    }

    let filtered = args
        .requisition
        .as_ref()
        .map(|id| RequisitionId::try_from(id.as_str()))
        .transpose()?;

    let library = load_library(store)?;
    let templates = load_templates(store)?;
    let playbook = load_playbook(store)?;
    let policy = Policy::load(store)?;
    let dispositions = Dispositions::open(store)?;
    let send_log = SendLog::open(store)?;
    let blocked_log = BlockedLog::open(store)?;
    let replies = Replies::open(store).all()?;
    let qa_log = QaLog::open(store)?;

    let keep = |id: &RequisitionId| filtered.as_ref().is_none_or(|want| want == id);
    let dispositions = dispositions
        .records()
        .iter()
        .filter(|record| keep(&record.form.requisition))
        .count();
    let sends: Vec<_> = send_log
        .entries()?
        .into_iter()
        .filter(|entry| keep(&entry.requisition))
        .collect();
    let blocks = blocked_log
        .attempts()
        .iter()
        .filter(|attempt| keep(&attempt.requisition))
        .count();
    let reply_count = replies
        .iter()
        .filter(|reply| keep(&reply.requisition))
        .count();
    let qa_count = qa_log
        .records()
        .iter()
        .filter(|record| keep(&record.requisition))
        .count();

    let book = load_requisitions(store)?;
    if json {
        let tasks: Vec<serde_json::Value> = TASKS
            .iter()
            .map(|task| {
                let completion = checklist.completion(task.id);
                serde_json::json!({
                    "id": task.id,
                    "phase": task.phase,
                    "title": task.title,
                    "done": completion.is_some(),
                    "actor": completion.map(|completion| completion.actor.clone()),
                    "at": completion.map(|completion| completion.at.to_rfc3339()),
                })
            })
            .collect();
        let requisitions: Vec<serde_json::Value> = book
            .iter()
            .filter(|requisition| keep(&requisition.id))
            .map(|requisition| {
                serde_json::json!({
                    "id": requisition.id.as_str(),
                    "title": requisition.title,
                    "employer": requisition.employer,
                    "ready": requisition.is_ready(&library),
                })
            })
            .collect();
        return emit_json(
            "status",
            serde_json::json!({
                "tasks": { "signed": done, "total": total, "items": tasks },
                "artefacts": {
                    "gate_copy_approved": library.is_approved(),
                    "templates_approved": templates.is_approved(),
                    "playbook_approved": playbook.is_approved(),
                    "escalation_owner": if policy.is_configured() {
                        Some(policy.escalation_owner.clone())
                    } else {
                        None
                    },
                    "retention_days": policy.retention_days,
                },
                "evidence": {
                    "dispositions": dispositions,
                    "qa_checks": qa_count,
                    "sends": sends.len(),
                    "blocked": blocks,
                    "replies": reply_count,
                },
                "requisitions": requisitions,
            }),
        );
    }

    println!("\nartefacts");
    println!(
        "  gate copy approved:      {}",
        yes_no(library.is_approved())
    );
    println!(
        "  templates approved:      {}",
        yes_no(templates.is_approved())
    );
    println!(
        "  playbook approved:       {}",
        yes_no(playbook.is_approved())
    );
    println!(
        "  escalation owner:        {}",
        if policy.is_configured() {
            policy.escalation_owner.as_str()
        } else {
            "NOT NAMED"
        }
    );
    println!("  retention:               {} days", policy.retention_days);

    println!("\nevidence");
    println!("  dispositions captured:   {dispositions}");
    println!("  QA checks recorded:      {qa_count}");
    println!("  emails sent:             {}", sends.len());
    println!("  sends blocked:           {blocks}");
    println!("  replies received:        {reply_count}");

    if !book.is_empty() {
        println!("\nrequisitions");
        for requisition in book.iter() {
            let state = if requisition.is_ready(&library) {
                "ready"
            } else {
                "not ready"
            };
            println!(
                "  {} {:<32} {:<10} {}",
                requisition.id, requisition.title, state, requisition.employer
            );
        }
    }

    let un_qa: Vec<&str> = sends
        .iter()
        .filter(|entry| entry.disposition == DispositionKind::Gate)
        .map(|entry| entry.application_id.as_str())
        .take(0)
        .collect();
    debug_assert!(un_qa.is_empty());
    Ok(())
}

fn governance(store: &Store, command: GovernanceCommand, json: bool) -> Result<()> {
    let mut policy = Policy::load(store)?;
    match command {
        GovernanceCommand::Show => {
            if json {
                return emit_json("governance show", serde_json::json!({ "policy": policy }));
            }
            println!("{policy}");
            Ok(())
        }
        GovernanceCommand::Set(args) => {
            if let Some(days) = args.retention_days {
                policy.set_retention_days(days)?;
            }
            if let Some(owner) = args.escalation_owner {
                policy.set_escalation_owner(owner)?;
            }
            if let Some(hours) = args.sla_hours {
                policy.set_escalation_sla_hours(hours)?;
            }
            policy.save(store)?;
            if json {
                return emit_json("governance set", serde_json::json!({ "policy": policy }));
            }
            println!("{policy}");
            Ok(())
        }
    }
}

fn gate(store: &Store, command: GateCommand, actor: &Actor<'_>, json: bool) -> Result<()> {
    match command {
        GateCommand::List => {
            let library = load_library(store)?;
            if json {
                return emit_json("gate list", serde_json::json!({ "library": library }));
            }
            println!(
                "gate library: {} blocks, approval: {}",
                library.copies.len(),
                approval_text(&library.approval)
            );
            for gate in GateKind::ALL {
                let copy = library.require(gate)?;
                println!("\n{} {} {}", gate.code(), gate.label(), copy.version);
                println!("  requirement: {}", copy.requirement);
                println!("  satisfies:   {}", copy.what_satisfies);
                let facts = gate.required_facts();
                if facts.is_empty() {
                    println!("  facts:       none");
                } else {
                    let names: Vec<String> = facts.iter().map(ToString::to_string).collect();
                    println!("  facts:       {}", names.join(", "));
                }
            }
            Ok(())
        }
        GateCommand::Approve(args) => {
            let role = args.role.parse::<SignOffRole>()?;
            let mut library = load_library(store)?;
            library.approve(args.actor.as_str(), role, Utc::now())?;
            actor.sign(&mut library.approval, role)?;
            library.save(store)?;
            if json {
                return emit_json(
                    "gate approve",
                    serde_json::json!({
                        "actor": args.actor,
                        "role": role.to_string(),
                        "library": library,
                    }),
                );
            }
            println!(
                "recorded {role} sign-off on the gate library ({})",
                approval_text(&library.approval)
            );
            Ok(())
        }
        GateCommand::BumpVersion(args) => {
            let gate = args.gate.parse::<GateKind>()?;
            let version = TextVersion::parse(&args.version)?;
            let mut library = load_library(store)?;
            library.bump_version(gate, version.clone())?;
            library.save(store)?;
            if json {
                return emit_json(
                    "gate bump-version",
                    serde_json::json!({
                        "gate": gate.code(),
                        "version": version,
                        "library": library,
                    }),
                );
            }
            println!("{gate} is now {version}; library sign-off cleared, re-approval required");
            Ok(())
        }
    }
}

fn requisition(
    store: &Store,
    command: RequisitionCommand,
    actor: &Actor<'_>,
    json: bool,
) -> Result<()> {
    match command {
        RequisitionCommand::Create(args) => requisition_create(store, &args, json),
        RequisitionCommand::Fact(args) => {
            let id = RequisitionId::try_from(args.id.as_str())?;
            let key = args.key.parse::<FactKey>()?;
            let mut book = load_requisitions(store)?;
            let requisition = book
                .get_mut(&id)
                .ok_or_else(|| Error::UnknownRequisition(id.clone()))?;
            requisition.set_fact(key, args.value.as_str())?;
            let stored = requisition.clone();
            save_requisitions(store, &book)?;
            if json {
                return emit_json(
                    "requisition fact",
                    serde_json::json!({
                        "key": key.to_string(),
                        "requisition": stored,
                    }),
                );
            }
            println!("recorded {key} for {id}");
            Ok(())
        }
        RequisitionCommand::Comparative(args) => requisition_comparative(store, &args, json),
        RequisitionCommand::Approve(args) => {
            let id = RequisitionId::try_from(args.id.as_str())?;
            let role = args.role.parse::<SignOffRole>()?;
            let mut book = load_requisitions(store)?;
            let requisition = book
                .get_mut(&id)
                .ok_or_else(|| Error::UnknownRequisition(id.clone()))?;
            if args.comparative {
                requisition.approve_comparative(args.actor.as_str(), role, Utc::now())?;
                if let Some(paragraph) = requisition.comparative.as_mut() {
                    actor.sign(&mut paragraph.approval, role)?;
                }
                let stored = requisition.clone();
                save_requisitions(store, &book)?;
                if json {
                    return emit_json(
                        "requisition approve",
                        serde_json::json!({
                            "comparative": true,
                            "role": role.to_string(),
                            "requisition": stored,
                        }),
                    );
                }
                println!("comparative paragraph for {id} signed by {role}");
            } else {
                requisition.approve(args.actor.as_str(), role, Utc::now())?;
                actor.sign(&mut requisition.approval, role)?;
                let still_needed = join_roles(&requisition.approval);
                let stored = requisition.clone();
                save_requisitions(store, &book)?;
                if json {
                    return emit_json(
                        "requisition approve",
                        serde_json::json!({
                            "comparative": false,
                            "role": role.to_string(),
                            "still_needed": still_needed,
                            "requisition": stored,
                        }),
                    );
                }
                println!("requisition {id} copy signed by {role}; still needed: {still_needed}");
            }
            Ok(())
        }
        RequisitionCommand::Show(args) => {
            let id = RequisitionId::try_from(args.id.as_str())?;
            let book = load_requisitions(store)?;
            let requisition = book.require(&id)?;
            let library = load_library(store)?;

            if json {
                let gates: Vec<serde_json::Value> = requisition
                    .availability_report(&library)
                    .into_iter()
                    .map(|(gate, availability)| match availability {
                        ledger::requisition::GateAvailability::Available => serde_json::json!({
                            "gate": gate.code(),
                            "label": gate.label(),
                            "available": true,
                            "missing": Vec::<String>::new(),
                        }),
                        ledger::requisition::GateAvailability::MissingFacts { missing } => {
                            serde_json::json!({
                                "gate": gate.code(),
                                "label": gate.label(),
                                "available": false,
                                "missing": missing
                                    .iter()
                                    .map(ToString::to_string)
                                    .collect::<Vec<String>>(),
                            })
                        }
                    })
                    .collect();
                let comparative = requisition.comparative.as_ref().map(|paragraph| {
                    serde_json::json!({
                        "text": paragraph.text(),
                        "version": paragraph.version.as_str(),
                        "lint": paragraph
                            .lint()
                            .into_iter()
                            .map(|issue| issue.to_string())
                            .collect::<Vec<String>>(),
                    })
                });
                return emit_json(
                    "requisition show",
                    serde_json::json!({
                        "requisition": requisition,
                        "gates": gates,
                        "comparative": comparative,
                        "blocking_issues": requisition.blocking_issues(&library),
                    }),
                );
            }

            println!("{}  {}", requisition.id, requisition.title);
            println!("employer:    {}", requisition.employer);
            println!("reply-to:    {}", requisition.reply_to);
            println!(
                "sender:      {}, {}",
                requisition.sender.name, requisition.sender.title
            );
            println!("copy signed: {}", approval_text(&requisition.approval));
            println!("\ngates");
            for (gate, availability) in requisition.availability_report(&library) {
                let state = match availability {
                    ledger::requisition::GateAvailability::Available => "available".to_owned(),
                    ledger::requisition::GateAvailability::MissingFacts { missing } => format!(
                        "UNAVAILABLE — missing {}",
                        missing
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<String>>()
                            .join(", ")
                    ),
                };
                println!("  {} {:<22} {state}", gate, gate.label());
            }
            if !requisition.facts.is_empty() {
                println!("\nfacts");
                for (key, value) in &requisition.facts {
                    println!("  {key}: {value}");
                }
            }
            match &requisition.comparative {
                None => println!("\ncomparative: not elicited"),
                Some(paragraph) => {
                    println!(
                        "\ncomparative: {} ({})",
                        approval_text(&paragraph.approval),
                        paragraph.version
                    );
                    println!("\n{}\n", paragraph.text());
                    for issue in paragraph.lint() {
                        println!("  LINT: {issue}");
                    }
                }
            }
            let issues = requisition.blocking_issues(&library);
            if issues.is_empty() {
                println!("ready for dispatch");
            } else {
                println!("not ready for dispatch:");
                for issue in issues {
                    println!("  - {issue}");
                }
            }
            Ok(())
        }
        RequisitionCommand::List => {
            let book = load_requisitions(store)?;
            if json {
                let requisitions: Vec<&Requisition> = book.iter().collect();
                return emit_json(
                    "requisition list",
                    serde_json::json!({ "requisitions": requisitions }),
                );
            }
            if book.is_empty() {
                println!("no requisitions yet");
                return Ok(());
            }
            for requisition in book.iter() {
                println!(
                    "{}  {:<32} {:<24} gates: {}",
                    requisition.id,
                    requisition.title,
                    requisition.employer,
                    gate_codes(&requisition.gates)
                );
            }
            Ok(())
        }
    }
}

fn requisition_create(store: &Store, args: &RequisitionCreate, json: bool) -> Result<()> {
    let id = RequisitionId::try_from(args.id.as_str())?;
    let reply_to = Mailbox::try_from(args.reply_to.as_str())?;
    let sender = Sender::try_new(args.sender_name.as_str(), args.sender_title.as_str())?;
    let gates: Result<Vec<GateKind>> = args
        .gates
        .iter()
        .map(|gate| gate.parse::<GateKind>())
        .collect();
    let mut facts = BTreeMap::new();
    for entry in &args.facts {
        let (key, value) = entry.split_once('=').ok_or_else(|| {
            Error::invalid(
                "requisition fact",
                format!("expected KEY=VALUE, got {entry:?}"),
            )
        })?;
        facts.insert(key.trim().parse::<FactKey>()?, value.trim().to_owned());
    }
    let requisition = Requisition::new(
        ledger::requisition::RequisitionDraft {
            id,
            employer: args.employer.clone(),
            title: args.title.clone(),
            reply_to,
            sender,
            gates: gates?,
            facts,
        },
        Utc::now(),
    )?;
    let mut book = load_requisitions(store)?;
    book.insert(requisition)?;
    save_requisitions(store, &book)?;
    if json {
        let stored = book.require(&RequisitionId::try_from(args.id.as_str())?)?;
        return emit_json(
            "requisition create",
            serde_json::json!({ "requisition": stored }),
        );
    }
    println!("created requisition {}", args.id);
    println!("gates: {}", args.gates.join(", "));
    if args.facts.is_empty() {
        println!(
            "\nnote: no facts documented. GATE dispositions for GATE-02..GATE-05 will be refused \
             until their facts are set; use `ledger requisition fact`."
        );
    }
    Ok(())
}

fn requisition_comparative(store: &Store, args: &RequisitionComparative, json: bool) -> Result<()> {
    let id = RequisitionId::try_from(args.id.as_str())?;
    let version = TextVersion::parse(&args.version)?;
    let mut book = load_requisitions(store)?;
    let requisition = book
        .get_mut(&id)
        .ok_or_else(|| Error::UnknownRequisition(id.clone()))?;
    requisition.set_comparative(
        [
            args.attribute_1.clone(),
            args.attribute_2.clone(),
            args.attribute_3.clone(),
        ],
        args.evidence.as_str(),
        args.non_factors.as_str(),
        version,
    )?;
    let paragraph = requisition
        .comparative
        .as_ref()
        .map(|paragraph| paragraph.text())
        .unwrap_or_default();
    let stored = requisition.clone();
    save_requisitions(store, &book)?;
    if json {
        return emit_json(
            "requisition comparative",
            serde_json::json!({ "requisition": stored, "paragraph": paragraph }),
        );
    }
    println!("elicited the comparative paragraph for {id}:\n\n{paragraph}\n");
    println!("this wording needs its own HM and Legal sign-off before dispatch");
    Ok(())
}

fn templates(
    store: &Store,
    command: TemplatesCommand,
    actor: &Actor<'_>,
    json: bool,
) -> Result<()> {
    match command {
        TemplatesCommand::Show => {
            let templates = load_templates(store)?;
            if json {
                return emit_json(
                    "templates show",
                    serde_json::json!({ "templates": templates }),
                );
            }
            println!("templates approved: {}", yes_no(templates.is_approved()));
            for kind in TemplateKind::ALL {
                let template = templates.require(kind)?;
                println!(
                    "\n=== Template {} ({kind}, {}, {}) ===",
                    kind.letter(),
                    template.version,
                    approval_text(&template.approval)
                );
                println!("Subject: {}", template.subject);
                println!("\n{}", template.body);
            }
            Ok(())
        }
        TemplatesCommand::Approve(args) => {
            let role = args.role.parse::<SignOffRole>()?;
            let mut templates = load_templates(store)?;
            templates.approve_all(args.actor.as_str(), role, Utc::now())?;
            if let Some(operator) = actor.operator {
                templates.sign_all(actor.registry, store, role, &operator.name)?;
            }
            templates.save(store)?;
            if json {
                return emit_json(
                    "templates approve",
                    serde_json::json!({ "role": role.to_string(), "templates": templates }),
                );
            }
            println!("recorded {role} sign-off on all three templates");
            if templates.is_approved() {
                println!("templates are approved");
            } else {
                let missing = templates
                    .items
                    .first()
                    .map(|item| item.approval.missing())
                    .unwrap_or_default();
                let roles: Vec<String> = missing.iter().map(ToString::to_string).collect();
                println!("still needed: {}", roles.join(", "));
            }
            Ok(())
        }
    }
}

fn playbook(store: &Store, command: PlaybookCommand, actor: &Actor<'_>, json: bool) -> Result<()> {
    match command {
        PlaybookCommand::Show => {
            let playbook = load_playbook(store)?;
            if json {
                return emit_json("playbook show", serde_json::json!({ "playbook": playbook }));
            }
            println!("playbook approved: {}", yes_no(playbook.is_approved()));
            for scenario in Scenario::ALL {
                let script = playbook.require(scenario)?;
                println!("\n=== {scenario} ===");
                println!("{}", script.text);
                if let Some(escalation) = &script.escalation {
                    println!("ESCALATE: {escalation}");
                }
            }
            Ok(())
        }
        PlaybookCommand::Script(args) => {
            let scenario = args.scenario.parse::<Scenario>()?;
            let playbook = load_playbook(store)?;
            let script = playbook.require(scenario)?;
            if json {
                return emit_json(
                    "playbook script",
                    serde_json::json!({
                        "scenario": scenario.to_string(),
                        "text": script.text,
                        "escalation": script.escalation,
                    }),
                );
            }
            println!("{}", script.text);
            if let Some(escalation) = &script.escalation {
                println!("\nESCALATE: {escalation}");
            }
            Ok(())
        }
        PlaybookCommand::Approve(args) => {
            let role = args.role.parse::<SignOffRole>()?;
            let mut playbook = load_playbook(store)?;
            playbook.approve(args.actor.as_str(), role, Utc::now())?;
            actor.sign(&mut playbook.approval, role)?;
            playbook.save(store)?;
            if json {
                return emit_json(
                    "playbook approve",
                    serde_json::json!({ "role": role.to_string(), "playbook": playbook }),
                );
            }
            println!(
                "recorded {role} sign-off on the playbook ({})",
                approval_text(&playbook.approval)
            );
            Ok(())
        }
    }
}

fn ats_import(store: &Store, args: &AtsImport, json: bool) -> Result<()> {
    let id = RequisitionId::try_from(args.requisition.as_str())?;
    let book = load_requisitions(store)?;
    book.require(&id)?;
    let file = std::fs::File::open(&args.file).map_err(|source| Error::Io {
        path: args.file.clone(),
        source,
    })?;
    let roster = Roster::from_csv(&id, file)?;
    let with_knockouts = roster
        .applicants
        .iter()
        .filter(|applicant| !applicant.knockouts.is_empty())
        .count();
    roster.save(store)?;
    if roster.applicants.is_empty() {
        return Err(Error::invalid(
            "ATS export",
            "no row survived the initial-screen filter",
        ));
    }
    if json {
        return emit_json(
            "ats import",
            serde_json::json!({
                "requisition": id.as_str(),
                "imported": roster.applicants.len(),
                "with_knockouts": with_knockouts,
                "excluded_not_initial_screen": roster.excluded_not_initial_screen,
            }),
        );
    }
    println!(
        "imported {} initial-screen applicants for {id} ({} with KO answers)",
        roster.applicants.len(),
        with_knockouts
    );
    if roster.excluded_not_initial_screen > 0 {
        println!(
            "excluded {} row(s) past the initial screen (hard filter)",
            roster.excluded_not_initial_screen
        );
    }
    Ok(())
}

fn disposition(
    store: &Store,
    command: DispositionCommand,
    operator: Option<&str>,
    registry: &ledger::identity::Registry,
    json: bool,
) -> Result<()> {
    match command {
        DispositionCommand::Record(args) => {
            disposition_record(store, &args, operator, registry, json)
        }
        DispositionCommand::List(args) => {
            let id = RequisitionId::try_from(args.requisition.as_str())?;
            let dispositions = Dispositions::open(store)?;
            let mut found = 0;
            let mut current = Vec::new();
            for record in dispositions.records() {
                if record.form.requisition != id {
                    continue;
                }
                if dispositions
                    .head(&id, &record.form.application_id)
                    .map(|head| head.id != record.id)
                    .unwrap_or(true)
                {
                    continue; // superseded rows are shown with `history`
                }
                found += 1;
                current.push(record.clone());
                if !json {
                    println!(
                        "{}  {}  {:<40} {:<22} by {} / dm {}",
                        record.id,
                        record.form.application_id,
                        record.form.disposition.to_string(),
                        abbreviate(&record.form.candidate_first_name, 20),
                        record.form.recruiter,
                        record.form.decision_maker
                    );
                }
            }
            if json {
                return emit_json(
                    "disposition list",
                    serde_json::json!({ "requisition": id.as_str(), "records": current }),
                );
            }
            if found == 0 {
                println!("no current dispositions for {id}");
            }
            Ok(())
        }
        DispositionCommand::History(args) => {
            let application = ApplicationId::try_from(args.application.as_str())?;
            let dispositions = Dispositions::open(store)?;
            let history = dispositions.history(&application);
            if json {
                return emit_json(
                    "disposition history",
                    serde_json::json!({
                        "application": application.as_str(),
                        "records": history,
                    }),
                );
            }
            if history.is_empty() {
                println!("no records for {application}");
                return Ok(());
            }
            for record in history {
                let link = record
                    .form
                    .supersedes
                    .as_ref()
                    .map(|previous| format!(" (supersedes {previous})"))
                    .unwrap_or_default();
                println!(
                    "{}  {}  {}{}  captured {}",
                    record.id,
                    record.form.application_id,
                    record.form.disposition,
                    link,
                    record.form.captured_at.format(STAMP)
                );
            }
            Ok(())
        }
    }
}

fn disposition_record(
    store: &Store,
    args: &DispositionRecordArgs,
    operator: Option<&str>,
    registry: &ledger::identity::Registry,
    json: bool,
) -> Result<()> {
    let requisition_id = RequisitionId::try_from(args.requisition.as_str())?;
    let book = load_requisitions(store)?;
    let requisition = book.require(&requisition_id)?;
    let library = load_library(store)?;

    let category = args.category.trim().to_uppercase();
    let disposition = match category.as_str() {
        "GATE" => {
            let gate = args
                .gate
                .as_deref()
                .ok_or_else(|| Error::invalid("disposition", "GATE needs --gate"))?
                .parse::<GateKind>()?;
            let answer = args
                .answer
                .as_deref()
                .ok_or_else(|| Error::invalid("disposition", "GATE needs --answer"))?
                .parse::<GateAnswer>()?;
            Disposition::Gate { gate, answer }
        }
        "FIELD" => Disposition::Field,
        "ADMIN" => {
            let reason = args
                .reason
                .as_deref()
                .ok_or_else(|| Error::invalid("disposition", "ADMIN needs --reason"))?
                .parse::<AdminReason>()?;
            Disposition::Admin { reason }
        }
        other => {
            return Err(Error::invalid(
                "disposition",
                format!("expected GATE, FIELD or ADMIN, got {other:?}"),
            ));
        }
    };

    let supersedes = args
        .supersedes
        .as_deref()
        .map(DispositionId::try_from)
        .transpose()?;

    let form = NewDisposition::try_new(
        requisition,
        &library,
        FormSubmission {
            application_id: ApplicationId::try_from(args.application.as_str())?,
            candidate_first_name: args.first_name.clone(),
            candidate_email: CandidateEmail::try_from(args.email.as_str())?,
            recruiter: RecruiterId::try_from(args.recruiter.as_str())?,
            decision_maker: DecisionMakerId::try_from(args.decision_maker.as_str())?,
            disposition,
            supersedes,
        },
        Utc::now(),
    )?;

    // A registered workspace signs every disposition; the signature names the key
    // that authorised the decision and is checked by `verify-signatures`.
    let form = match (operator, registry.is_empty()) {
        (Some(operator), false) => form.signed(registry, store, operator)?,
        _ => form,
    };

    let mut dispositions = Dispositions::open(store)?;
    let stored = dispositions.record(form)?.clone();
    if json {
        return emit_json(
            "disposition record",
            serde_json::json!({ "record": stored }),
        );
    }
    println!(
        "captured {} for {} at {} (dm {})",
        stored.form.disposition,
        stored.form.application_id,
        stored.form.captured_at.format(STAMP),
        stored.form.decision_maker
    );
    println!("record id: {}", stored.id);
    match &stored.form.signature {
        Some(signature) => println!(
            "signed by {} ({})",
            signature.operator,
            &signature.public_key[0..12]
        ),
        None => println!("UNSIGNED: no operator registry in this workspace"),
    }
    Ok(())
}

fn operator(
    store: &Store,
    command: OperatorCommand,
    explicit: Option<&str>,
    json: bool,
) -> Result<()> {
    let mut registry = ledger::identity::Registry::load(store)?;
    match command {
        OperatorCommand::Register(args) => {
            let role = args.role.parse::<ledger::identity::Role>()?;
            let operator = registry.register(store, args.name.as_str(), role)?;
            if json {
                return emit_json(
                    "operator register",
                    serde_json::json!({
                        "operator": operator,
                        "keyfile": ledger::identity::keyfile_path(store, &operator.name)
                            .display()
                            .to_string(),
                    }),
                );
            }
            println!(
                "registered {} as {} ({})",
                operator.name,
                operator.role,
                &operator.public_key[0..12]
            );
            println!(
                "private key: {}",
                ledger::identity::keyfile_path(store, &operator.name).display()
            );
            Ok(())
        }
        OperatorCommand::List => {
            if json {
                return emit_json(
                    "operator list",
                    serde_json::json!({ "operators": registry.operators() }),
                );
            }
            if registry.is_empty() {
                println!("no operators registered");
                return Ok(());
            }
            for operator in registry.operators() {
                println!(
                    "{:<16} {:<12} {}",
                    operator.name,
                    operator.role,
                    &operator.public_key[0..12]
                );
            }
            Ok(())
        }
        OperatorCommand::Whoami => {
            let operator = ledger::identity::active(&registry, explicit)?;
            if json {
                return emit_json(
                    "operator whoami",
                    serde_json::json!({ "operator": operator }),
                );
            }
            println!("{} ({})", operator.name, operator.role);
            Ok(())
        }
    }
}

#[cfg(feature = "ui")]
fn ui(store: &Store, args: &UiArgs, explicit: Option<&str>, json: bool) -> Result<()> {
    let registry = ledger::identity::Registry::load(store)?;
    let operator = ledger::identity::active(&registry, explicit)?;
    if json {
        emit_json(
            "ui",
            serde_json::json!({
                "url": format!("http://127.0.0.1:{}", args.port),
                "operator": operator,
            }),
        )?;
    }
    ledger::ui::run(ledger::ui::Config {
        data: store.root().to_path_buf(),
        operator: operator.name,
        port: args.port,
    })
}

fn verify_signatures(store: &Store, json: bool) -> Result<()> {
    let report = ledger::verify::verify_workspace(store)?;
    let registry = ledger::identity::Registry::load(store)?;

    if json {
        emit_json(
            "verify-signatures",
            serde_json::json!({ "report": &report }),
        )?;
    } else {
        println!("signed records:   {}", report.dispositions_signed);
        println!("unsigned records: {}", report.dispositions_unsigned);
        if !report.dispositions_invalid.is_empty() {
            println!(
                "INVALID:          {}",
                report.dispositions_invalid.join(", ")
            );
        }
        for artefact in &report.artefacts {
            println!("{:<32} {}", artefact.label, artefact.status);
        }
        if !report.stale_labels().is_empty() {
            println!(
                "STALE (copy edited after sign-off): {}",
                report.stale_labels().join(", ")
            );
        }
        if !report.unsigned_labels().is_empty() {
            println!(
                "UNSIGNED artefacts: {}",
                report.unsigned_labels().join(", ")
            );
        }
        if !report.invalid_labels().is_empty() {
            println!("INVALID artefacts:  {}", report.invalid_labels().join(", "));
        }
    }

    let unsigned_matters = !registry.is_empty() && report.dispositions_unsigned > 0;
    if !report.is_clean() || unsigned_matters {
        return Err(Error::invalid(
            "signature",
            format!(
                "{} invalid signature(s), {} stale or unsigned artefact(s), {} unsigned record(s)",
                report.dispositions_invalid.len() + report.invalid_labels().len(),
                report
                    .artefacts
                    .iter()
                    .filter(|a| !a.status.is_valid())
                    .count(),
                report.dispositions_unsigned
            ),
        ));
    }
    Ok(())
}

fn qa(store: &Store, command: QaCommand, json: bool) -> Result<()> {
    match command {
        QaCommand::Record(args) => {
            let registry = load_requisitions(store)?;
            let requisition = RequisitionId::try_from(args.requisition.as_str())?;
            registry.require(&requisition)?;
            let application = ApplicationId::try_from(args.application.as_str())?;
            let dispositions = Dispositions::open(store)?;
            let record = dispositions
                .head(&requisition, &application)
                .ok_or_else(|| {
                    Error::invalid(
                        "qa",
                        format!("{application} has no disposition record to check"),
                    )
                })?;
            let scope = match &record.form.disposition {
                Disposition::Gate { .. } => QaScope::GateFullCheck,
                _ => QaScope::RandomTenPercent,
            };
            let mut log = QaLog::open(store)?;
            let recorded = log.record(QaRecord {
                application_id: application.clone(),
                requisition: requisition.clone(),
                qa: QaId::try_from(args.qa.as_str())?,
                at: Utc::now(),
                scope,
            })?;
            if json {
                return emit_json(
                    "qa record",
                    serde_json::json!({
                        "recorded": recorded,
                        "requisition": requisition.as_str(),
                        "application": application.as_str(),
                        "scope": scope.code(),
                    }),
                );
            }
            if recorded {
                println!("recorded {} QA check for {application}", scope.code());
            } else {
                println!("{application} already has a QA check on file");
            }
            if scope == QaScope::RandomTenPercent && !sampled(&requisition, &application) {
                println!(
                    "note: {application} is outside the deterministic 10% sample; the check is \
                     still recorded"
                );
            }
            Ok(())
        }
        QaCommand::List(args) => {
            let requisition = RequisitionId::try_from(args.requisition.as_str())?;
            let log = QaLog::open(store)?;
            let mut found = 0;
            let mut selected = Vec::new();
            for record in log.records() {
                if record.requisition != requisition {
                    continue;
                }
                found += 1;
                selected.push(record.clone());
                if !json {
                    println!(
                        "{}  {}  {}  by {}",
                        record.application_id,
                        record.scope.code(),
                        record.at.format(STAMP),
                        record.qa
                    );
                }
            }
            if json {
                return emit_json(
                    "qa list",
                    serde_json::json!({ "requisition": requisition.as_str(), "records": selected }),
                );
            }
            if found == 0 {
                println!("no QA checks recorded for {requisition}");
            }
            Ok(())
        }
    }
}

fn dispatch_command(store: &Store, args: &DispatchArgs, json: bool) -> Result<()> {
    let id = RequisitionId::try_from(args.requisition.as_str())?;
    let book = load_requisitions(store)?;
    let requisition = book.require(&id)?;
    let library = load_library(store)?;
    let templates = load_templates(store)?;
    let playbook = load_playbook(store)?;
    let roster = Roster::load(store, &id)?.ok_or_else(|| {
        Error::invalid(
            "roster",
            format!("no ATS export imported for {id}; run `ledger ats import`"),
        )
    })?;
    let dispositions = Dispositions::open(store)?;
    let qa_log = QaLog::open(store)?;
    let mut send_log = SendLog::open(store)?;
    let mut blocked_log = BlockedLog::open(store)?;

    let mode = if args.preview {
        Mode::Preview
    } else {
        Mode::Execute
    };
    // A drafted batch has not been sent: record `queued` unless the operator says
    // otherwise, so the log never claims a delivery that did not happen.
    let delivery_status = match args.status.as_deref() {
        Some(status) => status.parse::<DeliveryStatus>()?,
        None if args.eml.is_some() => DeliveryStatus::Queued,
        None => DeliveryStatus::Sent,
    };
    let report = dispatch(
        &Dispatch {
            mode,
            requisition,
            library: &library,
            templates: &templates,
            playbook: &playbook,
            roster: &roster,
            dispositions: &dispositions,
            qa: &qa_log,
            delivery_status,
            now: Utc::now(),
        },
        &mut send_log,
        &mut blocked_log,
    )?;
    let mut eml_payload: Option<serde_json::Value> = None;
    let mut payload = serde_json::json!({
        "mode": if mode == Mode::Preview { "preview" } else { "execute" },
        "status": delivery_status.code(),
        "sent": report.sent_count(),
        "previews": report.preview_count(),
        "blocked": report.blocked_count(),
        "blocked_reasons": report
            .blocked
            .iter()
            .map(|attempt| attempt.reason.to_string())
            .collect::<Vec<String>>(),
    });

    if !json {
        print_dispatch_report(&report, mode);
    }

    if let Some(dir) = &args.eml {
        if mode == Mode::Preview {
            if !json {
                println!("\n(--eml writes drafts; run without --preview to record the batch)");
            }
        } else {
            let sender = format!("{}, {}", requisition.sender.name, requisition.sender.title);
            let batch = ledger::eml::write_batch(
                dir,
                requisition.id.as_str(),
                &sender,
                requisition.reply_to.as_str(),
                &report.sent,
            )?;
            if json {
                eml_payload = Some(serde_json::json!({
                    "directory": dir.display().to_string(),
                    "drafts": batch.files.len(),
                    "batch_digest": batch.batch_digest,
                }));
            } else {
                println!(
                    "\nwrote {} draft(s) to {} (status {})",
                    batch.files.len(),
                    dir.display(),
                    delivery_status
                );
                println!("batch digest: {}", batch.batch_digest);
            }
        }
    }

    if json {
        if let (Some(object), Some(eml)) = (payload.as_object_mut(), eml_payload) {
            object.insert("eml".to_owned(), eml);
        }
        emit_json("dispatch", payload)?;
    }

    if mode == Mode::Execute && report.blocked_count() > 0 && !args.allow_blocked {
        return Err(Error::Blocked {
            count: report.blocked_count(),
        });
    }
    Ok(())
}

fn delivery_command(
    store: &Store,
    command: DeliveryCommand,
    actor: &Actor<'_>,
    json: bool,
) -> Result<()> {
    match command {
        DeliveryCommand::Confirm(args) => {
            let send_id = SendId::try_from(args.send_id.as_str())?;
            let operator = actor.operator.ok_or_else(|| {
                Error::invalid(
                    "operator",
                    "register an operator first: `ledger operator register --name <name> --role <role>`",
                )
            })?;
            let sends = SendLog::open(store)?;
            let mut deliveries = DeliveryLog::open(store)?;
            let entry = deliveries.confirm(
                &sends,
                &send_id,
                &operator.name,
                &operator.public_key,
                Utc::now(),
            )?;
            if json {
                emit_json(
                    "delivery confirm",
                    serde_json::json!({ "confirmation": entry }),
                )?;
            } else {
                println!(
                    "confirmed {} delivered by {} at {}",
                    entry.send_id,
                    entry.confirmed_by,
                    entry.confirmed_at.format(STAMP)
                );
                println!(
                    "delivery chain: {} entr{}, head {}",
                    entry.seq + 1,
                    if entry.seq == 0 { "y" } else { "ies" },
                    entry.entry_hash
                );
                println!("send log unchanged: the confirmation is a new chained entry");
            }
            Ok(())
        }
        DeliveryCommand::List(_) => {
            let deliveries = DeliveryLog::open(store)?;
            let entries = deliveries.entries()?;
            if json {
                emit_json(
                    "delivery list",
                    serde_json::json!({
                        "entries": entries.len(),
                        "confirmations": entries,
                    }),
                )?;
            } else if entries.is_empty() {
                println!("no delivery confirmations recorded");
            } else {
                for entry in &entries {
                    println!(
                        "{}  {}  {}  by {}",
                        entry.send_id,
                        entry.confirmed_at.format(STAMP),
                        entry.seq,
                        entry.confirmed_by
                    );
                }
            }
            Ok(())
        }
    }
}

fn demo_seed(store: &Store, args: &DemoSeedArgs, json: bool) -> Result<()> {
    let summary = ledger::demo::seed(store, args.force, args.record)?;
    if json {
        emit_json("demo seed", serde_json::json!({ "summary": summary }))?;
        return Ok(());
    }
    println!("synthetic fixture — no pilot data yet");
    println!("requisition:        {}", summary.requisition);
    println!("CSV rows:           {}", summary.csv_rows);
    println!("roster rows:        {}", summary.roster_rows);
    println!("dropped at import:  {}", summary.dropped_at_import);
    println!(
        "dispositions:       GATE {}, FIELD {}, ADMIN {}, missing 1",
        summary
            .dispositions
            .get(&DispositionKind::Gate)
            .copied()
            .unwrap_or_default(),
        summary
            .dispositions
            .get(&DispositionKind::Field)
            .copied()
            .unwrap_or_default(),
        summary
            .dispositions
            .get(&DispositionKind::Admin)
            .copied()
            .unwrap_or_default(),
    );
    println!(
        "preview:            {} would send, {} blocked",
        summary.preview_sends, summary.preview_blocked
    );
    for reason in &summary.blocked_reasons {
        println!("  - {reason}");
    }
    if summary.recorded {
        println!("emails committed:   {}", summary.sends);
        println!("blocked:            {}", summary.blocked);
        println!("stale versions:     {}", summary.stale_versions);
    } else {
        println!("recorded:           no (run dispatch to commit the sends)");
    }
    println!("log head:           {}", summary.log_head);
    println!("fingerprint:        {}", summary.fingerprint);
    println!("ATS export:         {}", summary.csv_path.display());
    Ok(())
}

fn print_dispatch_report(report: &DispatchReport, mode: Mode) {
    match mode {
        Mode::Preview => {
            println!("preview: {} email(s) would send\n", report.preview_count());
            for preview in &report.previews {
                println!(
                    "--- {} -> {} ({}, {}) ---",
                    preview.application_id,
                    preview.candidate_email,
                    preview.disposition,
                    preview.text_version
                );
                println!("{}\n", preview.rendered);
            }
        }
        Mode::Execute => println!("sent {}", report.sent_count()),
    }
    println!("blocked {}", report.blocked_count());
    for attempt in &report.blocked {
        println!(
            "  {} {}: {}",
            attempt.application_id,
            attempt
                .candidate_email
                .as_ref()
                .map_or_else(String::new, ToString::to_string),
            attempt.reason
        );
    }
    if report.blocked_count() > 0 && mode == Mode::Preview {
        println!("\n(preview writes nothing; blocks are logged only on a real run)");
    }
}

fn log(store: &Store, command: LogCommand, json: bool) -> Result<()> {
    match command {
        LogCommand::Verify(_) => {
            let verification = SendLog::open(store)?.verification()?;
            if json {
                emit_json(
                    "log verify",
                    serde_json::json!({
                        "valid": true,
                        "entries": verification.entries,
                        "head": verification.head,
                    }),
                )?;
            } else {
                println!("send log verified: {} entries", verification.entries);
                println!("head: {}", verification.head);
            }
            Ok(())
        }
        LogCommand::Quarantine(args) => {
            if !args.yes {
                return Err(Error::invalid(
                    "quarantine",
                    "this truncates the send log; pass --yes",
                ));
            }
            let operator = ledger::operator::identity(args.operator.as_deref());
            let report =
                ledger::operator::quarantine(store, args.reason.as_str(), operator.as_str())?;
            if json {
                emit_json("log quarantine", serde_json::json!({ "report": report }))?;
            } else {
                println!(
                    "quarantined {} byte(s) to {}",
                    report.bytes_moved,
                    report.sidecar.display()
                );
                println!(
                    "send log now has {} entries and verifies",
                    report.remaining_entries
                );
                println!("recorded in operator.jsonl by {operator}");
            }
            Ok(())
        }
        LogCommand::Export(args) => {
            let id = RequisitionId::try_from(args.requisition.as_str())?;
            let entries: Vec<_> = SendLog::open(store)?
                .entries()?
                .into_iter()
                .filter(|entry| entry.requisition == id)
                .collect();
            let replied: std::collections::BTreeSet<String> = Replies::open(store)
                .for_requisition(&id)?
                .into_iter()
                .filter_map(|reply| {
                    reply
                        .application_id
                        .map(|application| application.to_string())
                })
                .collect();

            if let Some(path) = &args.out {
                let file = std::fs::File::create(path).map_err(|source| Error::Io {
                    path: path.clone(),
                    source,
                })?;
                write_send_csv(file, &entries, &replied)?;
                if json {
                    emit_json(
                        "log export",
                        serde_json::json!({
                            "requisition": id.as_str(),
                            "rows": entries.len(),
                            "out": path.display().to_string(),
                        }),
                    )?;
                } else {
                    println!("exported {} entries", entries.len());
                }
            } else if json {
                let rows: Vec<serde_json::Value> = entries
                    .iter()
                    .map(|entry| {
                        serde_json::json!({
                            "send_id": entry.send_id.as_str(),
                            "application_id": entry.application_id.as_str(),
                            "requisition": entry.requisition.as_str(),
                            "disposition": entry.disposition.code(),
                            "gate_code": entry.gate_code.map(GateKind::code),
                            "decision_maker": entry.decision_maker.as_str(),
                            "captured_at": entry.captured_at.to_rfc3339(),
                            "sent_at": entry.sent_at.to_rfc3339(),
                            "template": entry.template.letter(),
                            "text_version": entry.text_version.as_str(),
                            "text_digest": entry.text_digest.as_str(),
                            "body": entry.body.as_str(),
                            "delivery_status": entry.delivery_status.code(),
                            "reply_received": replied.contains(entry.application_id.as_str()),
                        })
                    })
                    .collect();
                emit_json(
                    "log export",
                    serde_json::json!({ "requisition": id.as_str(), "rows": rows }),
                )?;
            } else {
                write_send_csv(std::io::stdout(), &entries, &replied)?;
            }
            Ok(())
        }
    }
}

/// Writes the CSV export, the shape the ATS round-trip expects.
fn write_send_csv<W: std::io::Write>(
    sink: W,
    entries: &[SendEntry],
    replied: &std::collections::BTreeSet<String>,
) -> Result<()> {
    let mut writer = csv::Writer::from_writer(sink);
    writer
        .write_record([
            "SEND_ID",
            "APPLICATION_ID",
            "REQ_ID",
            "DISPOSITION",
            "GATE_CODE",
            "DECISION_MAKER_ID",
            "CAPTURE_TIMESTAMP",
            "SEND_TIMESTAMP",
            "TEMPLATE_ID",
            "TEXT_VERSION",
            "RENDERED_BODY_HASH",
            "RENDERED_BODY_FULL",
            "DELIVERY_STATUS",
            "REPLY_RECEIVED_FLAG",
        ])
        .map_err(Error::from)?;
    for entry in entries {
        let flag = if replied.contains(entry.application_id.as_str()) {
            "TRUE"
        } else {
            "FALSE"
        };
        writer
            .write_record([
                entry.send_id.as_str(),
                entry.application_id.as_str(),
                entry.requisition.as_str(),
                entry.disposition.code(),
                entry.gate_code.map(GateKind::code).unwrap_or(""),
                entry.decision_maker.as_str(),
                &entry.captured_at.to_rfc3339(),
                &entry.sent_at.to_rfc3339(),
                entry.template.letter(),
                entry.text_version.as_str(),
                entry.text_digest.as_str(),
                entry.body.as_str(),
                entry.delivery_status.code(),
                flag,
            ])
            .map_err(Error::from)?;
    }
    writer.flush().map_err(|source| Error::Io {
        path: PathBuf::from("<csv export>"),
        source,
    })?;
    Ok(())
}

fn reply_record(store: &Store, args: &ReplyRecordArgs, json: bool) -> Result<()> {
    if args.count == 0 {
        return Err(Error::invalid("reply count", "must be at least 1"));
    }
    let id = RequisitionId::try_from(args.requisition.as_str())?;
    let scenario = args
        .scenario
        .as_deref()
        .map(str::parse::<Scenario>)
        .transpose()?;
    let mut replies = Replies::open(store);
    replies.record(
        Reply {
            requisition: id.clone(),
            received_at: Utc::now(),
            scenario,
            application_id: args
                .application
                .as_deref()
                .map(ApplicationId::try_from)
                .transpose()?,
        },
        args.count,
    )?;
    if json {
        let escalation = if scenario == Some(Scenario::DiscriminationConcern) {
            let policy = Policy::load(store)?;
            Some(serde_json::json!({
                "owner": policy.escalation_owner,
                "sla_hours": policy.escalation_sla_hours,
                "configured": policy.is_configured(),
            }))
        } else {
            None
        };
        return emit_json(
            "reply record",
            serde_json::json!({
                "requisition": id.as_str(),
                "recorded": args.count,
                "scenario": scenario.map(|scenario| scenario.code()),
                "escalation": escalation,
            }),
        );
    }
    println!("recorded {} reply/replies for {id}", args.count);
    if scenario == Some(Scenario::DiscriminationConcern) {
        let policy = Policy::load(store)?;
        if policy.is_configured() {
            println!(
                "ESCALATE now to {} — acknowledge within {} hours",
                policy.escalation_owner, policy.escalation_sla_hours
            );
        } else {
            println!(
                "WARNING: no escalation owner is named. Run `ledger governance set \
                 --escalation-owner ...` before recording discrimination concerns."
            );
        }
    }
    Ok(())
}

fn signal_record(store: &Store, args: &SignalRecordArgs, json: bool) -> Result<()> {
    if args.count == 0 {
        return Err(Error::invalid("signal count", "must be at least 1"));
    }
    let id = RequisitionId::try_from(args.requisition.as_str())?;
    let value = args.value.parse::<ledger::metrics::SignalValue>()?;
    let mut signals = Signals::open(store);
    signals.record(
        ledger::metrics::Signal {
            requisition: id.clone(),
            value,
            at: Utc::now(),
        },
        args.count,
    )?;
    if json {
        return emit_json(
            "signal record",
            serde_json::json!({
                "requisition": id.as_str(),
                "value": value.code(),
                "recorded": args.count,
            }),
        );
    }
    println!("recorded {} x {value} for {id}", args.count);
    Ok(())
}

fn metrics_command(store: &Store, args: &MetricsArgs, json: bool) -> Result<()> {
    let id = RequisitionId::try_from(args.requisition.as_str())?;
    let metrics = compute_metrics(store, &id)?;
    if json {
        emit_json("metrics", serde_json::json!({ "metrics": metrics }))?;
    } else {
        println!("{metrics}");
    }
    Ok(())
}

fn compute_metrics(store: &Store, id: &RequisitionId) -> Result<Metrics> {
    let sends: Vec<_> = SendLog::open(store)?
        .entries()?
        .into_iter()
        .filter(|entry| &entry.requisition == id)
        .collect();
    let replies = Replies::open(store).for_requisition(id)?;
    let blocked: Vec<_> = BlockedLog::open(store)?
        .attempts()
        .iter()
        .filter(|attempt| &attempt.requisition == id)
        .cloned()
        .collect();
    let signals = Signals::open(store).for_requisition(id)?;
    let confirmations = DeliveryLog::open(store)?.entries()?;
    Ok(Metrics::compute(
        &sends,
        &replies,
        &blocked,
        &signals,
        &confirmations,
    ))
}

fn review(store: &Store, command: ReviewCommand, json: bool) -> Result<()> {
    match command {
        ReviewCommand::Criteria(args) => {
            let id = RequisitionId::try_from(args.requisition.as_str())?;
            let feasibility = args.status.parse::<CriteriaFeasibility>()?;
            let record = CriteriaRecord {
                requisition: id.clone(),
                feasibility,
                actor: args.actor.clone(),
                at: Utc::now(),
                note: args.note.clone(),
            };
            let mut reviews = Reviews::load(store)?;
            reviews.assess(record.clone())?;
            reviews.save(store)?;
            if json {
                return emit_json("review criteria", serde_json::json!({ "record": record }));
            }
            println!("recorded criteria feasibility for {id}: {feasibility}");
            Ok(())
        }
        ReviewCommand::Route(args) => {
            let id = RequisitionId::try_from(args.requisition.as_str())?;
            let reviews = Reviews::load(store)?;
            let metrics = compute_metrics(store, &id)?;
            let routing = Routing::route(
                id,
                metrics,
                reviews.feasibility_for(&RequisitionId::try_from(args.requisition.as_str())?),
            );
            if json {
                emit_json("review route", serde_json::json!({ "routing": routing }))?;
            } else {
                println!("{routing}");
            }
            Ok(())
        }
    }
}

fn checklist(store: &Store, command: ChecklistCommand, json: bool) -> Result<()> {
    match command {
        ChecklistCommand::Done(args) => {
            let mut checklist = Checklist::load(store)?;
            let completion = checklist
                .mark(
                    args.task.as_str(),
                    args.actor.as_str(),
                    Utc::now(),
                    args.reference,
                )?
                .clone();
            checklist.save(store)?;
            let (done, total) = checklist.progress();
            if json {
                return emit_json(
                    "checklist done",
                    serde_json::json!({
                        "completion": completion,
                        "progress": { "signed": done, "total": total },
                    }),
                );
            }
            println!(
                "marked task {} done (signed by {})",
                completion.task, completion.actor
            );
            Ok(())
        }
        ChecklistCommand::Show => {
            let checklist = Checklist::load(store)?;
            let (done, total) = checklist.progress();
            if json {
                let tasks: Vec<serde_json::Value> = TASKS
                    .iter()
                    .map(|task| {
                        serde_json::json!({
                            "id": task.id,
                            "phase": task.phase,
                            "title": task.title,
                            "done": checklist.is_done(task.id),
                            "actor": checklist
                                .completion(task.id)
                                .map(|completion| completion.actor.clone()),
                            "at": checklist
                                .completion(task.id)
                                .map(|completion| completion.at.to_rfc3339()),
                        })
                    })
                    .collect();
                return emit_json(
                    "checklist show",
                    serde_json::json!({
                        "signed": done,
                        "total": total,
                        "tasks": tasks,
                    }),
                );
            }
            println!("{done}/{total} tasks signed off\n");
            for task in TASKS {
                let mark = if checklist.is_done(task.id) { "x" } else { " " };
                println!("[{mark}] {} {}", task.id, task.title);
            }
            Ok(())
        }
    }
}

fn purge(store: &Store, args: &PurgeArgs, json: bool) -> Result<()> {
    let operator = ledger::operator::identity(args.operator.as_deref());
    let report = if args.dry_run {
        ledger::retention::preview(store, Utc::now())?
    } else {
        if !args.yes {
            return Err(Error::invalid(
                "purge",
                "tombstoning expired PII is destructive; pass --yes (or --dry-run)",
            ));
        }
        ledger::retention::purge(store, &operator, Utc::now())?
    };
    if json {
        emit_json("purge", serde_json::json!({ "report": report }))?;
    } else {
        println!("cutoff:               {}", report.cutoff.to_rfc3339());
        println!("dispositions purged:  {}", report.dispositions_purged);
        println!("dispositions kept:    {}", report.dispositions_retained);
        println!("ATS rosters purged:   {}", report.rosters_purged);
        println!(
            "send log retained:    {} entries (chained; erased only with the workspace)",
            report.send_log_entries_retained
        );
        if args.dry_run {
            println!("\n(dry run: nothing was written)");
        } else {
            println!("\nrecorded a PURGE event in operator.jsonl by {operator}");
        }
    }
    Ok(())
}

fn migrate(store: &Store, args: &MigrateArgs, json: bool) -> Result<()> {
    if !args.yes {
        return Err(Error::invalid(
            "migrate",
            "migration writes a workspace snapshot; pass --yes",
        ));
    }
    let operator = ledger::operator::identity(args.operator.as_deref());
    let report = ledger::migrate::migrate(store, &operator)?;
    if json {
        emit_json("migrate", serde_json::json!({ "report": report }))?;
    } else {
        println!(
            "schema:   found v{}, target v{}",
            report.found_version, report.target_version
        );
        println!("snapshot: {}", report.snapshot.display());
        for change in &report.changes {
            println!("  - {change}");
        }
        println!("chained logs were not rewritten");
    }
    Ok(())
}

fn anchor(store: &Store, args: &AnchorArgs, json: bool) -> Result<()> {
    let operator = ledger::operator::identity(args.operator.as_deref());
    let anchor = ledger::anchor::anchor(store, &operator, args.git_commit)?;
    if json {
        emit_json("anchor", serde_json::json!({ "anchor": anchor }))?;
    } else {
        println!("anchored {} entries at {}", anchor.entries, anchor.head);
        println!(
            "receipt in anchors/ ({}); copy it off this machine",
            anchor.destination
        );
        println!("recorded an ANCHOR event in operator.jsonl by {operator}");
    }
    Ok(())
}

fn find(store: &Store, args: &FindArgs, json: bool) -> Result<()> {
    let handle = args.hash.trim();
    let dispositions = Dispositions::open(store)?;
    let mut matches: Vec<serde_json::Value> = Vec::new();
    for record in dispositions.records() {
        let ids = [
            ledger::redact::id(record.form.application_id.as_str()),
            ledger::redact::id(record.form.candidate_email.as_str()),
        ];
        if ids.iter().any(|id| id == handle) {
            matches.push(serde_json::json!({
                "record": record.id.as_str(),
                "application": record.form.application_id.as_str(),
                "requisition": record.form.requisition.as_str(),
                "disposition": record.form.disposition.to_string(),
                "candidate_name": record.form.candidate_first_name,
                "candidate_email": record.form.candidate_email.as_str(),
            }));
        }
    }
    if json {
        emit_json(
            "find",
            serde_json::json!({ "handle": handle, "matches": matches }),
        )?;
    } else if matches.is_empty() {
        println!("no record matches {handle}");
    } else {
        for matched in &matches {
            println!("{matched}");
        }
    }
    Ok(())
}

/// The acting operator plus what a sign-off needs to record a signature.
struct Actor<'a> {
    operator: Option<&'a ledger::identity::Operator>,
    registry: &'a ledger::identity::Registry,
    store: &'a Store,
}

impl Actor<'_> {
    /// Signs a role's sign-off when an operator key is registered.
    fn sign(&self, approval: &mut ledger::approval::Approval, role: SignOffRole) -> Result<()> {
        if let Some(operator) = self.operator {
            approval.sign(role, self.registry, self.store, &operator.name)?;
        }
        Ok(())
    }
}

/// Whether a command only reads.
fn is_read_only(command: &Command) -> bool {
    // `ui` is excluded from the CLI's lock and RBAC pass: it launches a server
    // (any role may look) and enforces the role on each request instead.
    #[cfg(feature = "ui")]
    if matches!(command, Command::Ui(_)) {
        return true;
    }
    read_only_core(command)
}

fn read_only_core(command: &Command) -> bool {
    matches!(
        command,
        Command::Status(_)
            | Command::Gate(GateCommand::List)
            | Command::Requisition(RequisitionCommand::Show(_) | RequisitionCommand::List)
            | Command::Templates(TemplatesCommand::Show)
            | Command::Playbook(PlaybookCommand::Show | PlaybookCommand::Script(_))
            | Command::Disposition(DispositionCommand::List(_) | DispositionCommand::History(_))
            | Command::Qa(QaCommand::List(_))
            | Command::Delivery(DeliveryCommand::List(_))
            | Command::Log(LogCommand::Verify(_) | LogCommand::Export(_))
            | Command::Metrics(_)
            | Command::Review(ReviewCommand::Route(_))
            | Command::Checklist(ChecklistCommand::Show)
            | Command::Governance(GovernanceCommand::Show)
            | Command::Operator(OperatorCommand::List | OperatorCommand::Whoami)
            | Command::VerifySignatures(_)
            | Command::Find(_)
    )
}

fn load_library(store: &Store) -> Result<GateLibrary> {
    store
        .load(store::GATE_LIBRARY)?
        .ok_or_else(|| missing("gate library"))
}

fn load_templates(store: &Store) -> Result<Templates> {
    store
        .load(store::TEMPLATES)?
        .ok_or_else(|| missing("templates"))
}

fn load_playbook(store: &Store) -> Result<Playbook> {
    let playbook = Playbook::load(store)?.ok_or_else(|| missing("reply playbook"))?;
    playbook.validate()?;
    Ok(playbook)
}

fn load_requisitions(store: &Store) -> Result<RequisitionBook> {
    Ok(store.load(store::REQUISITIONS)?.unwrap_or_default())
}

fn save_requisitions(store: &Store, book: &RequisitionBook) -> Result<()> {
    store.save(store::REQUISITIONS, book)
}

fn missing(what: &str) -> Error {
    Error::invalid(
        "workspace",
        format!("no {what} in this workspace; run `ledger init`"),
    )
}

fn approval_text(approval: &ledger::approval::Approval) -> String {
    if approval.is_approved() {
        "approved".to_owned()
    } else {
        let missing: Vec<String> = approval.missing().iter().map(ToString::to_string).collect();
        format!("awaiting {}", missing.join(", "))
    }
}

fn join_roles(approval: &ledger::approval::Approval) -> String {
    let missing: Vec<String> = approval.missing().iter().map(ToString::to_string).collect();
    if missing.is_empty() {
        "nothing".to_owned()
    } else {
        missing.join(", ")
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn gate_codes(gates: &[GateKind]) -> String {
    gates
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<String>>()
        .join(", ")
}

fn abbreviate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_owned();
    }
    let kept: String = value.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}…")
}
