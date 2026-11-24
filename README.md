# Ledger

Audit-grade candidate feedback pilot, implemented in Rust. The operational spec is
[`V2.md`](V2.md); the site and demo-fixture spec is [`V3.md`](V3.md); the earlier v1
scope is [`plan.md`](plan.md).

The point of the tool is that V2's constraints are not review instructions. They
live in the code and in the logs: enums and validation that reject a bad row, a
lint that rejects bad copy, and append-only files that cannot be quietly rewritten.
Some of that is checked by the compiler, more of it by tests at runtime — the
claim is "enforced in code and logs", not "unrepresentable".

## What V2 requires, and where it lives

| V2 task | Implementation |
| --- | --- |
| 1.1 Retention sign-off | `policy.rs` — `retention_days` on the record (`ledger governance`) |
| 1.2 Monitored mailbox + escalation | `id::Mailbox` refuses `no-reply@`; `policy.rs` holds the named ER/Legal owner and SLA |
| 1.3 Single-pass sign-off | `approval.rs` — one signature per role, HM + Legal required, on the gate library, the requisition, the paragraph, the templates, and the playbook |
| 2.1 Locked form | `disposition.rs` — `GATE`/`FIELD`/`ADMIN` only, no free text, no "Other", named `DECISION_MAKER_ID`, append-only, `SUPERSEDES` corrections |
| 2.2 Versioned gate blocks | `gate.rs` — `GATE-01`..`GATE-05` with `TEXT_VERSION`; `GATE-02`..`05` declare the requisition facts they need and are **unavailable** without them |
| 2.3 Role-level paragraph | `requisition.rs` — elicited into five slots and rejected at sign-off for relative superiority, implied deficit, subjective proxies, or a years bar |
| 2.4 Templates + value signal | `template.rs` — Templates A/B/C with allow-listed slots, `{{FEEDBACK_DECLINE}}`, `{{REENGAGEMENT_LINE}}`, and three one-click `mailto:` signals |
| 2.5 Reply playbook | `playbook.rs` — five signed-off scripts; the discrimination script carries the one-business-day escalation |
| 3.1 ATS export + hard filter | `ats.rs` — `KO_ANSWER_01..05` → `GATE-01..05`; rows past the initial screen are dropped at import |
| 3.2 Immutable send log | `log.rs` — every V2 column, SHA-256 chained, refuses to extend a broken log |
| 3.3 Validation script | `pipeline.rs` — three circuit-breakers, then render, then append |
| 4.1 Runbook | `qa.rs` — QA required for 100% of `GATE` and a deterministic 10% sample of `FIELD`/`ADMIN`; `dispatch --preview` for the dry run |
| 4.2 Five metrics | `metrics.rs` — pre-registered green/amber/red bands |
| 4.3 Routing decision | `review.rs` — the six V2 consequences, applied in order |
| Task sheet | `checklist.rs` — the thirteen tasks, signed by a named person |

## Quick start

```bash
cargo build --release

# 1. Governance and static copy (single pass, HM + Legal)
ledger init --data pilot
ledger governance set --data pilot --escalation-owner "M. Okafor, Employee Relations"
for role in hiring-manager legal; do
  ledger gate approve      --data pilot --actor "J. Chen"   --role $role
  ledger templates approve --data pilot --actor "M. Okafor" --role $role
  ledger playbook approve  --data pilot --actor "M. Okafor" --role $role
done

# 2. Requisition with its facts documented before posting
ledger requisition create --data pilot --id REQ-2026-014 \
  --employer "Example Health" --title "Registered Nurse" \
  --reply-to careers-feedback@example.com.au \
  --sender-name "R. Patel" --sender-title "Recruitment Coordinator" \
  --gate right-to-work --gate work-rights-duration \
  --fact ROLE_END_OR_MIN_TERM="30 June 2027" \
  --fact MIN_TERM="12 months" \
  --fact ROLE_DURATION_REASON="The appointment is a fixed-term 12-month contract."

# 3. The comparative paragraph, then its sign-off
ledger requisition comparative --data pilot --id REQ-2026-014 \
  --attribute-1 "assessment of deteriorating patients" \
  --attribute-2 "escalation to a registrar within their first hour" \
  --attribute-3 "documentation a covering clinician could act on" \
  --evidence "a de-identified case note attached to the application" \
  --non-factors "a post-graduate qualification or a specific employer"
ledger requisition approve --data pilot --id REQ-2026-014 --actor "J. Chen" --role hiring-manager
ledger requisition approve --data pilot --id REQ-2026-014 --actor "M. Okafor" --role legal
ledger requisition approve --data pilot --id REQ-2026-014 --comparative --actor "J. Chen" --role hiring-manager
ledger requisition approve --data pilot --id REQ-2026-014 --comparative --actor "M. Okafor" --role legal

# 4. Import, capture, QA, dispatch
ledger ats import --data pilot --requisition REQ-2026-014 --file export.csv
ledger disposition record --data pilot --requisition REQ-2026-014 \
  --application APP-1001 --first-name Ada --email ada@example.com \
  --recruiter rec-rui --decision-maker dm-chen \
  --category GATE --gate work-rights-duration --answer shorter-than-role
ledger dispatch --data pilot --requisition REQ-2026-014 --preview
ledger qa record --data pilot --requisition REQ-2026-014 --application APP-1001 --qa qa-patel
ledger dispatch --data pilot --requisition REQ-2026-014

# 5. Evidence and decision
ledger log verify --data pilot
ledger log export --data pilot --requisition REQ-2026-014 --out send-log.csv
ledger reply  record --data pilot --requisition REQ-2026-014 --application APP-1002 --scenario next-time-guidance
ledger signal record --data pilot --requisition REQ-2026-014 --value yes --count 2
ledger metrics --data pilot --requisition REQ-2026-014
ledger review criteria --data pilot --requisition REQ-2026-014 --status agreed-one-session --actor "M. Okafor"
ledger review route --data pilot --requisition REQ-2026-014
```

## Demo fixture and artefact site (V3)

Everything on the site is generated from the CLI at build time, from a deterministic
fixture. Nothing on it is pilot data.

```bash
# Deterministic fixture: 151 CSV rows, 150 roster rows, 12 GATE / 129 FIELD / 8 ADMIN,
# 1 missing disposition, 1 gate/ATS contradiction, 1 missing QA, 1 dropped at import.
ledger demo seed --data pilot --force            # pre-dispatch: preview shows 3 blocked
ledger dispatch --data pilot --requisition REQ-2026-014 --preview

# Recorded variant for the site, including the GATE-02 version bump:
ledger demo seed --data pilot --force --record

# Write the artefact pages straight from the loaded workspace.
ledger dispatch --data pilot --requisition REQ-2026-014 --preview --export-html site
```

`scripts/build-site.sh` runs the seed and the export, and records `site/demo.cast`
when `asciinema` is installed. The fixture is byte-deterministic: two seeds in two
directories produce the same fingerprint and the same log head, which the test suite
asserts.

| Route | Page |
| --- | --- |
| `/emails` | Templates A/B/C exactly as a candidate receives them |
| `/gates` | Five blocks, versions, and fact dependencies |
| `/audit` | Valid chain and the one-byte-altered chain side by side |
| `/comparative` | Four elicitation questions, approved paragraph, rejected drafts |
| `/demo` | Scripted terminal run, 3 blocked of 150, verify PASS → tamper → FAIL |

Every page carries `Synthetic fixture — no pilot data yet`.

## ATS export schema

```csv
APPLICATION_ID,FIRST_NAME,EMAIL,REQ_ID,ROLE_TITLE,STAGE_AT_REJECTION,KO_ANSWER_01,KO_ANSWER_02,APPLIED_DATE
APP-1001,Ada,ada@example.com,REQ-2026-014,Registered Nurse,Initial screen,not-held,shorter-than-role,2026-11-01
```

`APPLICATION_ID`, `FIRST_NAME`, `EMAIL`, `REQ_ID`, and `STAGE_AT_REJECTION` are
required. `KO_ANSWER_01..KO_ANSWER_05` map to `GATE-01..GATE-05` in order. Any row
whose stage is not the initial screen is **dropped at import** — an interviewed
candidate can never enter the roster, so `Template B` cannot reach them.

## Gate availability

| Gate | Meaning | Required facts |
| --- | --- | --- |
| `GATE-01` | Right to work in Australia | — |
| `GATE-02` | Work-rights duration vs role duration | `ROLE_END_OR_MIN_TERM`, `MIN_TERM`, `ROLE_DURATION_REASON` |
| `GATE-03` | Legislated registration or accreditation | `REGULATOR_URL` (https, verified) |
| `GATE-04` | Licence inherent to the duties | `INHERENT_REQUIREMENT_REASON` |
| `GATE-05` | Location / work location | `LOCATION_REQUIREMENT`, `LOCATION_SATISFACTION_TEXT`, `WAIVER_TEXT` |

If a gate's facts are missing, `GATE` capture for that gate is refused with an
instruction to record `FIELD`. That is V2's fallback, enforced rather than advised.

## The three circuit-breakers

1. **Completeness** — no send without a locked, timestamped disposition.
2. **Gate consistency** — a `GATE` answer must equal the candidate's own
   `KO_ANSWER_0n`; a mismatch blocks and tells the operator to escalate, never to
   auto-correct.
3. **Version resolution** — the logged `TEXT_VERSION` is the version of the block
   that actually rendered (`GATE-0n` version for `GATE`, paragraph version for
   `FIELD`, template version for `ADMIN`), and nothing renders from unsigned copy.

Plus the runbook guard: a `GATE` send needs a recorded second-person QA check, and
one in ten `FIELD`/`ADMIN` applications needs one too.

## Metrics and routing

| Metric | Green | Amber | Red |
| --- | --- | --- | --- |
| Reply volume (per 1,000 sends) | < 30 | 30–80 | > 80 |
| Form-block rate | < 10% | 10–25% | > 25% |
| Gate-mismatch rate | < 2% | 2–10% | > 10% |
| Criteria feasibility | yes, 1 session | yes, external input | no |
| Candidate-value signal ("Yes" share) | > 50% | 30–50% | < 30% |

`review route` applies V2's consequences in order: candidate-value red → **stop**;
gate-mismatch red → **flag discrimination exposure**; form-block red → **capture
discipline middleware**; form-block green + criteria yes → **reassess venture
viability**; reply volume red → **scope reply triage**; criteria no → **pivot to
criteria validation**.

## Workspace files

`demo-summary.json`, `demo-ats.csv`,
`gate_library.json`, `templates.json`, `playbook.json`, `requisitions.json`,
`roster-<REQ>.json`, `dispositions.jsonl`, `qa.jsonl`, `send_log.jsonl`,
`blocked.jsonl`, `replies.jsonl`, `signals.jsonl`, `review.json`,
`checklist.json`, `policy.json`.

Every `*.jsonl` file is append-only: there is no update or delete code path.

## Crash safety, locking and recovery

- Every append writes one complete newline-terminated line, then `fsync`s the file
  and its parent directory.
- A trailing partial line is a **torn write**: the log refuses to load, and
  `dispatch` refuses to run. There is no silent repair.
- `ledger log quarantine --reason "<note>"` moves the torn tail into
  `send_log.jsonl.<timestamp>.quarantine`, truncates the log to its last complete
  line, re-verifies the chain, and records the action in `operator.jsonl`.
- Mutating commands take an exclusive `flock` on the workspace; a second process
  exits `4` with the holder's PID and timestamp. The lock is released by the kernel
  when a process dies, so a leftover `.lock` file is diagnostic, never stale in the
  blocking sense.
- State files are written temp → `fsync` → rename → `fsync` directory.

`tests/crash.rs` kills a real `dispatch` after 70 of 147 sends and asserts the
two branches: clean restart commits exactly 77 more with no duplicate application;
a torn tail refuses (`exit 3`) until it is quarantined, after which the same 77
commit.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | success |
| `1` | internal/IO error |
| `2` | circuit-breaker block (dispatch committed nothing; `--allow-blocked` opt-out) |
| `3` | invalid input, schema or state (including a torn log tail) |
| `4` | workspace lock contention |

### Operators, roles and signatures

```bash
ledger operator register --data pilot --name rec-rui --role coordinator
ledger operator register --data pilot --name j-chen  --role approver
ledger operator list --data pilot
ledger --operator rec-rui disposition record ...      # signed by rec-rui
ledger verify-signatures --data pilot                # re-checks every signature
```

Each operator is an Ed25519 keypair: the public half in `operators.json`, the secret
half in `operators/<name>.key` at `0600`. Dispositions are signed over a canonical
digest of the record, so a later reader can tell who authorised a decision without
trusting the file. Signatures live **outside** the send-log digest: the chain proves
the log is unedited, signatures prove who decided what. `verify_log.py` reports
"signatures not checked"; `ledger verify-signatures` does that in Rust and exits
`3` on any invalid signature, any stale sign-off, or any unsigned sign-off in a
registered workspace.

| Role | May run (read-only commands are open to all) |
| --- | --- |
| `coordinator` | `ats import`, `disposition`, `qa`, `dispatch`, `reply`, `signal`, `anchor`, `log` |
| `approver` | `gate`, `templates`, `playbook`, `requisition`, `governance`, `purge`, `migrate`, `checklist`, `init`, `operator` |
| `auditor` | read-only only: `status`, `metrics`, `log verify/export`, `find`, `verify-signatures` |

A workspace with no registry is in bootstrap: only `init`, `demo seed` and
`operator register` run until someone registers, then every mutating command needs
a role. Dispatch and `purge` take an automatic snapshot into `backups/` first.

### Drafted dispatch (`.eml`)

```bash
ledger --operator qa-patel dispatch --data pilot --requisition REQ-2026-014 \
  --eml pilot/batch --allow-blocked
```

Writes one RFC 5322 draft per permitted recipient plus `manifest.json` (per-file
SHA-256 and a batch digest). Every send recorded this way is `queued`, never
`sent`: a draft is not a delivery, and editing it in Outlook afterwards shows up as
a manifest mismatch rather than being silently absorbed into the log.

### Retention, migration and anchoring

| Command | What it does |
| --- | --- |
| `ledger purge --yes` | Tombstones PII in dispositions and ATS snapshots older than `policy.retention_days`, records a `PURGE` event. `--dry-run` reports without writing. The **send log is retained**: its PII is inside the hash chain, so it is erased by deleting the workspace at the end of retention, not by selective edits. |
| `ledger migrate --yes` | Snapshots the workspace to `backups/<UTC>/`, then migrates unchained state. Chained logs are never rewritten; an unknown `schema_version` fails closed. |
| `ledger anchor [--git-commit]` | Publishes the chain head as `anchors/<UTC>-<head>.json`. Copy it off the machine, or let it commit locally. No network calls, by design. |
| `ledger find --hash <#handle>` | Resolves a redacted handle printed by an error back to its record. |

`--json` is available on `demo seed`, `dispatch`, `metrics`, `log verify`,
`log quarantine`, `review route`, `purge`, `migrate` and `anchor`; the remaining
subcommands print prose.

Console output redacts application ids and email addresses to stable `#hash`
handles; `--debug-ids` (or `LEDGER_DEBUG_IDS=1`) reveals them. Destructive
commands (`purge`, `migrate`, `log quarantine`, `demo seed --force`) require an
explicit flag. The tool never sets `RUST_BACKTRACE`; it simply never prints a
backtrace itself.

### PII on disk

The workspace directory is `0700` and every state and log file is `0600`,
enforced on creation regardless of the process umask. `tests/` asserts this.

### Site pages

`/`, `/emails`, `/gates`, `/audit`, `/comparative`, `/demo`, `/format`,
`/threat-model`, `/verify` and `/cli`, plus `404.html`, `robots.txt` and
`sitemap.xml`. Every generated page carries build provenance: commit SHA, the
commit timestamp used as generation time, and the fixture fingerprint. Two runs of
`scripts/build-site.sh` on one commit are byte-identical, which is what makes the
CI drift check meaningful.

Deployment uses the existing Vercel↔GitHub integration on `main`: Vercel's build
image has no Rust toolchain, so it serves the committed pages that
`scripts/build-site.sh` produced, and CI fails if those pages differ from a fresh
generation. That keeps "what was reviewed" and "what is deployed" the same bytes
without building Rust at deploy time.

## Supported platforms

macOS (aarch64, x86_64) and Linux (glibc) are supported and tested; releases are
built by `.github/workflows/release.yml` with SHA-256 checksums. **Windows is not
supported** and no Windows binary is published: nothing here has been run on
it, and shipping an untested cross-compiled binary is worse than not shipping one.
CSV quirks that Windows users care about — UTF-8 BOM, CRLF, quoted commas,
embedded newlines — are covered as *input* concerns and tested on macOS.

## Documented `expect` sites

`clippy::unwrap_used`, `clippy::panic` and `clippy::indexing_slicing` are denied
crate-wide (tests excluded via `clippy.toml`). `clippy::expect_used` is
**deliberately not denied**: these are constants, and turning them into `Result`
would add error paths that cannot fire. Each is annotated:

| Site | Why it cannot fail |
| --- | --- |
| `gate::GateLibrary::seed` — `TextVersion::parse("v1.0")` | the literal matches `v<digits>.<digits>` |
| `demo::base_time` — `with_ymd_and_hms(...).single()` | fixed literal date at midnight, single-valued |
| `approval`/`gate` tests | test-only |

## Formats and threat model

- [`FORMAT_SPEC.md`](FORMAT_SPEC.md) — canonical byte encoding, digest coverage,
  chain rules, schema versions, and the known gaps.
- [`THREAT_MODEL.md`](THREAT_MODEL.md) — what the chain proves, what it does not,
  and the off-host anchoring that wholesale rewriting requires.

## Development

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

129 tests cover: the initial-screen hard filter, the three-variant form, superseding
corrections, gate unavailability falling back to `FIELD`, the paragraph lint,
required template slots, QA enforcement, the chain detecting edits and truncation,
the five metric bands, the routing order, the fixture's determinism and planted
failure cases, an interrupted dispatch resuming without duplicates, a torn tail
failing closed until quarantined, workspace locking, golden-format compatibility,
and agreement between the Rust chain and the standalone Python verifier.

## What the tool does not do

It does not send email, call the ATS, or verify that the mailbox is monitored. Per
V2's scope, dispatch stays manual: the tool renders the exact text, applies the
circuit-breakers, and records the send. Pasting the rendered email into the ATS or
mail client is the operator's step.
