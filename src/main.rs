//! Command line interface for the pilot.

use std::path::PathBuf;

use chrono::Utc;
use clap::{Args, Parser, Subcommand};

use ledger::approval::SignOffRole;
use ledger::ats::Roster;
use ledger::checklist::{Checklist, TASKS};
use ledger::disposition::{ClosureReason, DispositionCategory, DispositionRecord, Dispositions};
use ledger::error::{Error, Result};
use ledger::gate::{GateAnswer, GateKind, GateLibrary};
use ledger::id::{ApplicationId, CandidateEmail, Mailbox, RecruiterId, RequisitionId};
use ledger::log::{BlockedLog, SendLog};
use ledger::metrics::{Metrics, Replies, Reply, ReplyTheme};
use ledger::pipeline::{Dispatch, DispatchReport, Mode, dispatch};
use ledger::requisition::{Requisition, RequisitionBook};
use ledger::review::{CriteriaAssessment, CriteriaRecord, GoNoGo, Policy, Reviews};
use ledger::store::{self, Store};
use ledger::template::Templates;

/// Timestamp format used in all human-readable output.
const STAMP: &str = "%Y-%m-%d %H:%M UTC";

#[derive(Debug, Parser)]
#[command(
    name = "ledger",
    version,
    about = "Operational tooling for the v1 manual candidate feedback pilot",
    long_about = "Implements the pilot in plan.md: a locked disposition form, pre-approved \
                  gate copy, three master templates, an append-only hash-chained send log, the \
                  gate-blocking protocol, the three v2 decision metrics, and the go/no-go review."
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
    /// Seed the workspace with the gate library, templates, and policy.
    Init,
    /// Show the plan task sheet and the evidence the workspace holds.
    Status(StatusArgs),
    /// Inspect the Tier 0 gate copy library.
    #[command(subcommand)]
    Gate(GateCommand),
    /// Create, approve, and inspect requisitions.
    #[command(subcommand)]
    Requisition(RequisitionCommand),
    /// Inspect and approve the three master email templates.
    #[command(subcommand)]
    Templates(TemplateCommand),
    /// Import the ATS export.
    #[command(subcommand)]
    Ats(AtsCommand),
    /// Capture and list dispositions.
    #[command(subcommand)]
    Disposition(DispositionCommand),
    /// Render and dispatch feedback under the gate-blocking protocol.
    Dispatch(DispatchArgs),
    /// Verify the send log.
    #[command(subcommand)]
    Log(LogCommand),
    /// Record candidate replies.
    #[command(subcommand)]
    Reply(ReplyCommand),
    /// Compute the pilot metrics.
    Metrics(MetricsArgs),
    /// Record criteria feasibility and compile the go/no-go review.
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
enum GateCommand {
    /// List the five gates and their copy.
    List,
    /// Sign off the copy library in one pass.
    Approve(GateApprove),
}

#[derive(Debug, Args)]
struct GateApprove {
    #[arg(long)]
    actor: String,
    #[arg(long)]
    role: String,
}

#[derive(Debug, Subcommand)]
enum RequisitionCommand {
    /// Create a requisition.
    Create(RequisitionCreate),
    /// Draft the static comparative paragraph.
    Comparative(RequisitionComparative),
    /// Sign off the requisition copy, or the comparative paragraph.
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
    /// Monitored reply-to mailbox, for example careers-feedback@example.com.au.
    #[arg(long)]
    reply_to: String,
    /// Knockout gate this requisition screens on; repeat for each gate.
    #[arg(long = "gate", required = true)]
    gates: Vec<String>,
}

#[derive(Debug, Args)]
struct RequisitionComparative {
    #[arg(long)]
    id: String,
    /// Paragraph text. Mutually exclusive with --file.
    #[arg(long, conflicts_with = "file")]
    text: Option<String>,
    /// File holding the paragraph.
    #[arg(long)]
    file: Option<PathBuf>,
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
enum TemplateCommand {
    /// Show the three templates and their sign-off state.
    Show,
    /// Sign off all three templates in one pass.
    Approve(TemplateApprove),
}

#[derive(Debug, Args)]
struct TemplateApprove {
    #[arg(long)]
    actor: String,
    #[arg(long)]
    role: String,
}

#[derive(Debug, Subcommand)]
enum AtsCommand {
    /// Import an ATS CSV export for a requisition.
    Import(AtsImport),
}

#[derive(Debug, Args)]
struct AtsImport {
    #[arg(long)]
    requisition: String,
    /// CSV export with application_id, candidate_name, email, and optional
    /// knockout_<gate> columns.
    #[arg(long)]
    file: PathBuf,
}

#[derive(Debug, Subcommand)]
enum DispositionCommand {
    /// Capture one disposition at the moment of rejection.
    Record(DispositionRecordArgs),
    /// List captured dispositions for a requisition.
    List(DispositionList),
}

#[derive(Debug, Args)]
struct DispositionRecordArgs {
    #[arg(long)]
    requisition: String,
    #[arg(long)]
    application: String,
    #[arg(long)]
    email: String,
    #[arg(long)]
    name: String,
    #[arg(long)]
    recruiter: String,
    /// failed-gate, met-the-bar-stronger-field, or administrative-closure.
    #[arg(long)]
    category: String,
    /// Gate code, required for failed-gate.
    #[arg(long)]
    gate: Option<String>,
    /// Gate answer, required for failed-gate.
    #[arg(long)]
    answer: Option<String>,
    /// Closure reason, required for administrative-closure.
    #[arg(long)]
    reason: Option<String>,
}

#[derive(Debug, Args)]
struct DispositionList {
    #[arg(long)]
    requisition: String,
}

#[derive(Debug, Args)]
struct DispatchArgs {
    #[arg(long)]
    requisition: String,
    /// Render the emails and report blocks without writing either log.
    #[arg(long)]
    preview: bool,
}

#[derive(Debug, Subcommand)]
enum LogCommand {
    /// Verify the hash chain and print its head.
    Verify,
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
    /// gate-question, status-request, process-concern, or other.
    #[arg(long)]
    theme: String,
    /// How many replies of this theme arrived.
    #[arg(long, default_value_t = 1)]
    count: u32,
}

#[derive(Debug, Args)]
struct MetricsArgs {
    #[arg(long)]
    requisition: String,
    /// Print the metrics as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Subcommand)]
enum ReviewCommand {
    /// Record whether HM and Legal agreed a structured criteria list.
    Criteria(ReviewCriteriaArgs),
    /// Compile the go/no-go review.
    Report(ReviewReportArgs),
}

#[derive(Debug, Args)]
struct ReviewCriteriaArgs {
    #[arg(long)]
    requisition: String,
    /// agreed, disputed, or not-assessed.
    #[arg(long)]
    status: String,
    #[arg(long)]
    actor: String,
    #[arg(long)]
    note: Option<String>,
}

#[derive(Debug, Args)]
struct ReviewReportArgs {
    #[arg(long)]
    requisition: String,
    /// Print the review as JSON.
    #[arg(long)]
    json: bool,
    /// Exit non-zero unless the recommendation is to proceed to v2 engineering.
    #[arg(long)]
    strict: bool,
}

#[derive(Debug, Subcommand)]
enum ChecklistCommand {
    /// Mark a plan task done.
    Done(ChecklistDoneArgs),
    /// Print the plan task sheet.
    Show,
}

#[derive(Debug, Args)]
struct ChecklistDoneArgs {
    /// Task id from plan.md, for example 2.1.
    task: String,
    #[arg(long)]
    actor: String,
    /// Optional evidence reference, such as a ticket id.
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
        Command::Gate(GateCommand::List) => gate_list(store),
        Command::Gate(GateCommand::Approve(args)) => gate_approve(store, &args),
        Command::Requisition(command) => requisition(store, command),
        Command::Templates(command) => templates(store, command),
        Command::Ats(AtsCommand::Import(args)) => ats_import(store, &args),
        Command::Disposition(command) => disposition(store, command),
        Command::Dispatch(args) => dispatch_command(store, &args),
        Command::Log(LogCommand::Verify) => log_verify(store),
        Command::Reply(ReplyCommand::Record(args)) => reply_record(store, &args),
        Command::Metrics(args) => metrics_command(store, &args),
        Command::Review(command) => review(store, command),
        Command::Checklist(command) => checklist(store, command),
    }
}

fn init(store: &Store) -> Result<()> {
    if store.load::<GateLibrary>(store::GATE_LIBRARY)?.is_none() {
        GateLibrary::seed().save(store)?;
        println!("seeded {} Tier 0 gate copy blocks", GateKind::ALL.len());
    }
    if store.load::<Templates>(store::TEMPLATES)?.is_none() {
        Templates::seed().save(store)?;
        println!("seeded 3 master email templates");
    }
    if store.load::<Policy>(store::POLICY)?.is_none() {
        Policy::default().save(store)?;
        println!("seeded go/no-go policy");
    }
    println!("\nworkspace: {}", store.root().display());
    println!("\nnext steps:");
    println!(
        "  1. get the gate copy and templates signed off: ledger templates approve --actor ... --role legal"
    );
    println!(
        "  2. create the requisition:                      ledger requisition create --id REQ-1 ..."
    );
    println!(
        "  3. draft the comparative paragraph:             ledger requisition comparative --id REQ-1 --file paragraph.txt"
    );
    println!(
        "  4. import the ATS export:                       ledger ats import --requisition REQ-1 --file export.csv"
    );
    println!(
        "  5. capture dispositions, then dispatch:         ledger dispatch --requisition REQ-1 --preview"
    );
    Ok(())
}

fn status(store: &Store, args: &StatusArgs) -> Result<()> {
    let checklist = Checklist::load(store)?;
    let (done, total) = checklist.progress();
    println!("plan.md tasks: {done}/{total} signed off\n");
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
    let dispositions = Dispositions::open(store)?;
    let send_log = SendLog::open(store)?;
    let send_entries = send_log.entries()?;
    let blocked_log = BlockedLog::open(store)?;
    let blocked = blocked_log.attempts();
    let replies = Replies::open(store).all()?;

    let keep = |id: &RequisitionId| filtered.as_ref().is_none_or(|want| want == id);
    let dispositions = dispositions
        .records()
        .iter()
        .filter(|record| keep(&record.requisition))
        .count();
    let sends = send_entries
        .iter()
        .filter(|entry| keep(&entry.requisition))
        .count();
    let blocks = blocked
        .iter()
        .filter(|attempt| keep(&attempt.requisition))
        .count();
    let reply_count = replies
        .iter()
        .filter(|reply| keep(&reply.requisition))
        .count();

    println!("\nevidence");
    println!(
        "  gate copy approved:      {}",
        yes_no(library.is_approved())
    );
    println!(
        "  templates approved:      {}",
        yes_no(templates.is_approved())
    );
    println!("  dispositions captured:   {dispositions}");
    println!("  emails sent:             {sends}");
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
    Ok(())
}

fn gate_list(store: &Store) -> Result<()> {
    let library = load_library(store)?;
    println!(
        "gate library: {} of {} copy blocks, approval: {}",
        library.copies.len(),
        GateKind::ALL.len(),
        approval_text(&library.approval)
    );
    for gate in GateKind::ALL {
        println!("\n{} ({})", gate.label(), gate);
        match library.copy_for(gate) {
            None => println!("  no copy block"),
            Some(copy) => {
                println!("  requirement:    {}", copy.requirement);
                println!("  satisfies:      {}", copy.what_satisfies);
                println!("  flexibility:    {}", copy.flexibility.sentence());
            }
        }
    }
    Ok(())
}

fn gate_approve(store: &Store, args: &GateApprove) -> Result<()> {
    let role = args.role.parse::<SignOffRole>()?;
    let mut library = load_library(store)?;
    library.approve(args.actor.as_str(), role, Utc::now())?;
    library.save(store)?;
    println!(
        "recorded {role} sign-off on the gate copy library ({})",
        approval_text(&library.approval)
    );
    Ok(())
}

fn requisition(store: &Store, command: RequisitionCommand) -> Result<()> {
    match command {
        RequisitionCommand::Create(args) => requisition_create(store, &args),
        RequisitionCommand::Comparative(args) => requisition_comparative(store, &args),
        RequisitionCommand::Approve(args) => requisition_approve(store, &args),
        RequisitionCommand::Show(args) => requisition_show(store, &args),
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
    let gates: Result<Vec<GateKind>> = args
        .gates
        .iter()
        .map(|gate| gate.parse::<GateKind>())
        .collect();
    let requisition = Requisition::new(
        id,
        args.employer.as_str(),
        args.title.as_str(),
        reply_to,
        gates?,
        Utc::now(),
    )?;
    let mut book = load_requisitions(store)?;
    book.insert(requisition)?;
    save_requisitions(store, &book)?;
    println!("created requisition {}", args.id);
    println!("knockout gates: {}", args.gates.join(", "));
    Ok(())
}

fn requisition_comparative(store: &Store, args: &RequisitionComparative) -> Result<()> {
    let text = match (&args.text, &args.file) {
        (Some(text), _) => text.clone(),
        (None, Some(file)) => std::fs::read_to_string(file).map_err(|source| Error::Io {
            path: file.clone(),
            source,
        })?,
        (None, None) => {
            return Err(Error::invalid(
                "comparative paragraph",
                "pass --text or --file",
            ));
        }
    };
    let id = RequisitionId::try_from(args.id.as_str())?;
    let mut book = load_requisitions(store)?;
    let requisition = book
        .get_mut(&id)
        .ok_or_else(|| Error::UnknownRequisition(id.clone()))?;
    requisition.set_comparative(text)?;
    let paragraph = requisition
        .comparative
        .as_ref()
        .map(|paragraph| paragraph.text.clone())
        .unwrap_or_default();
    save_requisitions(store, &book)?;
    println!("drafted the comparative paragraph for {id}:");
    println!("\n{paragraph}\n");
    println!("this copy needs its own HM and Legal sign-off before dispatch");
    Ok(())
}

fn requisition_approve(store: &Store, args: &RequisitionApprove) -> Result<()> {
    let id = RequisitionId::try_from(args.id.as_str())?;
    let role = args.role.parse::<SignOffRole>()?;
    let mut book = load_requisitions(store)?;
    let requisition = book
        .get_mut(&id)
        .ok_or_else(|| Error::UnknownRequisition(id.clone()))?;
    if args.comparative {
        requisition.approve_comparative(args.actor.as_str(), role, Utc::now())?;
    } else {
        requisition.approve(args.actor.as_str(), role, Utc::now())?;
    }
    let missing = requisition.approval.missing();
    save_requisitions(store, &book)?;
    if missing.is_empty() && !args.comparative {
        println!("requisition {id} copy is signed off");
    } else if args.comparative {
        println!("comparative paragraph for {id} signed by {role}");
    } else {
        let roles: Vec<String> = missing.iter().map(ToString::to_string).collect();
        println!(
            "recorded {role} sign-off on {id}; still needed: {}",
            roles.join(", ")
        );
    }
    Ok(())
}

fn requisition_show(store: &Store, args: &RequisitionShow) -> Result<()> {
    let id = RequisitionId::try_from(args.id.as_str())?;
    let book = load_requisitions(store)?;
    let requisition = book.require(&id)?;
    let library = load_library(store)?;

    println!("{}  {}", requisition.id, requisition.title);
    println!("employer:   {}", requisition.employer);
    println!("reply-to:   {}", requisition.reply_to);
    println!("gates:      {}", gate_codes(&requisition.gates));
    println!("copy signed: {}", approval_text(&requisition.approval));
    match &requisition.comparative {
        None => println!("comparative: not drafted"),
        Some(paragraph) => {
            println!("comparative: {}", approval_text(&paragraph.approval));
            println!("\n{}\n", paragraph.text);
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

fn templates(store: &Store, command: TemplateCommand) -> Result<()> {
    match command {
        TemplateCommand::Show => {
            let templates = load_templates(store)?;
            println!("templates approved: {}", yes_no(templates.is_approved()));
            for kind in ledger::id::TemplateKind::ALL {
                let template = templates.require(kind)?;
                println!("\n=== {kind} ({}) ===", approval_text(&template.approval));
                println!("Subject: {}", template.subject);
                println!();
                println!("{}", template.body);
            }
            Ok(())
        }
        TemplateCommand::Approve(args) => {
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
        "imported {} applicants for {id} ({} with knockout answers pre-filled)",
        roster.applicants.len(),
        with_knockouts
    );
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
                if record.requisition != id {
                    continue;
                }
                found += 1;
                println!(
                    "{}  {}  {:<40} {}  by {}",
                    record.captured_at.format(STAMP),
                    record.application_id,
                    record.category.to_string(),
                    abbreviate(&record.candidate_name, 24),
                    record.recruiter
                );
            }
            if found == 0 {
                println!("no dispositions captured for {id}");
            }
            Ok(())
        }
    }
}

fn disposition_record(store: &Store, args: &DispositionRecordArgs) -> Result<()> {
    let requisition_id = RequisitionId::try_from(args.requisition.as_str())?;
    let book = load_requisitions(store)?;
    let requisition = book.require(&requisition_id)?;

    let category = match args.category.trim().to_lowercase().as_str() {
        "failed-gate" | "gate" => {
            let gate = args
                .gate
                .as_deref()
                .ok_or_else(|| Error::invalid("disposition", "failed-gate needs --gate"))?
                .parse::<GateKind>()?;
            let answer = args
                .answer
                .as_deref()
                .ok_or_else(|| Error::invalid("disposition", "failed-gate needs --answer"))?
                .parse::<GateAnswer>()?;
            DispositionCategory::FailedGate { gate, answer }
        }
        "met-the-bar-stronger-field" | "comparative" => DispositionCategory::MetTheBarStrongerField,
        "administrative-closure" | "administrative" => {
            let reason = args
                .reason
                .as_deref()
                .ok_or_else(|| {
                    Error::invalid("disposition", "administrative-closure needs --reason")
                })?
                .parse::<ClosureReason>()?;
            DispositionCategory::AdministrativeClosure { reason }
        }
        other => {
            return Err(Error::invalid(
                "disposition category",
                format!(
                    "expected failed-gate, met-the-bar-stronger-field or administrative-closure; got {other:?}"
                ),
            ));
        }
    };

    let record = DispositionRecord::try_new(
        requisition,
        ApplicationId::try_from(args.application.as_str())?,
        CandidateEmail::try_from(args.email.as_str())?,
        args.name.as_str(),
        RecruiterId::try_from(args.recruiter.as_str())?,
        Utc::now(),
        category,
    )?;

    let mut dispositions = Dispositions::open(store)?;
    let stored = dispositions.record(record)?.clone();
    println!(
        "captured {} for {} at {}",
        stored.category,
        stored.application_id,
        stored.captured_at.format(STAMP)
    );
    Ok(())
}

fn dispatch_command(store: &Store, args: &DispatchArgs) -> Result<()> {
    let id = RequisitionId::try_from(args.requisition.as_str())?;
    let book = load_requisitions(store)?;
    let requisition = book.require(&id)?;
    let library = load_library(store)?;
    let templates = load_templates(store)?;
    let roster = Roster::load(store, &id)?.ok_or_else(|| {
        Error::invalid(
            "roster",
            format!("no ATS export imported for {id}; run `ledger ats import`"),
        )
    })?;
    let dispositions = Dispositions::open(store)?;
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
            roster: &roster,
            dispositions: &dispositions,
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
            println!("preview: {} email(s) would send\n", report.previews.len());
            for preview in &report.previews {
                println!(
                    "--- {} -> {} ({}) ---",
                    preview.application_id, preview.candidate_email, preview.category
                );
                println!("{}\n", preview.rendered);
            }
        }
        Mode::Execute => {
            println!("sent {}", report.sent_count());
        }
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
        println!("\n(preview writes nothing; blocked attempts are logged only on a real run)");
    }
}

fn log_verify(store: &Store) -> Result<()> {
    let log = SendLog::open(store)?;
    let verification = log.verification()?;
    println!("send log verified: {} entries", verification.entries);
    println!("head: {}", verification.head);
    Ok(())
}

fn reply_record(store: &Store, args: &ReplyRecordArgs) -> Result<()> {
    if args.count == 0 {
        return Err(Error::invalid("reply count", "must be at least 1"));
    }
    let id = RequisitionId::try_from(args.requisition.as_str())?;
    let theme = args.theme.parse::<ReplyTheme>()?;
    let mut replies = Replies::open(store);
    replies.record(
        Reply {
            requisition: id,
            received_at: Utc::now(),
            theme,
        },
        args.count,
    )?;
    println!("recorded {} reply/replies with theme {theme}", args.count);
    Ok(())
}

fn metrics_command(store: &Store, args: &MetricsArgs) -> Result<()> {
    let id = RequisitionId::try_from(args.requisition.as_str())?;
    let metrics = compute_metrics(store, &id)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&metrics)?);
    } else {
        println!("{metrics}");
        if !metrics.reply_themes.is_empty() {
            println!("\nreply themes");
            for (theme, count) in &metrics.reply_themes {
                println!("  {theme:<16} {count}");
            }
        }
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
    let blocked_log = BlockedLog::open(store)?;
    let blocked: Vec<_> = blocked_log
        .attempts()
        .iter()
        .filter(|attempt| &attempt.requisition == id)
        .cloned()
        .collect();
    Ok(Metrics::compute(&sends, &replies, &blocked))
}

fn review(store: &Store, command: ReviewCommand) -> Result<()> {
    match command {
        ReviewCommand::Criteria(args) => {
            let id = RequisitionId::try_from(args.requisition.as_str())?;
            let assessment = args.status.parse::<CriteriaAssessment>()?;
            let mut reviews = Reviews::load(store)?;
            reviews.assess(CriteriaRecord {
                requisition: id.clone(),
                assessment,
                actor: args.actor.clone(),
                at: Utc::now(),
                note: args.note.clone(),
            })?;
            reviews.save(store)?;
            println!("recorded criteria feasibility for {id}: {assessment}");
            Ok(())
        }
        ReviewCommand::Report(args) => {
            let id = RequisitionId::try_from(args.requisition.as_str())?;
            let policy = Policy::load(store)?;
            let reviews = Reviews::load(store)?;
            let metrics = compute_metrics(store, &id)?;
            let assessment = reviews.assessment_for(&id);
            let review = GoNoGo::decide(id, metrics, assessment, policy);
            if args.json {
                println!("{}", serde_json::to_string_pretty(&review)?);
            } else {
                println!("{review}");
            }
            if args.strict
                && review.recommendation != ledger::review::Recommendation::ProceedToV2Engineering
            {
                return Err(Error::invalid(
                    "go/no-go",
                    format!("recommendation is {}", review.recommendation),
                ));
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
                "marked plan task {} done (signed by {})",
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
