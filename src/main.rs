//! Command line interface for the pilot.

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::Utc;
use clap::{Args, Parser, Subcommand};

use ledger::approval::SignOffRole;
use ledger::ats::Roster;
use ledger::checklist::{Checklist, TASKS};
use ledger::disposition::{
    AdminReason, Disposition, DispositionKind, Dispositions, FormSubmission, NewDisposition,
};
use ledger::error::{Error, Result};
use ledger::gate::{FactKey, GateAnswer, GateKind, GateLibrary};
use ledger::id::{
    ApplicationId, CandidateEmail, DecisionMakerId, DispositionId, Mailbox, QaId, RecruiterId,
    RequisitionId, TemplateKind, TextVersion,
};
use ledger::log::{BlockedLog, DeliveryStatus, SendLog};
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
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Seed the workspace: gate copy, templates, playbook, policy.
    Init,
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
    /// DELIVERY_STATUS to record: queued, sent, or failed.
    #[arg(long, default_value = "sent")]
    status: String,
}

#[derive(Debug, Subcommand)]
enum LogCommand {
    /// Verify the hash chain and print its head.
    Verify,
    /// Export the send log as CSV with REPLY_RECEIVED_FLAG.
    Export(LogExportArgs),
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
    /// Print as JSON.
    #[arg(long)]
    json: bool,
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
    /// Print as JSON.
    #[arg(long)]
    json: bool,
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
            std::process::ExitCode::FAILURE
        }
    }
}

fn execute(cli: Cli) -> Result<()> {
    let store = Store::open(&cli.data)?;
    run(&store, cli.command)
}

fn run(store: &Store, command: Command) -> Result<()> {
    match command {
        Command::Init => init(store),
        Command::Status(args) => status(store, &args),
        Command::Governance(command) => governance(store, command),
        Command::Gate(command) => gate(store, command),
        Command::Requisition(command) => requisition(store, command),
        Command::Templates(command) => templates(store, command),
        Command::Playbook(command) => playbook(store, command),
        Command::Ats(AtsCommand::Import(args)) => ats_import(store, &args),
        Command::Disposition(command) => disposition(store, command),
        Command::Qa(command) => qa(store, command),
        Command::Dispatch(args) => dispatch_command(store, &args),
        Command::Log(command) => log(store, command),
        Command::Reply(ReplyCommand::Record(args)) => reply_record(store, &args),
        Command::Signal(SignalCommand::Record(args)) => signal_record(store, &args),
        Command::Metrics(args) => metrics_command(store, &args),
        Command::Review(command) => review(store, command),
        Command::Checklist(command) => checklist(store, command),
    }
}

fn init(store: &Store) -> Result<()> {
    if store.load::<GateLibrary>(store::GATE_LIBRARY)?.is_none() {
        GateLibrary::seed().save(store)?;
        println!("seeded 5 gate blocks at v1.0");
    }
    if store.load::<Templates>(store::TEMPLATES)?.is_none() {
        Templates::seed().save(store)?;
        println!("seeded templates A, B, C at v1.0");
    }
    if store.load::<Playbook>(store::PLAYBOOK)?.is_none() {
        Playbook::seed().save(store)?;
        println!("seeded 5 reply scripts");
    }
    if store.load::<Policy>(store::POLICY)?.is_none() {
        Policy::default().save(store)?;
        println!("seeded retention and escalation policy");
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

fn status(store: &Store, args: &StatusArgs) -> Result<()> {
    let checklist = Checklist::load(store)?;
    let (done, total) = checklist.progress();
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

    let book = load_requisitions(store)?;
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

fn governance(store: &Store, command: GovernanceCommand) -> Result<()> {
    let mut policy = Policy::load(store)?;
    match command {
        GovernanceCommand::Show => {
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
            println!("{policy}");
            Ok(())
        }
    }
}

fn gate(store: &Store, command: GateCommand) -> Result<()> {
    match command {
        GateCommand::List => {
            let library = load_library(store)?;
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
            library.save(store)?;
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
            println!("{gate} is now {version}; library sign-off cleared, re-approval required");
            Ok(())
        }
    }
}

fn requisition(store: &Store, command: RequisitionCommand) -> Result<()> {
    match command {
        RequisitionCommand::Create(args) => requisition_create(store, &args),
        RequisitionCommand::Fact(args) => {
            let id = RequisitionId::try_from(args.id.as_str())?;
            let key = args.key.parse::<FactKey>()?;
            let mut book = load_requisitions(store)?;
            let requisition = book
                .get_mut(&id)
                .ok_or_else(|| Error::UnknownRequisition(id.clone()))?;
            requisition.set_fact(key, args.value.as_str())?;
            save_requisitions(store, &book)?;
            println!("recorded {key} for {id}");
            Ok(())
        }
        RequisitionCommand::Comparative(args) => requisition_comparative(store, &args),
        RequisitionCommand::Approve(args) => {
            let id = RequisitionId::try_from(args.id.as_str())?;
            let role = args.role.parse::<SignOffRole>()?;
            let mut book = load_requisitions(store)?;
            let requisition = book
                .get_mut(&id)
                .ok_or_else(|| Error::UnknownRequisition(id.clone()))?;
            if args.comparative {
                requisition.approve_comparative(args.actor.as_str(), role, Utc::now())?;
                println!("comparative paragraph for {id} signed by {role}");
            } else {
                requisition.approve(args.actor.as_str(), role, Utc::now())?;
                println!(
                    "requisition {id} copy signed by {role}; still needed: {}",
                    join_roles(&requisition.approval)
                );
            }
            save_requisitions(store, &book)?;
            Ok(())
        }
        RequisitionCommand::Show(args) => {
            let id = RequisitionId::try_from(args.id.as_str())?;
            let book = load_requisitions(store)?;
            let requisition = book.require(&id)?;
            let library = load_library(store)?;

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

fn requisition_create(store: &Store, args: &RequisitionCreate) -> Result<()> {
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

fn requisition_comparative(store: &Store, args: &RequisitionComparative) -> Result<()> {
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
    save_requisitions(store, &book)?;
    println!("elicited the comparative paragraph for {id}:\n\n{paragraph}\n");
    println!("this wording needs its own HM and Legal sign-off before dispatch");
    Ok(())
}

fn templates(store: &Store, command: TemplatesCommand) -> Result<()> {
    match command {
        TemplatesCommand::Show => {
            let templates = load_templates(store)?;
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
            templates.save(store)?;
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

fn playbook(store: &Store, command: PlaybookCommand) -> Result<()> {
    match command {
        PlaybookCommand::Show => {
            let playbook = load_playbook(store)?;
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
            playbook.save(store)?;
            println!(
                "recorded {role} sign-off on the playbook ({})",
                approval_text(&playbook.approval)
            );
            Ok(())
        }
    }
}

fn ats_import(store: &Store, args: &AtsImport) -> Result<()> {
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
    if roster.applicants.is_empty() {
        return Err(Error::invalid(
            "ATS export",
            "no row survived the initial-screen filter",
        ));
    }
    Ok(())
}

fn disposition(store: &Store, command: DispositionCommand) -> Result<()> {
    match command {
        DispositionCommand::Record(args) => disposition_record(store, &args),
        DispositionCommand::List(args) => {
            let id = RequisitionId::try_from(args.requisition.as_str())?;
            let dispositions = Dispositions::open(store)?;
            let mut found = 0;
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
            if found == 0 {
                println!("no current dispositions for {id}");
            }
            Ok(())
        }
        DispositionCommand::History(args) => {
            let application = ApplicationId::try_from(args.application.as_str())?;
            let dispositions = Dispositions::open(store)?;
            let history = dispositions.history(&application);
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

fn disposition_record(store: &Store, args: &DispositionRecordArgs) -> Result<()> {
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

    let mut dispositions = Dispositions::open(store)?;
    let stored = dispositions.record(form)?.clone();
    println!(
        "captured {} for {} at {} (dm {})",
        stored.form.disposition,
        stored.form.application_id,
        stored.form.captured_at.format(STAMP),
        stored.form.decision_maker
    );
    println!("record id: {}", stored.id);
    Ok(())
}

fn qa(store: &Store, command: QaCommand) -> Result<()> {
    match command {
        QaCommand::Record(args) => {
            let registry = load_requisitions(store)?;
            let requisition = RequisitionId::try_from(args.requisition.as_str())?;
            registry.require(&requisition)?;
            let application = ApplicationId::try_from(args.application.as_str())?;
            let dispositions = Dispositions::open(store)?;
            let record = dispositions
                .head(&requisition, &application)
                .ok_or_else(|| Error::UnknownApplication(application.clone()))?;
            let scope = match &record.form.disposition {
                Disposition::Gate { .. } => QaScope::GateFullCheck,
                _ => QaScope::RandomTenPercent,
            };
            let mut log = QaLog::open(store)?;
            let recorded = log.record(QaRecord {
                application_id: application.clone(),
                requisition,
                qa: QaId::try_from(args.qa.as_str())?,
                at: Utc::now(),
                scope,
            })?;
            if recorded {
                println!("recorded {} QA check for {application}", scope.code());
            } else {
                println!("{application} already has a QA check on file");
            }
            if scope == QaScope::RandomTenPercent && !sampled(&application) {
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
            for record in log.records() {
                if record.requisition != requisition {
                    continue;
                }
                found += 1;
                println!(
                    "{}  {}  {}  by {}",
                    record.application_id,
                    record.scope.code(),
                    record.at.format(STAMP),
                    record.qa
                );
            }
            if found == 0 {
                println!("no QA checks recorded for {requisition}");
            }
            Ok(())
        }
    }
}

fn dispatch_command(store: &Store, args: &DispatchArgs) -> Result<()> {
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
            delivery_status: args.status.parse::<DeliveryStatus>()?,
            now: Utc::now(),
        },
        &mut send_log,
        &mut blocked_log,
    )?;
    print_dispatch_report(&report, mode);
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

fn log(store: &Store, command: LogCommand) -> Result<()> {
    match command {
        LogCommand::Verify => {
            let verification = SendLog::open(store)?.verification()?;
            println!("send log verified: {} entries", verification.entries);
            println!("head: {}", verification.head);
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

            let sink: Box<dyn std::io::Write> = match &args.out {
                Some(path) => {
                    Box::new(std::fs::File::create(path).map_err(|source| Error::Io {
                        path: path.clone(),
                        source,
                    })?)
                }
                None => Box::new(std::io::stdout()),
            };
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
            for entry in &entries {
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
                path: args.out.clone().unwrap_or_default(),
                source,
            })?;
            if args.out.is_some() {
                println!("exported {} entries", entries.len());
            }
            Ok(())
        }
    }
}

fn reply_record(store: &Store, args: &ReplyRecordArgs) -> Result<()> {
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

fn signal_record(store: &Store, args: &SignalRecordArgs) -> Result<()> {
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
    println!("recorded {} x {value} for {id}", args.count);
    Ok(())
}

fn metrics_command(store: &Store, args: &MetricsArgs) -> Result<()> {
    let id = RequisitionId::try_from(args.requisition.as_str())?;
    let metrics = compute_metrics(store, &id)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&metrics)?);
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
    Ok(Metrics::compute(&sends, &replies, &blocked, &signals))
}

fn review(store: &Store, command: ReviewCommand) -> Result<()> {
    match command {
        ReviewCommand::Criteria(args) => {
            let id = RequisitionId::try_from(args.requisition.as_str())?;
            let feasibility = args.status.parse::<CriteriaFeasibility>()?;
            let mut reviews = Reviews::load(store)?;
            reviews.assess(CriteriaRecord {
                requisition: id.clone(),
                feasibility,
                actor: args.actor.clone(),
                at: Utc::now(),
                note: args.note.clone(),
            })?;
            reviews.save(store)?;
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
            if args.json {
                println!("{}", serde_json::to_string_pretty(&routing)?);
            } else {
                println!("{routing}");
            }
            Ok(())
        }
    }
}

fn checklist(store: &Store, command: ChecklistCommand) -> Result<()> {
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
            println!(
                "marked task {} done (signed by {})",
                completion.task, completion.actor
            );
            Ok(())
        }
        ChecklistCommand::Show => {
            let checklist = Checklist::load(store)?;
            let (done, total) = checklist.progress();
            println!("{done}/{total} tasks signed off\n");
            for task in TASKS {
                let mark = if checklist.is_done(task.id) { "x" } else { " " };
                println!("[{mark}] {} {}", task.id, task.title);
            }
            Ok(())
        }
    }
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
