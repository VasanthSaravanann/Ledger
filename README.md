# Ledger

Rust tooling for the **v1 manual candidate feedback pilot** described in
[`plan.md`](plan.md): one employer, one high-volume requisition, initial screen
only, 100% of rejected candidates, no ATS API integration.

The pilot's constraints are enforced by the program rather than by a process
document:

| Plan task | Where it lives |
| --- | --- |
| 1.2 Single-pass HM + Legal sign-off | `approval.rs` — a role signs once, and copy is unusable until both have |
| 2.1 Locked disposition form | `disposition.rs` — a three-variant enum, no free text, no "Other", append-only |
| 2.2 Tier 0 gate-text library | `gate.rs` — five Australian knockouts, each with requirement / satisfaction / waiver |
| 2.3 Role-level comparative paragraph | `requisition.rs` — one static paragraph, signed off separately, no merge slots |
| 2.4 Three master templates | `template.rs` — allow-listed merge slots, required slots per category |
| 3.1 ATS export and merge mapping | `ats.rs` — CSV keyed on `application_id`, optional `knockout_*` answers |
| 3.2 Immutable send log | `log.rs` — append-only JSONL, SHA-256 hash chain, refuses to extend a broken log |
| 3.3 Gate-blocking protocol | `pipeline.rs` — no send without a completed, timestamped disposition |
| 4.1 Pilot execution | `ledger dispatch` |
| 4.2 The three decision metrics | `metrics.rs` — reply volume rate, form-block rate, criteria feasibility |
| 4.3 Go/no-go review | `review.rs` — policy thresholds and the recommendation |
| 1.1, 1.3, and the rest | `checklist.rs` — the task sheet, signed off by a named person |

## Quick start

```bash
cargo build --release

ledger init --data pilot            # seed gate copy, templates, policy

# Phase 1-2: sign off the static copy in one pass
ledger templates approve --data pilot --actor "J. Chen" --role hiring-manager
ledger templates approve --data pilot --actor "M. Okafor" --role legal
ledger gate approve      --data pilot --actor "J. Chen" --role hiring-manager
ledger gate approve      --data pilot --actor "M. Okafor" --role legal

# The requisition, its comparative paragraph, and its sign-off
ledger requisition create --data pilot --id REQ-2026-014 \
  --employer "Example Health" --title "Registered Nurse" \
  --reply-to careers-feedback@example.com.au \
  --gate visa-restrictions --gate right-to-work
ledger requisition comparative --data pilot --id REQ-2026-014 --file paragraph.txt
ledger requisition approve --data pilot --id REQ-2026-014 --actor "J. Chen" --role hiring-manager
ledger requisition approve --data pilot --id REQ-2026-014 --actor "M. Okafor" --role legal
ledger requisition approve --data pilot --id REQ-2026-014 --comparative --actor "J. Chen" --role hiring-manager
ledger requisition approve --data pilot --id REQ-2026-014 --comparative --actor "M. Okafor" --role legal

# Phase 3-4: import, capture, dispatch, measure
ledger ats import --data pilot --requisition REQ-2026-014 --file export.csv
ledger disposition record --data pilot --requisition REQ-2026-014 \
  --application APP-1001 --email ada@example.com --name "Ada Lovelace" \
  --recruiter rec-rui --category failed-gate \
  --gate visa-restrictions --answer restricted
ledger dispatch --data pilot --requisition REQ-2026-014 --preview
ledger dispatch --data pilot --requisition REQ-2026-014
ledger log verify --data pilot
ledger reply record --data pilot --requisition REQ-2026-014 --theme gate-question
ledger metrics --data pilot --requisition REQ-2026-014
ledger review criteria --data pilot --requisition REQ-2026-014 --status agreed --actor "M. Okafor"
ledger review report --data pilot --requisition REQ-2026-014
ledger status --data pilot
```

`ledger status` prints the `plan.md` task sheet next to the evidence the
workspace actually holds.

## ATS export schema

```csv
application_id,candidate_name,email,knockout_visa-restrictions
APP-1001,Ada Lovelace,ada@example.com,restricted
APP-1002,Grace Hopper,grace@example.com,
```

`application_id`, `candidate_name`, and `email` are required. Any
`knockout_<gate>` column is optional; when present, the disposition's gate
answer must agree with it or the send is blocked.

## The three metrics

1. **Reply volume rate** — replies ÷ sends × 1,000.
2. **Form-block rate** — blocked attempts ÷ (blocked attempts + sends). A block is
   counted once per application and reason, so retries do not inflate it.
3. **Criteria validation feasibility** — recorded, not computed: `agreed`,
   `disputed`, or `not-assessed`.

`review report` applies the policy in `policy.json` (defaults: ≤ 20 replies per
1,000 sends, ≤ 25% blocked). The rules are:

- criteria not agreed → **run criteria-validation consulting** (the v2 Tier 1
  fork is unresolved, whatever the metrics say);
- criteria agreed and no threshold breached → **proceed to v2 engineering**;
- criteria agreed but a threshold breached → **hold and iterate** on the manual
  pipeline.

## What the tool does not do

- It does not send email or call the ATS. Per the plan's scope, dispatch is
  manual: the tool renders the exact text, applies the gate-blocking rule, and
  records the send. Pasting the rendered email into the ATS or mail client is the
  operator's step.
- It does not verify that the reply mailbox is monitored. It does refuse
  `no-reply`-style addresses.

## Workspace files

| File | Contents |
| --- | --- |
| `gate_library.json` | the five copy blocks and their sign-off |
| `templates.json` | the three master templates and their sign-off |
| `requisitions.json` | requisitions, comparative paragraphs, sign-offs |
| `roster-<REQ>.json` | the imported ATS snapshot |
| `dispositions.jsonl` | append-only disposition captures |
| `send_log.jsonl` | append-only, hash-chained sends with exact text |
| `blocked.jsonl` | append-only refusals from the gate-blocking protocol |
| `replies.jsonl` | append-only reply tally |
| `review.json` | criteria-feasibility decisions |
| `checklist.json` | plan-task sign-offs |
| `policy.json` | go/no-go thresholds |

## Development

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

Invariants covered by tests: a disposition is captured once and never edited; a
gate answer must be one the gate offers; a requisition and its paragraph need HM
and Legal sign-off before any send; every email names the monitored mailbox; the
same comparative text goes to every candidate; an edited or truncated send log is
detected before the next append; and dispatch is idempotent.
