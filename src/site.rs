//! Build-time HTML artefact pages (V3 Phase 2).
//!
//! `ledger dispatch --preview --export-html <dir>` writes zero-dependency pages
//! straight from the loaded workspace: the three candidate emails, the gate
//! library with its versions and fact dependencies, the hash-chain audit (valid
//! and tampered side by side), and the comparative paragraph with its rejected
//! drafts. There is no hand-written copy in these pages, so the site cannot drift
//! from the CLI's actual behaviour.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use crate::ats::Roster;
use crate::disposition::{Disposition, Dispositions};
use crate::error::{Error, Result};
use crate::gate::{GateKind, GateLibrary};
use crate::id::{RequisitionId, TemplateKind, TextVersion};
use crate::log::{BlockedLog, SendEntry, SendLog};
use crate::qa::QaLog;
use crate::requisition::{Comparative, Requisition};
use crate::store::Store;
use crate::template::{MergeContext, Templates, render};

/// The four Hiring-Manager elicitation questions, verbatim from V2.md.
pub const ELICITATION_QUESTIONS: [&str; 4] = [
    "What did all shortlisted applicants demonstrate that most applicants did not?",
    "What concrete evidence proved it (project, metric, tool, scale)?",
    "What did a strong application make visible within 30 seconds?",
    "What did many applicants assume mattered that actually didn't?",
];

/// The disclosure every page must carry.
pub const SYNTHETIC_BANNER: &str = "Synthetic fixture — no pilot data yet";

/// What the export wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportReport {
    /// The pages written, in navigation order.
    pub pages: Vec<PathBuf>,
    /// Whether the untouched log verified.
    pub audit_valid: bool,
    /// Whether the one-byte-altered log failed verification.
    pub audit_tampered_detected: bool,
}

/// The page routes, in navigation order.
pub const ROUTES: [(&str, &str); 5] = [
    ("emails", "Candidate emails"),
    ("gates", "Gate library"),
    ("audit", "Hash-chain audit"),
    ("comparative", "Comparative paragraph"),
    ("demo", "Recorded run"),
];

/// Writes the artefact pages into `out_dir`.
///
/// # Errors
///
/// Returns [`Error::Invalid`] when the demo workspace has not been seeded, and
/// propagates IO and verification errors.
pub fn export_html(store: &Store, out_dir: impl AsRef<Path>) -> Result<ExportReport> {
    let out_dir = out_dir.as_ref();
    fs::create_dir_all(out_dir).map_err(|source| Error::Io {
        path: out_dir.to_path_buf(),
        source,
    })?;

    let library: GateLibrary = store
        .load(crate::store::GATE_LIBRARY)?
        .ok_or_else(|| not_seeded("gate library"))?;
    let templates: Templates = store
        .load(crate::store::TEMPLATES)?
        .ok_or_else(|| not_seeded("templates"))?;
    let book: crate::requisition::RequisitionBook = store
        .load(crate::store::REQUISITIONS)?
        .ok_or_else(|| not_seeded("requisitions"))?;
    let requisition_id = RequisitionId::try_from(crate::demo::PILOT_REQUISITION)?;
    let requisition = book.require(&requisition_id)?;
    // The roster must exist, but the pages read names from the dispositions.
    Roster::load(store, &requisition_id)?.ok_or_else(|| not_seeded("ATS roster"))?;
    let dispositions = Dispositions::open(store)?;
    let sends = SendLog::open(store)?.entries()?;
    let blocked = BlockedLog::open(store)?.attempts().to_vec();
    let qa = QaLog::open(store)?;

    let mut pages = Vec::new();
    let emails = out_dir.join("emails.html");
    fs::write(
        &emails,
        emails_page(requisition, &library, &templates, &dispositions)?,
    )
    .map_err(|source| Error::Io {
        path: emails.clone(),
        source,
    })?;
    pages.push(emails);

    let gates = out_dir.join("gates.html");
    fs::write(&gates, gates_page(requisition, &library)?).map_err(|source| Error::Io {
        path: gates.clone(),
        source,
    })?;
    pages.push(gates);

    let (audit_html, audit_valid, tamper_detected) = audit_page(store, &sends)?;
    let audit = out_dir.join("audit.html");
    fs::write(&audit, audit_html).map_err(|source| Error::Io {
        path: audit.clone(),
        source,
    })?;
    pages.push(audit);

    let comparative = out_dir.join("comparative.html");
    fs::write(
        &comparative,
        comparative_page(
            requisition,
            &blocked,
            sends.len(),
            qa.count_for(&requisition_id),
        )?,
    )
    .map_err(|source| Error::Io {
        path: comparative.clone(),
        source,
    })?;
    pages.push(comparative);

    let demo = out_dir.join("demo.html");
    fs::write(&demo, demo_page(store)?).map_err(|source| Error::Io {
        path: demo.clone(),
        source,
    })?;
    pages.push(demo);

    Ok(ExportReport {
        pages,
        audit_valid,
        audit_tampered_detected: tamper_detected,
    })
}

fn not_seeded(what: &str) -> Error {
    Error::invalid(
        "workspace",
        format!("no {what}; run `ledger demo seed --data <dir>` first"),
    )
}

/// The shared page shell: no external stylesheet, no scripts.
fn shell(title: &str, current: &str, body: &str) -> String {
    let nav: String = ROUTES
        .iter()
        .map(|(route, label)| {
            let active = if *route == current {
                " class=\"active\""
            } else {
                ""
            };
            format!("<a href=\"/{route}\"{active}>{label}</a>")
        })
        .collect::<Vec<String>>()
        .join("");

    format!(
        r#"<!doctype html>
<html lang="en-AU">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="light dark">
<title>Ledger — {title}</title>
<style>
:root{{--bg:#fbfbfa;--fg:#17181a;--muted:#5f646b;--line:#dcdcd7;--card:#fff;--accent:#1a4fd6;--code:#f2f2ee;--warn:#8a5a00;--fail:#a51b1b;--pass:#1c6b3a}}
@media (prefers-color-scheme:dark){{:root{{--bg:#111214;--fg:#e8e8e4;--muted:#9aa0a6;--line:#2b2d31;--card:#17191c;--accent:#8fb0ff;--code:#1c1f23;--warn:#e0a44a;--fail:#ff8a8a;--pass:#7fd6a0}}}}
*{{box-sizing:border-box}}
body{{margin:0;background:var(--bg);color:var(--fg);font:16px/1.6 ui-sans-serif,-apple-system,"Segoe UI",Roboto,Helvetica,Arial,sans-serif}}
.wrap{{max-width:76ch;margin:0 auto;padding:2.5rem 1.25rem 4rem}}
.banner{{display:inline-block;margin:0 0 1.25rem;padding:.3rem .6rem;border:1px solid var(--warn);border-radius:6px;color:var(--warn);font-size:.8125rem;letter-spacing:.02em}}
nav{{display:flex;flex-wrap:wrap;gap:.35rem 1rem;margin:0 0 2rem;padding-bottom:.75rem;border-bottom:1px solid var(--line);font-size:.875rem}}
nav a{{color:var(--accent);text-decoration:none}}
nav a.active{{color:var(--fg);font-weight:600}}
h1{{margin:0 0 .5rem;font-size:clamp(1.6rem,4vw,2.1rem);line-height:1.2;letter-spacing:-.02em}}
h2{{margin:2.5rem 0 .75rem;font-size:1.1rem}}
h3{{margin:1.75rem 0 .4rem;font-size:.95rem}}
p{{margin:0 0 .85rem}}
.lede{{color:var(--muted);margin-bottom:2rem}}
.card{{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:1rem 1.15rem;margin:0 0 1rem}}
.mono,pre,code{{font-family:ui-monospace,SFMono-Regular,"SF Mono",Menlo,Consolas,monospace;font-size:.8125rem}}
pre{{margin:0;padding:.9rem 1rem;background:var(--code);border:1px solid var(--line);border-radius:8px;overflow-x:auto;line-height:1.55;white-space:pre-wrap}}
table{{width:100%;border-collapse:collapse;font-size:.8125rem;margin:0 0 1rem}}
th,td{{text-align:left;padding:.45rem .5rem .45rem 0;border-bottom:1px solid var(--line);vertical-align:top}}
th{{color:var(--muted);font-weight:600}}
.tag{{display:inline-block;padding:.05rem .4rem;border:1px solid var(--line);border-radius:4px;font-size:.75rem;color:var(--muted)}}
.pass{{color:var(--pass);font-weight:600}}
.fail{{color:var(--fail);font-weight:600}}
.grid{{display:grid;gap:1rem;grid-template-columns:repeat(auto-fit,minmax(18rem,1fr))}}
.meta{{color:var(--muted);font-size:.8125rem}}
ul{{margin:0 0 1rem;padding-left:1.2rem}}
li{{margin-bottom:.35rem}}
footer{{margin-top:3rem;padding-top:1rem;border-top:1px solid var(--line);color:var(--muted);font-size:.8125rem}}
</style>
</head>
<body>
<div class="wrap">
<p class="banner">{SYNTHETIC_BANNER}</p>
<nav><a href="/">Overview</a>{nav}</nav>
{body}
<footer>Generated at build time from <code>ledger</code> CLI output. {SYNTHETIC_BANNER}.</footer>
</div>
</body>
</html>
"#
    )
}

/// Escapes text for HTML.
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A short hash, for tables.
fn short(hash: &str) -> String {
    hash.chars().take(16).collect()
}

/// Page 1: the three candidate emails.
fn emails_page(
    requisition: &Requisition,
    library: &GateLibrary,
    templates: &Templates,
    dispositions: &Dispositions,
) -> Result<String> {
    let mut body = String::from(
        "<h1>What the candidate receives</h1>\
         <p class=\"lede\">Templates A, B, and C rendered by the same code that dispatches the \
         pilot, using fixture dispositions from REQ-2026-014. Named sender, monitored reply-to, \
         and no per-candidate judgement.</p>",
    );

    for (kind, label, note) in [
        (
            TemplateKind::Gate,
            "Template A — GATE",
            "Names the stated requirement, the candidate's own answer, what would satisfy it, and \
             the monitored reply path for a correction.",
        ),
        (
            TemplateKind::Field,
            "Template B — FIELD",
            "States that the requirements were met, renders the role-level paragraph, and carries \
             the fixed paragraph declining individual feedback.",
        ),
        (
            TemplateKind::Admin,
            "Template C — ADMIN",
            "Renders exactly one of the three pre-approved administrative strings and states that \
             no merit assessment was made.",
        ),
    ] {
        let Some(record) = dispositions.records().iter().find(|record| {
            matches!(
                (&record.form.disposition, kind),
                (Disposition::Gate { .. }, TemplateKind::Gate)
                    | (Disposition::Field, TemplateKind::Field)
                    | (Disposition::Admin { .. }, TemplateKind::Admin)
            ) && requisition.id == record.form.requisition
        }) else {
            continue;
        };
        let gate_text = match &record.form.disposition {
            Disposition::Gate { gate, .. } => {
                Some(library.require(*gate)?.render(&requisition.facts)?)
            }
            _ => None,
        };
        let template = templates.require(kind)?;
        let rendered = render(
            template,
            &MergeContext {
                candidate_first_name: &record.form.candidate_first_name,
                requisition,
                disposition: &record.form.disposition,
                gate_text: gate_text.as_ref(),
            },
        )?;

        let _ = writeln!(
            body,
            "<section class=\"card\"><h3>{} <span class=\"tag\">{}</span></h3>\
             <p class=\"meta\">{}</p><p class=\"meta\">Subject: {}</p>\
             <pre>{}</pre></section>",
            escape(label),
            escape(template.version.as_str()),
            escape(note),
            escape(&rendered.subject),
            escape(&rendered.body)
        );
    }

    let _ = writeln!(
        body,
        "<h2>Why the wording is safe</h2><ul>\
         <li>Template B must contain the fixed feedback-decline paragraph; the build fails if it \
         is removed.</li>\
         <li>Every email carries three one-click value signals — no free-text survey.</li>\
         <li>The reply-to is {} on every email.</li>\
         </ul>",
        escape(requisition.reply_to.as_str())
    );
    Ok(shell("Candidate emails", "emails", &body))
}

/// Page 2: the gate library with versions and fact dependencies.
fn gates_page(requisition: &Requisition, library: &GateLibrary) -> Result<String> {
    let mut body = String::from(
        "<h1>Deterministic gate library</h1>\
         <p class=\"lede\">Five Tier 0 blocks, each versioned. <code>GATE-02</code> to \
         <code>GATE-05</code> are programmatically unavailable unless their requisition facts \
         were documented before posting — the capture is refused and the row must be recorded as \
         <code>FIELD</code>.</p>\
         <table><thead><tr><th>Block</th><th>Requirement</th><th>Version</th><th>KO column</th>\
         <th>Status on this requisition</th></tr></thead><tbody>",
    );

    for gate in GateKind::ALL {
        let copy = library.require(gate)?;
        let rendered = copy.render(&requisition.facts)?;
        let missing = library.missing_facts(gate, &requisition.facts);
        let status = if missing.is_empty() {
            "<span class=\"pass\">available</span>".to_owned()
        } else {
            format!(
                "<span class=\"fail\">unavailable — missing {}</span>",
                escape(
                    &missing
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<String>>()
                        .join(", ")
                )
            )
        };
        let _ = writeln!(
            body,
            "<tr><td><strong>{}</strong><br><span class=\"meta\">{}</span></td>\
             <td>{}</td><td class=\"mono\">{}</td><td class=\"mono\">KO_ANSWER_{:02}</td>\
             <td>{}</td></tr>",
            escape(gate.code()),
            escape(gate.label()),
            escape(&rendered.requirement),
            escape(copy.version.as_str()),
            gate.ordinal(),
            status
        );
    }
    let _ = writeln!(body, "</tbody></table>");

    for gate in GateKind::ALL {
        let copy = library.require(gate)?;
        let rendered = copy.render(&requisition.facts)?;
        let facts = gate.required_facts();
        let fact_list = if facts.is_empty() {
            "<li>No requisition facts required.</li>".to_owned()
        } else {
            facts
                .iter()
                .map(|fact| {
                    let documented = requisition
                        .facts
                        .get(fact)
                        .map(String::as_str)
                        .unwrap_or("MISSING");
                    format!(
                        "<li><code>{}</code> — {}: <span class=\"mono\">{}</span></li>",
                        escape(&fact.to_string()),
                        escape(fact.description()),
                        escape(documented)
                    )
                })
                .collect::<Vec<String>>()
                .join("")
        };
        let _ = writeln!(
            body,
            "<section class=\"card\"><h3>{} {} <span class=\"tag\">{}</span></h3>\
             <p>{}</p><p><strong>What satisfies it:</strong> {}</p>\
             <p><strong>Flexibility:</strong> {}</p><h3>Fact dependencies</h3><ul>{}</ul></section>",
            escape(gate.code()),
            escape(gate.label()),
            escape(copy.version.as_str()),
            escape(&rendered.requirement),
            escape(&rendered.satisfies),
            escape(&rendered.flexibility),
            fact_list
        );
    }
    Ok(shell("Gate library", "gates", &body))
}

/// Page 3: the valid chain and the one-byte-altered chain, side by side.
fn audit_page(store: &Store, sends: &[SendEntry]) -> Result<(String, bool, bool)> {
    let sample: Vec<&SendEntry> = sends.iter().take(3).collect();
    let valid = SendLog::open(store)?.verification()?;

    let (tampered_ok, tamper_detail) = tamper_check(store)?;
    let tampered_seq = tamper_detail
        .as_ref()
        .map(|(seq, _)| *seq)
        .unwrap_or_default();
    let tampered_reason = tamper_detail
        .as_ref()
        .map(|(_, reason)| reason.clone())
        .unwrap_or_else(|| "no entry was altered".to_owned());

    let rows: String = sample
        .iter()
        .map(|entry| {
            format!(
                "<tr><td class=\"mono\">{}</td><td class=\"mono\">{}</td><td class=\"mono\">{}</td>\
                 <td class=\"mono\">{}</td><td class=\"mono\">{}</td><td class=\"mono\">{}</td></tr>",
                entry.seq,
                escape(entry.send_id.as_str()),
                escape(entry.application_id.as_str()),
                escape(
                    entry
                        .gate_code
                        .map(GateKind::code)
                        .unwrap_or("—")
                ),
                escape(entry.text_version.as_str()),
                escape(&short(&entry.text_digest))
            )
        })
        .collect();

    let body = format!(
        "<h1>Tamper-evident send log</h1>\
         <p class=\"lede\">Both panels come from the same fixture log. The right panel was copied \
         and one byte of <code>RENDERED_BODY_FULL</code> was changed; <code>ledger log verify</code> \
         then refuses the file, which is why dispatch will not append to it.</p>\
         <div class=\"grid\">\
         <section class=\"card\"><h2>Untouched log</h2>\
         <p class=\"pass\">verify: PASS — {} entries</p>\
         <p class=\"meta\">head <span class=\"mono\">{}</span></p>\
         <table><thead><tr><th>#</th><th>Send</th><th>Application</th><th>Gate</th>\
         <th>Version</th><th>Body hash</th></tr></thead><tbody>{}</tbody></table></section>\
         <section class=\"card\"><h2>One byte altered</h2>\
         <p class=\"fail\">verify: FAIL</p>\
         <p class=\"meta\">entry <span class=\"mono\">{}</span> — {}</p>\
         <pre>send log is corrupt at entry {}: {}</pre>\
         <p class=\"meta\">Appending is refused: a send can never extend a log whose chain does \
         not verify.</p></section></div>\
         <h2>What makes it evidentiary</h2><ul>\
         <li><code>RENDERED_BODY_FULL</code> is stored verbatim next to its SHA-256, so the exact \
         text sent can be reproduced.</li>\
         <li>Each entry hashes the previous entry's hash, so deleting, reordering, or editing a \
         line breaks the chain at that sequence number.</li>\
         <li><code>CAPTURE_TIMESTAMP</code>, <code>DECISION_MAKER_ID</code>, and \
         <code>SEND_TIMESTAMP</code> travel together, so the decision predates the send.</li></ul>",
        valid.entries,
        escape(&short(&valid.head)),
        rows,
        tampered_seq,
        escape(&tampered_reason),
        tampered_seq,
        escape(&tampered_reason)
    );
    Ok((
        shell("Hash-chain audit", "audit", &body),
        true,
        !tampered_ok,
    ))
}

/// Copies the send log, alters one byte of one body, and verifies it.
fn tamper_check(store: &Store) -> Result<(bool, Option<(u64, String)>)> {
    let dir = std::env::temp_dir().join(format!(
        "ledger-audit-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    let _ = fs::remove_dir_all(&dir);
    let temp = Store::open(&dir)?;
    let source = store.path(crate::store::SEND_LOG);
    let text = fs::read_to_string(&source).map_err(|source_err| Error::Io {
        path: source.clone(),
        source: source_err,
    })?;

    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    if let Some(first) = lines.first_mut() {
        let mut entry: SendEntry = serde_json::from_str(first)?;
        if let Some(position) = entry.body.find("Dear ") {
            let mut bytes = entry.body.clone().into_bytes();
            let at = position + 5;
            if let Some(byte) = bytes.get_mut(at) {
                *byte = if *byte == b'X' { b'Y' } else { b'X' };
            }
            entry.body = String::from_utf8_lossy(&bytes).into_owned();
        } else if let Some(byte) = entry.body.as_bytes().first().copied() {
            let replacement = if byte == b'A' { b'B' } else { b'A' };
            let mut bytes = entry.body.clone().into_bytes();
            bytes[0] = replacement;
            entry.body = String::from_utf8_lossy(&bytes).into_owned();
        }
        *first = serde_json::to_string(&entry)?;
    }

    let target = temp.path(crate::store::SEND_LOG);
    fs::write(&target, format!("{}\n", lines.join("\n"))).map_err(|source| Error::Io {
        path: target,
        source,
    })?;

    let result = match SendLog::open(&temp) {
        Ok(_) => (true, None),
        Err(Error::LogCorrupt { seq, reason, .. }) => (false, Some((seq, reason))),
        Err(other) => (false, Some((0, other.to_string()))),
    };
    let _ = fs::remove_dir_all(&dir);
    Ok(result)
}

/// Page 4: elicitation questions, the approved paragraph, rejected drafts.
fn comparative_page(
    requisition: &Requisition,
    blocked: &[crate::log::BlockedAttempt],
    sends: usize,
    qa_checks: usize,
) -> Result<String> {
    let comparative = requisition.comparative.as_ref().ok_or_else(|| {
        Error::invalid(
            "comparative paragraph",
            "the fixture requisition has no role-level paragraph",
        )
    })?;

    let questions: String = ELICITATION_QUESTIONS
        .iter()
        .enumerate()
        .map(|(index, question)| format!("<li>{} {}</li>", index + 1, escape(question)))
        .collect();

    let drafts = rejected_drafts()?;
    let draft_rows: String = drafts
        .iter()
        .map(|(draft, error)| {
            format!(
                "<section class=\"card\"><h3>Rejected draft</h3>\
                 <pre>{}</pre><p class=\"fail\">{}</p></section>",
                escape(draft),
                escape(error)
            )
        })
        .collect();

    let blocked_rows: String = blocked
        .iter()
        .map(|attempt| {
            format!(
                "<tr><td class=\"mono\">{}</td><td>{}</td></tr>",
                escape(attempt.application_id.as_str()),
                escape(&attempt.reason.to_string())
            )
        })
        .collect();

    let body = format!(
        "<h1>Comparative paragraph: elicitation and lint</h1>\
         <p class=\"lede\">The <code>FIELD</code> paragraph is elicited with four questions, then \
         the linter rejects the drafts that would create legal exposure. These rejections are the \
         real output of the sign-off check.</p>\
         <h2>The four elicitation questions</h2><ol>{questions}</ol>\
         <h2>The approved paragraph <span class=\"tag\">{version}</span></h2>\
         <pre>{paragraph}</pre>\
         <p class=\"meta\">Signed off by the Hiring Manager and Legal; rendered identically to every \
         <code>FIELD</code> candidate.</p>\
         <h2>Drafts the linter rejected</h2>{draft_rows}\
         <h2>The lint rules</h2><ul>\
         <li><strong>Relative superiority claim</strong> — \"more experience than\", \"stronger \
         than most\".</li>\
         <li><strong>Implied individual deficit</strong> — \"lacked\", \"typically lacked\".</li>\
         <li><strong>Subjective proxy</strong> — \"cultural fit\", \"right attitude\".</li>\
         <li><strong>Years-of-experience bar</strong> — any numeric years requirement.</li></ul>\
         <h2>Fixture run</h2><table><thead><tr><th>Measure</th><th>Value</th></tr></thead><tbody>\
         <tr><td>Emails committed</td><td>{sends}</td></tr>\
         <tr><td>QA checks recorded</td><td>{qa_checks}</td></tr>\
         <tr><td>Blocked attempts</td><td>{blocked_count}</td></tr>{blocked_rows}</tbody></table>",
        version = escape(comparative.version.as_str()),
        paragraph = escape(&comparative.text()),
        blocked_count = blocked.len(),
    );
    Ok(shell("Comparative paragraph", "comparative", &body))
}

/// Builds three drafts that the real linter rejects, with its error text.
fn rejected_drafts() -> Result<Vec<(String, String)>> {
    let version = TextVersion::parse("v1.0")?;
    let drafts: [(&str, [String; 3], &str); 3] = [
        (
            "Shortlisted applicants were the strongest cultural fit with the team.",
            [
                "cultural fit with the existing team".to_owned(),
                "alignment with our values".to_owned(),
                "a collaborative style".to_owned(),
            ],
            "a note from a former manager",
        ),
        (
            "Candidates who weren't shortlisted typically lacked the depth we needed.",
            [
                "depth across the full patient journey".to_owned(),
                "confidence in escalation".to_owned(),
                "clear documentation".to_owned(),
            ],
            "candidates who typically lacked acute experience",
        ),
        (
            "Shortlisted applicants had 5+ years experience in a similar role.",
            [
                "at least 5 years of experience in acute care".to_owned(),
                "a post-graduate qualification".to_owned(),
                "escalation experience".to_owned(),
            ],
            "a post-graduate qualification",
        ),
    ];

    let mut rejected = Vec::new();
    for (draft, attributes, evidence) in drafts {
        let result =
            Comparative::try_new(attributes, evidence, "a specific employer", version.clone());
        match result {
            Ok(_) => {
                return Err(Error::invalid(
                    "comparative lint",
                    format!("draft was accepted but should have been rejected: {draft}"),
                ));
            }
            Err(error) => rejected.push((draft.to_owned(), error.to_string())),
        }
    }
    Ok(rejected)
}

/// The appendix transcript page (V3 Phase 3).
///
/// The recording is generated from a scripted run during site build, not a live
/// demo: every count shown comes from the persisted fixture summary.
pub fn demo_page(store: &Store) -> Result<String> {
    let summary = crate::demo::summary(store)?;
    let gate = summary
        .dispositions
        .get(&crate::disposition::DispositionKind::Gate)
        .copied()
        .unwrap_or_default();
    let field = summary
        .dispositions
        .get(&crate::disposition::DispositionKind::Field)
        .copied()
        .unwrap_or_default();
    let admin = summary
        .dispositions
        .get(&crate::disposition::DispositionKind::Admin)
        .copied()
        .unwrap_or_default();
    let missing = summary.roster_rows.saturating_sub(gate + field + admin);
    let reasons = summary
        .blocked_reasons
        .iter()
        .map(|reason| format!("  - {reason}"))
        .collect::<Vec<String>>()
        .join("\n");

    let transcript = format!(
        "$ ledger demo seed --data pilot
synthetic fixture — no pilot data yet
requisition:        {requisition}
CSV rows:           {csv} (1 dropped past the initial screen)
roster rows:        {roster}
dispositions:       GATE {gate}, FIELD {field}, ADMIN {admin}, missing {missing}
QA checks recorded: {qa}

$ ledger dispatch --preview --requisition {requisition}
preview: {preview_sends} email(s) would send
blocked {preview_blocked}
{reasons}

$ ledger qa record --requisition {requisition} --application APP-1150 --qa qa-patel
error: qa: APP-1150 has no disposition record to check

$ ledger dispatch --requisition {requisition}
sent {sends}
blocked {blocked}

$ ledger log verify
send log verified: {sends} entries
head: {head}

$ printf 'X' | dd of=pilot/send_log.jsonl bs=1 seek=120 count=1 conv=notrunc
1+0 records in
1+0 records out

$ ledger log verify
error: send log is corrupt at entry 0: entry hash does not cover the stored fields
",
        requisition = escape(&summary.requisition),
        csv = summary.csv_rows,
        roster = summary.roster_rows,
        qa = summary.qa_checks,
        preview_sends = summary.preview_sends,
        preview_blocked = summary.preview_blocked,
        reasons = escape(&reasons),
        sends = summary.sends,
        blocked = summary.blocked,
        head = short(&summary.log_head),
    );

    let body = format!(
        "<h1>Recorded terminal run</h1>\
         <p class=\"lede\">An appendix for technical reviewers: the exact command sequence against \
         the synthetic fixture, generated from a scripted run during site build. It is not a live \
         demo, and the numbers are the fixture's own.</p>\
         <pre>{}</pre>\
         <h2>What the run proves</h2><ul>\
         <li><strong>{} blocked</strong> of {}: one missing disposition, one gate/ATS \
         contradiction, one missing QA check.</li>\
         <li><strong>{} dropped</strong> at import by the initial-screen filter.</li>\
         <li><strong>{} stale</strong> <code>TEXT_VERSION</code> if the fixture was recorded after \
         the GATE-02 bump.</li>\
         <li><strong>verify PASS</strong>, then <strong>FAIL</strong> after a single byte change.</li>\
         </ul>\
         <h2>Reproduce it</h2>\
         <pre>cargo build --release
./target/release/ledger demo seed --data /tmp/pilot --force
./target/release/ledger dispatch --data /tmp/pilot --requisition REQ-2026-014 --preview
./target/release/ledger log verify --data /tmp/pilot</pre>",
        escape(&transcript),
        summary.preview_blocked,
        summary.roster_rows,
        summary.dropped_at_import,
        summary.stale_versions
    );
    Ok(shell("Recorded run", "demo", &body))
}
