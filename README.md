# Ledger

Audit-grade candidate feedback for a manual hiring pilot. A Rust command line tool
records the reason for every rejection and keeps an append-only, hash-chained log of
the exact text each candidate received; a browser workspace shows the recruiter what
that looks like before they commit to it.

**Live workspace:** **https://ledger-pilot-chi.vercel.app** — no account, generated
sample data, nothing to install.

- **Try the recorder quickly:** `cargo build --release && ./target/release/ledger demo seed --data /tmp/pilot --force --record`
- **Try the preview quickly:** open `/triage`, click a candidate, record a disposition.
- **Check the claim:** `./verify_log.py /tmp/pilot/send_log.jsonl` verifies 147 entries in a
  stdlib-only script that has no access to the tool.

---

## The problem

Most rejected candidates hear nothing, or hear "we had a lot of strong applicants".
Neither answer tells them which requirement they missed. Neither answer can be
checked by anyone afterwards — not by the candidate, not by Legal, not by a reviewer
looking at the same decision a year later. When the only record of what was said is
someone's memory of an email, the employer cannot show that a rule was applied rather
than a preference, and the candidate cannot tell a mistake from a decision.

Ledger exists so that the answer to **"why was I rejected, and who decided that?"** is
a record someone can open, not a recollection someone has to be trusted on.

## What Ledger is

Operational tooling for a deliberately narrow pilot: **one employer, one requisition,
the initial screen only, and every rejected candidate** — no sampling, no opt-in. A
record for one candidate contains:

| Part | What it holds |
| --- | --- |
| Disposition | `GATE` (a stated requirement not met), `FIELD` (met, a stronger field appointed) or `ADMIN` (closed for an unrelated reason) — no free text, no "other" |
| Copy | The gate block or template version that actually rendered, and the hash of that copy |
| People | A named decision maker, a recruiter, and a second person who checked the email |
| Text | The subject and body exactly as sent, and the SHA-256 digest of the body |
| Proof | The previous entry's hash, this entry's hash, the schema version, and both timestamps |

It is not a recruitment platform. It does not post jobs, rank candidates, send email,
or talk to an ATS.

## How it works

Five steps, in this order, each leaving a file behind:

1. **Document the requisition before it posts.** Role end date, minimum term, the
   reason for that term, the regulator, the location. A gate block whose facts are
   missing is unavailable, so the disposition falls back to `FIELD` rather than
   inventing a requirement.
2. **Sign the copy.** Five knockouts, `GATE-01`…`GATE-05`, each versioned. Hiring
   manager and Legal both sign, and the signature binds to the copy's hash: editing
   the text makes the approval stale instead of silently inherited.
3. **Capture the disposition.** One locked form, a named decision maker, a locked
   timestamp. A correction supersedes the earlier record; nothing is overwritten.
4. **Check it twice.** Every `GATE` email and a deterministic 10% sample of `FIELD`
   and `ADMIN` need a second person's QA before anything leaves.
5. **Render, then chain.** The tool renders an RFC 5322 draft, appends one hash-chained
   entry per send, and never claims delivery — a draft is `queued` until a person
   confirms it arrived.

Three circuit-breakers sit between the form and the log:

- **Completeness** — no locked, timestamped disposition means no send.
- **Gate consistency** — a form answer that contradicts the candidate's own ATS
  knockout answer blocks and escalates; it is never auto-corrected.
- **Version resolution** — the logged `TEXT_VERSION` is the copy that rendered;
  nothing renders from unsigned copy.

The chain is what makes it evidence: each entry hashes the head before it, so an
edit anywhere breaks every link after it. `verify_log.py` (Python standard library
only) re-derives the same answer without importing any Ledger code.

---

## Try it

### In the browser (preview)

Open **https://ledger-pilot-chi.vercel.app**:

1. `/triage` — 48 generated candidates. Click one, choose a disposition, record it.
   The pending metric and the dispatch count change.
2. `/dispatch` — the draft it queued, with a preview and a zip export carrying a
   SHA-256 manifest. Nothing is sent.
3. `/governance` and `/audit` — five hash-bound artefacts, all unsigned, and a chain
   reported as *not connected*, because this page is a preview.
4. `/import` — drop `sample-ats.csv` (linked on the page); six of its eight rows
   import, two are excluded for being past the initial screen.

**The preview is not the ledger.** It is session-only sample data with no auth, no
storage and no signatures; reload and it resets. The Rust CLI is the recorder.

### On the command line (the real recorder)

Requires Rust **1.85 or newer** (`cargo --version`). Python 3 is only needed for the
independent verifier.

```bash
cargo build --release
./target/release/ledger demo seed --data /tmp/pilot --force --record
```

You should see the fixture's exact shape:

```text
CSV rows:           151
roster rows:        150
dropped at import:  1
dispositions:       GATE 12, FIELD 129, ADMIN 8, missing 1
preview:            147 would send, 3 blocked
emails committed:   147
blocked:            3
stale versions:     1
log head:           d347a639fff01dadf90822d5f833b0b76ba5d7b3d8fec39d0a78491d7b6c897b
fingerprint:        64f9e1f9e8b4b9ce30c38f2f7c82c904906e5c4c2d9d23caf5eca6df73833ca2
```

Verify it without trusting the binary:

```bash
./verify_log.py /tmp/pilot/send_log.jsonl
```

```text
OK /tmp/pilot/send_log.jsonl: 147 entries verified
head: d347a639fff01dadf90822d5f833b0b76ba5d7b3d8fec39d0a78491d7b6c897b
```

Prove it detects tampering — flip one byte and verify again:

```bash
cp -r /tmp/pilot /tmp/pilot-tampered
printf 'X' | dd of=/tmp/pilot-tampered/send_log.jsonl bs=1 seek=120 count=1 conv=notrunc
./target/release/ledger log verify --data /tmp/pilot-tampered
# error: log .../send_log.jsonl is corrupt at entry 0: entry hash does not cover the stored fields
```

---

## Reproduce it from scratch

### Prerequisites

| Tool | Version used | Needed for |
| --- | --- | --- |
| Rust | 1.85+ (CI compiles 1.85) | the CLI |
| Python | 3.10+ | `verify_log.py` only |
| Bun | 1.3+ | the browser preview (npm works too) |
| Node | 22+ | the preview build, if you skip Bun |

```bash
git clone https://github.com/VasanthSaravanann/Ledger.git
cd Ledger
cargo test --locked          # 136 tests (144 with --features ui)
```

### A complete pilot from an empty directory

This is the whole lifecycle, verified end to end. **Two things trip people up and are
called out below:** the first operator you register becomes the acting identity, so
register the approvers first; and once more than one operator exists you must name the
actor with `--operator`.

```bash
L=./target/release/ledger
$L init --data pilot

# 1. Operators. The FIRST one becomes the acting identity, so start with an approver:
$L operator register --data pilot --name "M. Okafor" --role approver
$L --operator "M. Okafor" operator register --data pilot --name "J. Chen" --role approver
$L --operator "M. Okafor" operator register --data pilot --name "R. Patel" --role coordinator
$L --operator "M. Okafor" operator register --data pilot --name "qa-patel" --role coordinator
$L --operator "M. Okafor" governance set --data pilot \
  --escalation-owner "M. Okafor, Employee Relations"

# 2. Sign the static copy, both roles, single pass:
for role in hiring-manager legal; do
  $L --operator "M. Okafor" gate approve      --data pilot --actor "J. Chen"   --role $role
  $L --operator "M. Okafor" templates approve --data pilot --actor "M. Okafor" --role $role
  $L --operator "M. Okafor" playbook approve  --data pilot --actor "M. Okafor" --role $role
done

# 3. The requisition, with the facts its gate blocks depend on:
$L --operator "M. Okafor" requisition create --data pilot --id REQ-2026-014 \
  --employer "Example Health" --title "Registered Nurse" \
  --reply-to careers-feedback@example.com.au \
  --sender-name "R. Patel" --sender-title "Recruitment Coordinator" \
  --gate right-to-work --gate work-rights-duration \
  --fact ROLE_END_OR_MIN_TERM="30 June 2027" \
  --fact MIN_TERM="12 months" \
  --fact ROLE_DURATION_REASON="The appointment is a fixed-term 12-month contract."

# 4. The comparative paragraph for FIELD, then its separate sign-off:
$L --operator "M. Okafor" requisition comparative --data pilot --id REQ-2026-014 \
  --attribute-1 "assessment of deteriorating patients" \
  --attribute-2 "escalation to a registrar within their first hour" \
  --attribute-3 "documentation a covering clinician could act on" \
  --evidence "a de-identified case note attached to the application" \
  --non-factors "a post-graduate qualification or a specific employer"

$L --operator "M. Okafor" requisition approve --data pilot --id REQ-2026-014 --actor "J. Chen"   --role hiring-manager
$L --operator "M. Okafor" requisition approve --data pilot --id REQ-2026-014 --actor "M. Okafor" --role legal
$L --operator "M. Okafor" requisition approve --data pilot --id REQ-2026-014 --comparative --actor "J. Chen"   --role hiring-manager
$L --operator "M. Okafor" requisition approve --data pilot --id REQ-2026-014 --comparative --actor "M. Okafor" --role legal

# 5. Import the roster (the filter drops anyone past the initial screen):
$L --operator "R. Patel" ats import --data pilot \
  --requisition REQ-2026-014 --file examples/export.csv

# 6. Capture one disposition, preview, QA, dispatch:
$L --operator "R. Patel" disposition record --data pilot --requisition REQ-2026-014 \
  --application APP-1001 --first-name Ada --email ada@example.com \
  --recruiter "R. Patel" --decision-maker "J. Chen" \
  --category GATE --gate right-to-work --answer not-held
$L --operator "R. Patel" dispatch --data pilot --requisition REQ-2026-014 --preview
$L --operator "qa-patel" qa record --data pilot --requisition REQ-2026-014 \
  --application APP-1001 --qa "qa-patel"
$L --operator "R. Patel" dispatch --data pilot --requisition REQ-2026-014 --allow-blocked

# 7. Evidence:
$L --operator "R. Patel" log verify  --data pilot
$L --operator "R. Patel" metrics     --data pilot --requisition REQ-2026-014
$L --operator "R. Patel" review route --data pilot --requisition REQ-2026-014
```

`examples/export.csv` is a real four-row export; row `APP-1004` is at `Interview`, so
the import reports `excluded 1 row(s) past the initial screen (hard filter)`.

### The browser preview locally

```bash
cd web
bun install --frozen-lockfile
bun run dev                        # http://localhost:5173
bun run test                       # 6 tests
bun run lint                       # eslint + prettier
NITRO_PRESET=vercel bun run build  # what Vercel runs
```

The build runs Nitro with the `vercel` preset by default, so it writes
`.vercel/output` (Build Output API v3) with no environment variable set. Setting
`NITRO_PRESET` overrides the target if you build for something else.

### Deploying it yourself

The app is a separate project from the CLI, and Vercel needs four settings:

| Setting | Value |
| --- | --- |
| Root Directory | `web` |
| Install Command | `bun install --frozen-lockfile` |
| Build Command | `NITRO_PRESET=vercel bun run build` |
| Output Directory | *(empty — Vercel reads `.vercel/output`)* |

Then push to `main`. No Rust toolchain is needed at deploy time: the CLI is not built
or run by the site.

### Gotchas that will bite

| Symptom | Cause | Fix |
| --- | --- | --- |
| `invalid operator: register an operator first` | every mutation needs an identity | register approvers first (step 1 above) |
| `operator R. Patel (coordinator) may not run 'operator'` | the first registered operator becomes the acting identity | pass `--operator "M. Okafor"` (or set `LEDGER_OPERATOR`) |
| `unknown answer "held"` | KO answers are a fixed vocabulary per gate | `not-held`, `shorter-than-role`, `restricted`, `not-registered`, `expired`, `outside-required-location` |
| `no ATS export imported` | `dispatch` refuses an empty roster | run `ats import` first |
| `error: cannot update the lock file ... --locked` | you edited `Cargo.toml` | run once without `--locked`, commit `Cargo.lock` |
| `ledger ui` not found | the `ui` feature is off | `cargo build --release --features ui` |

---

## Repository map

| Path | What it is |
| --- | --- |
| `src/` | the CLI: 33 modules, `main.rs` is the only command surface |
| `src/digest.rs`, `src/log.rs` | canonical encoding and the hash-chained send log — the evidence layer |
| `src/disposition.rs`, `src/gate.rs`, `src/requisition.rs` | the locked form, the five versioned knockouts, the paragraph lint |
| `src/pipeline.rs`, `src/qa.rs`, `src/ats.rs` | circuit-breakers, second-person sampling, CSV import and its filter |
| `src/identity.rs`, `src/approval.rs`, `src/verify.rs` | Ed25519 operators, hash-bound sign-off, workspace verification |
| `web/` | the browser preview: TanStack Start + React 19, deployed at `/` and `/triage`…`/audit` |
| `tests/` | integration tests, golden byte fixtures, HTTP tests for `ledger ui` |
| `fuzz/` | `parse_csv` and `parse_log` fuzz targets (weekly in CI) |
| `verify_log.py` | independent verifier: chain, digests, monotonic time, no imports |
| `FORMAT_SPEC.md` | the normative byte format: what the digest covers, and what it does not |
| `THREAT_MODEL.md` | what the chain proves, what it cannot, and why anchoring matters |
| `SKILL.md` | the project's own compressed documentation: decisions, invariants, failure history |
| `CONTRIBUTING.md` | coverage floors, lint policy, format-change procedure |
| `examples/export.csv` | a real ATS export to feed `ats import` |
| `scripts/` | `demo-walkthrough.sh` (tamper demo), `coverage-floors.sh` |

## Verification, not assertions

- **`verify_log.py`** re-checks the chain, every digest and timestamp ordering using
  only the standard library. It is the reason the format is specified rather than
  implied.
- **Golden fixtures** in `tests/golden.rs` pin the head, the log digest, the
  dispositions digest and the fixture fingerprint, so a `serde` rename cannot silently
  invalidate an existing log.
- **Tamper demo**: `scripts/demo-walkthrough.sh` seeds, dispatches, verifies, flips one
  byte, and shows the failure.
- **Independent of the tool**: `THREAT_MODEL.md` states plainly what this does not
  prove — a rewrite of the whole file from genesis is invisible without an off-host
  copy of the chain head, which `ledger anchor` produces.

## Limitations

Honest list; each one is a decision, not an oversight.

| Limitation | Detail |
| --- | --- |
| The web app is a preview | 48 generated candidates, session-only, no auth, no storage, no signatures. Only the CLI records a real disposition. |
| No email sending | Drafts and a manifest only. Delivery stays with a mail client. |
| `queued` ≠ `sent` | Nothing moves a draft to `sent` without a person confirming it; there is no delivery-detection integration. |
| No ATS integration | One CSV in, drafts out. No API, no polling, no write-back. |
| Manual by design | No model in the send path, no ranking, no automation of the decision. |
| Single operator per process | Identity is a name plus a `0600` keyfile, bound at launch; switching means relaunching. |
| macOS and Linux only | No Windows binary is built or tested, and the README says so. |
| Retention is coarse | `purge` tombstones dispositions and rosters; send-log PII lives inside the chain, so end-of-retention erasure means deleting the workspace, not editing it. |
| No published release | `release.yml` builds and checksums tarballs on a tag; no tag has been cut yet. |
| Specification documents are archived | The original `plan.md`/`V2.md`…`website.md` are no longer in the tree; they remain readable at `git show 90dfcb6:V2.md`. |

## Security and privacy

- Console output redacts application ids and emails to stable `#hash` handles;
  `--debug-ids` (or `LEDGER_DEBUG_IDS=1`) reveals them.
- The workspace directory is `0700`, every state file `0600`, enforced on creation
  regardless of umask; one writer per workspace via `flock`.
- Nothing in the dispatch path opens a socket. The only HTTP server is `ledger ui`,
  which binds loopback behind the optional `ui` feature.
- Destructive commands need an explicit flag (`purge --yes`, `migrate --yes`,
  `log quarantine --yes`, `demo seed --force`), so scripts never mutate by accident.

## Reference

**Exit codes:** `0` ok · `1` internal/IO · `2` blocked (`--allow-blocked` opts out) ·
`3` invalid input, schema or state (including a torn tail) · `4` lock contention.

**Roles:** `coordinator` (import, disposition, QA, dispatch, delivery, review, reply,
signal, anchor, log) · `approver` (gate, templates, playbook, requisition, governance,
purge, migrate, checklist, operator, init, demo seed) · `auditor` (read-only).

**Metric bands**, fixed before the pilot: reply volume per 1,000 sends `<30 / 30–80 / >80`;
form-block rate `<10% / 10–25% / >25%`; gate-mismatch rate `<2% / 2–10% / >10%`;
candidates who found the reply useful `>50% / 30–50% / <30%`; plus whether criteria
feasibility was agreed in one session.

**Routing**, applied in this order: candidate value red → stop; gate mismatch red →
flag discrimination exposure; form-block red → capture-discipline middleware;
form-block green with criteria agreed → reassess venture viability; reply volume red →
scope reply triage; criteria not agreed → validate criteria first.

## JSON output

`--json` is a global flag: `ledger status --json`, `ledger gate approve … --json`, and
`ledger --json metrics …` all work. When it is set, the command prints **exactly one
JSON object** on stdout and nothing on stderr; exit codes do not change. Every object
carries `"command"`, the leaf command that produced it, so a runbook can switch on it.

| Command | Keys beside `command` |
| --- | --- |
| `init` | `workspace`, `seeded[]` |
| `demo seed` | `summary` — the fixture numbers, head, fingerprint |
| `status` | `tasks{signed,total,items[]}`, `artefacts{...}`, `evidence{...}`, `requisitions[]` |
| `governance show`, `set` | `policy{retention_days,escalation_owner,escalation_sla_hours}` |
| `gate list` | `library` — copies and approvals |
| `gate approve` | `actor`, `role`, `library` |
| `gate bump-version` | `gate`, `version`, `library` |
| `requisition create`, `fact`, `comparative`, `approve` | `requisition`; `key` for `fact`; `paragraph` for `comparative`; `comparative`, `role`, `still_needed` for `approve` |
| `requisition show` | `requisition`, `gates[]`, `comparative`, `blocking_issues[]` |
| `requisition list` | `requisitions[]` |
| `templates show`, `approve` | `templates`; `role` when approving |
| `playbook show`, `approve` | `playbook`; `role` when approving |
| `playbook script` | `scenario`, `text`, `escalation` |
| `ats import` | `requisition`, `imported`, `with_knockouts`, `excluded_not_initial_screen` |
| `disposition record` | `record` |
| `disposition list`, `history` | `requisition` or `application`, `records[]` |
| `qa record` | `recorded`, `requisition`, `application`, `scope` |
| `qa list` | `requisition`, `records[]` |
| `dispatch` | `mode`, `status`, `sent`, `previews`, `blocked`, `blocked_reasons[]`, and `eml` when drafts were written |
| `delivery confirm` | `confirmation` — the new chained entry |
| `delivery list` | `entries`, `confirmations[]` |
| `log verify` | `valid`, `entries`, `head` |
| `log quarantine` | `report` |
| `log export` | `requisition`, `rows[]` (or `rows` count and `out` when `--out` was given) |
| `reply record` | `requisition`, `recorded`, `scenario`, `escalation` for a discrimination concern |
| `signal record` | `requisition`, `value`, `recorded` |
| `metrics` | `metrics` — sends, confirmed, queued, replies, blocked and the five ratings |
| `review criteria` | `record` |
| `review route` | `routing` — metrics, criteria and the ordered decisions |
| `checklist done` | `completion`, `progress{signed,total}` |
| `checklist show` | `signed`, `total`, `tasks[]` |
| `purge` | `report` |
| `migrate` | `report` |
| `anchor` | `anchor` — entries, head, operator, receipt path |
| `find` | `handle`, `matches[]` |
| `operator register` | `operator`, `keyfile` |
| `operator list` | `operators[]` |
| `operator whoami` | `operator` |
| `verify-signatures` | `report` — signed, unsigned, invalid and every artefact verdict |
| `ui` | `url`, `operator`, then the server runs |

`tests/cli_json.rs` enumerates the clap tree and fails if a command lacks the flag, then
runs a fixture and parses what each command actually printed.

## Documentation

- [`SKILL.md`](SKILL.md) — the operating manual: decisions and why, invariants with the
  tests that enforce them, failure modes that actually bit.
- [`CHANGELOG.md`](CHANGELOG.md) — the complete log: every commit, what it changed and
  the reasoning that is not in the diff.
- [`PRODUCTION.md`](PRODUCTION.md) — the production-readiness loop: a 19-task backlog in
  `.plan/tasks.json`, each with a shell gate, and a runner an agent can execute until
  only signatures remain (`node scripts/plan-next.mjs`).
- [`FORMAT_SPEC.md`](FORMAT_SPEC.md) — the normative byte format.
- [`THREAT_MODEL.md`](THREAT_MODEL.md) — what the chain does and does not prove.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — how to change it without breaking the format.

## Licence

MIT — see [`LICENSE`](LICENSE).
