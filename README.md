# Ledger

Audit-grade candidate feedback pilot, implemented in Rust. The operational spec is
[`V2.md`](V2.md); the demo-fixture spec is [`V3.md`](V3.md); the earlier v1
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

## Working on this repo

[`SKILL.md`](SKILL.md) is the project skill: the seven specs compressed, the
decisions ledger (and why), the invariants with the tests that enforce them, the
failure modes that actually bit, and the current open work. Read it before changing
anything non-trivial.

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

## Demo fixture (V3)

The deterministic synthetic fixture that the tests and the walkthrough use. Nothing
in it is pilot data.

```bash
# Deterministic fixture: 151 CSV rows, 150 roster rows, 12 GATE / 129 FIELD / 8 ADMIN,
# 1 missing disposition, 1 gate/ATS contradiction, 1 missing QA, 1 dropped at import.
ledger demo seed --data pilot --force            # pre-dispatch: preview shows 3 blocked
ledger dispatch --data pilot --requisition REQ-2026-014 --preview

# Recorded variant, including the GATE-02 version bump:
ledger demo seed --data pilot --force --record
```

The fixture is byte-deterministic: two seeds in two directories produce the same
fingerprint and the same log head, which the test suite asserts.

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

### Recruiter workspace (`ledger ui`)

Behind an optional feature so the default build stays the offline CLI:

```bash
cargo build --release --features ui
ledger --data pilot --operator rec-rui ui --port 8420
# prints http://127.0.0.1:8420/?token=<32-byte hex>
```

* **One operator per process.** The session is bound at launch; identity here is a
  name plus a `0600` keyfile, not an account, so switching operators is a relaunch.
* **Token → cookie.** The first `GET /?token=` consumes the token and sets
  `HttpOnly; SameSite=Strict` `ledger_session`, then 303s to `/`. Every other
  request needs the cookie (`401` otherwise).
* **RBAC per request.** Mutations call `Role::permits`, so an auditor gets `403`
  on `/api/dispose*` and a `200` on `/audit`.
* **Lock discipline.** Writes take `Store::lock` (`flock`) inside
  `spawn_blocking`; `Error::Locked` becomes `409` with the holder. Reads take a
  fresh snapshot and never block on a CLI run.
* **No assets on the wire.** `tokens.css`, `app.css` and `app.js` are embedded with
  `include_str!`; nothing is fetched, so it works air-gapped.

| Route | Method | Role | Purpose |
| --- | --- | --- | --- |
| `/` | GET | any signed-in | Triage table + four KPI cards |
| `/api/preview` | POST | any signed-in | Render the email for a disposition |
| `/api/dispose` | POST | coordinator | Capture one disposition |
| `/api/dispose/batch` | POST | coordinator | Atomic batch (new + `supersedes`) |
| `/governance` | GET | any signed-in | 7 artefact verdicts + checklist |
| `/audit` | GET | any signed-in | Chain check + operator activity |

Not yet wired into the UI: the import column mapper, the `.eml`/zip dispatch
route, one-click re-approval, the snapshot button, and delivery confirmation.

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

### Workspace preview (`web/`)

`web/` is the public face of the pilot: a landing page at `/` that explains what the
tool is, how a rejection is recorded and why the pilot exists, and the recruiter
workspace itself at `/triage`, `/import`, `/dispatch`, `/governance` and `/audit`. The
landing page carries real captures of those screens (`web/public/product/`), the
fixture's own chain values in its diagram, and one live action: it hashes the sample
record's text with WebCrypto.
TanStack Start + React 19, built in [Lovable](https://lovable.dev) (project
`f0481c24-dcfa-4ea5-9b31-1d0a285832ee`), deployed to Vercel as the project's
production site (`ledger-pilot`, domain `ledger-pilot-chi.vercel.app`).

**It is a UI preview, not the ledger.** The roster is a 48-row session fixture, every
change is in-memory, and nothing is signed, chained or persisted. The app says so on
every screen (`Sample data · Not connected to the ledger`, `UI preview`); the Rust CLI
remains the only thing that records a real disposition. Do not present browser-side
"dispositions" as Ed25519 signatures or a verified append log.

```bash
cd web
bun install --frozen-lockfile
bun run dev                      # http://localhost:5173 — landing, then /triage
bun run lint                     # eslint + prettier (ignores build output)
bun run test                     # vitest: 6 tests
NITRO_PRESET=vercel bun run build  # writes .vercel/output (Build Output API v3)
```

Vercel builds `web/` on every push to `main` (Root Directory `web`, install
`bun install --frozen-lockfile`, build `NITRO_PRESET=vercel bun run build`). The Rust
workspace is untouched by that build: no `ledger` binary, no workspace directory, no
`cargo` at deploy time. `web/AGENTS.md` carries Lovable's own rules — do not rewrite
history on the connected branch, and change DB/secret/AI-gateway pieces in Lovable,
not by hand.

### No generated site

`ledger` still has no static site generator: nothing under `site/` is generated,
committed or served. The only published bytes are the `web/` app above, and the only
server the Rust code runs is the loopback-only `ledger ui`.

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

136 tests (144 with `--features ui`) cover: the initial-screen hard filter, the three-variant form, superseding
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
