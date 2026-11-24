# Ledger

Audit-grade candidate feedback pilot, implemented in Rust. The operational spec is
[`V2.md`](V2.md); the site and demo-fixture spec is [`V3.md`](V3.md); the earlier v1
scope is [`plan.md`](plan.md).

The point of the tool is that V2's constraints are not review instructions — they
are types, lint rules, and append-only logs.

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

## Development

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

82 tests cover: the initial-screen hard filter, the three-variant form, superseding
corrections, gate unavailability falling back to `FIELD`, the paragraph lint,
required template slots, QA enforcement, the chain detecting edits and truncation,
the five metric bands, the routing order, and the fixture's determinism and planted
failure cases.

## What the tool does not do

It does not send email, call the ATS, or verify that the mailbox is monitored. Per
V2's scope, dispatch stays manual: the tool renders the exact text, applies the
circuit-breakers, and records the send. Pasting the rendered email into the ATS or
mail client is the operator's step.
