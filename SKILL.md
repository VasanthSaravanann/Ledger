---
name: ledger-pilot
description: >-
  Build, change, review or operate Ledger — the audit-grade candidate-feedback pilot
  at ~/Desktop/Ledger: a Rust CLI that records every rejection in signed copy and an
  append-only hash chain, plus a TanStack Start workspace preview at
  ledger-pilot-chi.vercel.app. Trigger whenever a task touches this repo or its
  concepts: dispositions (GATE/FIELD/ADMIN), gate copy GATE-01..05, comparative
  paragraph, templates A/B/C, reply playbook, ATS CSV import, QA sampling, send log /
  hash chain / canonical digest / schema_version, signatures / operators / RBAC /
  sign-off drift, metrics and go-no-go routing, the demo fixture, `ledger ui`, the
  web/ preview, or a V-task id. Also use it when the user says "the pilot",
  "candidate feedback", "immutable log", "verify-signatures", or "the workspace".
---

# Ledger — project skill

The operating manual: what the product is, what must not break, how to run and ship it.
The **complete log** — every commit, in order, with the reasoning — is `CHANGELOG.md`;
what is left before a real pilot lives in `PRODUCTION.md`; how a stranger reproduces it
lives in `README.md`. This file carries the current state (§2) and points at the rest.

## 1. What this is

A tool for a **manual** candidate-feedback pilot: one employer, one requisition, the
initial screen, **every** rejected candidate. Each rejection carries a reason a named
person chose, copy Legal signed, and an append-only hash-chained log that proves the
exact text sent.

Two surfaces, one system:

| Surface | What it is | Truthful status |
| --- | --- | --- |
| `src/` — Rust CLI | the recorder: dispositions, gate copy, QA, draft dispatch, chained log, verification, retention, anchoring | the only thing that records a real decision |
| `web/` — TanStack Start app | `/` explains the pilot; `/triage`, `/import`, `/dispatch`, `/governance`, `/audit` preview the workspace with 48 generated candidates | session-only preview: no auth, no storage, no signatures |

Non-goals, by design: no model in the send path, no email sending, no ATS API, no
hosted tenancy, no Windows binary, no generated static site.

## 2. Current state (verified, not assumed)

- Rust 2024, MSRV **1.85** (compiled in CI), 11 direct crates plus one dev-dependency
  (`proptest`), **no network crates** in the default graph. Optional `ui` feature adds
  axum + tokio.
- 34 modules, ~15.3k lines, **155 tests** (163 with `--features ui`), including three
  proptest suites.
- 26 top-level CLI commands, `--json` on every one (a single global flag: one object on
  stdout, nothing on stderr).
- Web: 6 routes, 11 tests, eslint 0 errors (7 known `react-refresh` warnings). The skip
  link is the first tab stop on every route, and the checker passes `skip-link`,
  `focus-ring`, `focus-visible`, `focus-hidden`, `tap-size` and `menu-toggle` at 390
  and 320.
- Every state document carries `schema_version: 1`; a newer version fails closed and
  `ledger migrate` upgrades a version-0 workspace without touching the chained logs.
- Deployed at **https://ledger-pilot-chi.vercel.app** from `web/` on every push to
  `main`; CI runs `test`, `msrv`, `web`, `supply-chain`, `coverage`, plus weekly
  `mutants` and `fuzz`. `verify` runs daily at 06:17 UTC; `update` opens grouped
  dependency PRs weekly at 05:00 UTC.
- Release **v0.1.0** exists: three targets, 14 assets, a `.sha256` per artefact and
  `verify_log.py` inside every tarball.
- The pilot checklist is **0/13 signed** — every legal act is still human and undone.

### The plan, after the autonomous run

`node scripts/plan-next.mjs` prints `STOP=awaiting-human`: **13 of 19 tasks done, 0
blocked**. The full commit-by-commit history is `CHANGELOG.md`; the machine's end state
is the status line in `PRODUCTION.md` §10.

- **Shipped by the loop:** P0-1 delivery confirmation · P0-2 restore drill · P0-3 key
  custody · P0-5 chain alarm · P1-1 `--json` everywhere · P1-2 incident runbook · P1-4
  release v0.1.0 · P1-5 DSAR drill · P1-6 state versioning · P2-1 proptest · P2-2
  evidence pack · P2-3 accessibility · P2-5 dependency updates.
- **Left for a person:** P0-4 access review (sign) · P0-6 purge rehearsal (Legal) ·
  P0-7 the 13 checklist items · P1-3 escalation SLA proof · P2-4 branch protection ·
  P2-6 the design-gate decision. `plan-done.mjs` refuses to close any of them without
  `--signed-by` and a real evidence file, which is the point.

## 3. Repository map

| Path | Responsibility |
| --- | --- |
| `src/main.rs` | CLI (clap); the only place RBAC and the workspace lock are applied |
| `src/store.rs`, `src/jsonl.rs` | workspace directory, atomic writes, `flock`, `0600`/`0700`, append-only logs, torn-tail detection |
| `src/digest.rs`, `src/log.rs`, `src/delivery.rs` | canonical encoder, SHA-256, hash chain, schema version, monotonic verify, the chained delivery confirmation |
| `src/disposition.rs`, `src/gate.rs`, `src/requisition.rs` | locked form, five versioned knockouts with fact dependencies, paragraph lint |
| `src/pipeline.rs`, `src/qa.rs`, `src/ats.rs` | three circuit-breakers, second-person sampling, CSV import + hard filter |
| `src/identity.rs`, `src/approval.rs`, `src/verify.rs` | Ed25519 operators, hash-bound sign-off and drift, workspace verification |
| `src/metrics.rs`, `src/review.rs`, `src/policy.rs` | five pre-registered bands, routing consequences, retention + SLA |
| `src/retention.rs`, `src/migrate.rs`, `src/anchor.rs` | purge with tombstones, snapshot migration, off-host chain-head receipt |
| `src/eml.rs`, `src/operator.rs`, `src/redact.rs` | RFC 5322 drafts + manifest, audit log, `#hash` console redaction |
| `src/ui/`, `src/assets.rs` | optional loopback server and its embedded CSS/JS |
| `src/demo.rs` | deterministic fixture: 151 rows → 150 roster, 147 sends, 3 blocked |
| `web/` | the preview (see `web/README.md`); `public/product/*.jpg` are its captures |
| `verify_log.py` | stdlib-only independent verifier (chain, digests, time order) |
| `FORMAT_SPEC.md`, `THREAT_MODEL.md` | normative bytes; what the chain does and does not prove |
| `tests/`, `fuzz/`, `coverage-floors.toml`, `deny.toml` | golden fixtures, fuzz targets, per-module coverage floors, supply chain |
| `scripts/` | plan runners and `check-docs.mjs`; `restore-drill.sh`, `verify-chain.sh`, `dsar-drill.sh`, `evidence-pack.sh` |
| `docs/` | key custody, monitoring, incident runbook, DSAR answer, evidence pack, restore drill |
| `.github/workflows/` | `ci` (push/PR + weekly mutants, fuzz), `release` (tag), `verify` (daily), `update` (weekly) |

## 4. How a rejection is recorded

1. **Requisition facts** documented before posting; a gate without its facts is
   unavailable, so capture falls back to `FIELD`.
2. **Copy signed** by hiring manager + Legal, bound to the copy hash (an edit makes the
   approval **stale**, never silently inherited).
3. **Disposition captured** on one locked form: `GATE`, `FIELD` or `ADMIN`, no free
   text; corrections supersede, never overwrite.
4. **Second-person QA**: 100% of `GATE`, deterministic 10% of `FIELD`/`ADMIN`.
5. **Render then chain**: RFC 5322 draft, one chained entry per send, recorded
   `queued` — never `sent` without a human confirmation, which is itself a new
   chained `DELIVERY_CONFIRMED` entry in `delivery.jsonl`, never an edit of the send
   entry.

Breakers: completeness, gate consistency (never auto-corrected), version resolution.
Exit codes: `0` ok · `1` internal/IO · `2` blocked · `3` invalid state (incl. torn
tail) · `4` lock contention.

## 5. Decisions that must not be reversed

1. **The preview is not the ledger.** No browser action is presented as a signature, a
   signed approval or a verified chain; every screen says so.
2. **One brand, one system.** `web/src/components/brand-mark.tsx` is the only lockup;
   the landing and the workspace share the sans, the 70px bar, the 10px uppercase
   micro-label, the 7px radius and the shadcn buttons. When a landing style and a
   workspace style disagree, the workspace wins.
3. **Canonical layout is not covered by the digest.** Sorted keys, integers only, NFC,
   epoch seconds, 21-field allowlist; reordering keys verifies, editing a value fails.
4. **Signatures live outside the digest.** `verify_log.py` says "signatures not
   checked"; `ledger verify-signatures` does that in Rust.
5. **Every sign-off is bound to the copy hash it approved**; drift is reportable and
   re-approval appends.
6. **Four verdicts:** `Valid | Stale | Unsigned | Invalid` — forgery must never read as
   "unsigned".
7. **Migrations never rewrite chained logs.** Snapshot, then migrate unchained state.
8. **Purge tombstones dispositions and rosters only.** Send-log PII is inside the chain:
   erasure means deleting the workspace at end of retention.
9. **The build has one target.** `vite.config.ts` sets Nitro's `defaultPreset` to
   `vercel`, so `bun run build` writes `.vercel/output` unaided; `NITRO_PRESET` only
   overrides it for another target.
10. **The web app has no auth and must not acquire one** until it has persistence, real
    identities and a reason — see `PRODUCTION.md`.
11. **macOS and Linux only**; no Windows binary is shipped because none has been run.
12. **`hyper` is not banned; every outbound client is.** No socket in the dispatch path.

## 6. Invariants, and the tests that hold them

| Invariant | Enforced by |
| --- | --- |
| Three dispositions, no free text | `Disposition` enum, `validate_against` |
| A gate must be a requisition knockout, and its answer must be offered | `Disposition::validate_against` |
| A gate without documented facts cannot be captured | `Error::GateUnavailable` |
| FIELD requires the comparative paragraph; GATE emails require 100% QA | `validate_against`, `qa::requires_second_person` |
| Records are locked; corrections supersede | `DispositionRecord.locked`, `record`/`admit` |
| A rejected batch writes nothing | `record_batch` validates before `append_batch` |
| Editing or truncating the log is detected | `verify_chain` |
| A torn tail refuses, never repairs | `AppendLog::read_all` |
| Capture precedes send; sends never go backwards | `verify_chain` + golden fixtures |
| Sign-offs cannot be inherited by edited copy | `content_hash` + `Approval::is_bound_to` |
| A delivery status changes only by appending a confirmation | `DeliveryLog::confirm`, `effective_status` |
| Roles gate commands and requests | `Role::permits` at CLI and HTTP |
| One writer per workspace | `Store::lock` (flock) |
| The fixture is reproducible | fixed clock, derived keys → `e0a91f66…`, head `d347a639…` |
| Independent verification agrees | `verify_log.py` vs the Rust chain |
| A state document from a newer build fails closed | `Store::load`, `migrate::inspect` |
| Every command emits one JSON object under `--json` | global flag + `tests/cli_json.rs` |
| The skip link is the first tab stop; phone controls are 44px | `accessibility.test.tsx` + the checker |

## 7. Running and shipping

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked                 # 155
cargo test --locked --features ui   # 163
cargo build --release --features ui # build the ui feature last: plain build drops it

./target/release/ledger demo seed --data /tmp/pilot --force --record
./verify_log.py /tmp/pilot/send_log.jsonl          # OK, 147 entries
./target/release/ledger --operator <name> ui --port 8420   # loopback workspace

bash scripts/restore-drill.sh                # backup, restore, verify, tamper control
bash scripts/verify-chain.sh --data /tmp/pilot   # the alarm, by hand
bash scripts/dsar-drill.sh                   # the data-subject answer on the fixture
bash scripts/evidence-pack.sh                # evidence/<date>/ + index.json with digests
node scripts/check-docs.mjs                  # every commit logged, every quoted number true
```

Web:

```bash
cd web && bun install --frozen-lockfile
bun run lint && bun run test   # 0 errors, 11 tests
bun run build                  # Nitro preset vercel → .vercel/output
```

Deploy is a push to `main`: Vercel builds `web/` (Root Directory `web`, install
`bun install --frozen-lockfile`, build `bun run build`). No Rust at deploy time.

## 8. Gotchas that have bitten

| Symptom | Cause | Fix |
| --- | --- | --- |
| `invalid operator: register an operator first` | every mutation needs an identity | register an **approver** first |
| `operator X (coordinator) may not run 'operator'` | the first registered operator becomes the acting identity | pass `--operator "M. Okafor"` or set `LEDGER_OPERATOR` |
| `unknown answer "held"` | KO answers are a fixed vocabulary | `not-held`, `shorter-than-role`, `restricted`, `not-registered`, `expired`, `outside-required-location` |
| `no ATS export imported` | dispatch refuses an empty roster | `ats import` first (`examples/export.csv`) |
| `E0658: let expressions … unstable` on 1.85 | let-chains need 1.88 | nested `if let` |
| `ledger ui` missing after a build | `ui` feature off | rebuild with `--features ui` |
| landing button labels invisible | `.lp a { color: inherit }` (0,1,1) beat the button utility (0,1,0) | never set colour on `.lp a`; let utilities own it |
| plates show a stale UI | screenshots are committed | re-shoot `web/public/product/*.jpg` when the shell changes |
| `bun run lint` hangs | eslint walking `.vercel/output` | keep build dirs in `eslint.config.js` ignores |
| favicon 404 | icon deleted | `public/favicon.svg`, `/favicon.ico` 308s to it |
| release job fails with `configuration 'macos-14-arm64…' is not supported` | the runner label is unavailable for this repo | `macos-15` (arm64) and `macos-15-intel` (x86_64) |
| `unexpected argument '--json'` | `--json` is global, not per subcommand | put it after any command: `ledger gate list --json` |
| `test -s evidence/index.json` fails | the pack has never run; `evidence/` is git-ignored | `bash scripts/evidence-pack.sh` |
| checker: “focus lands on something you cannot see” | an off-canvas drawer still participates in the tab order | `visibility: hidden` when closed, switched immediately (no delayed transition) |
| checker: `focus-ring` FAIL on `outline-none` utilities | Tailwind's utility removes the outline and the checker cannot see a replacement | do not use `focus*:outline-none`; define one `:focus-visible` ring in `styles.css` |

## 9. Working with an agent on this repo

- Read `README.md` for the user-facing story, `PRODUCTION.md` for what is missing, and
  `CHANGELOG.md` before proposing a change that looks new — it may have been removed on
  purpose (the artefact site, the pre-built site deploy, the seven spec documents).
- Do not reintroduce a generator, a `site/` directory, committed HTML, or a second
  design system.
- Every user-visible claim must be checkable: a number from the fixture, a file name, an
  exit code, or a command whose output is in the doc. No invented statistics.
- When a check fails, fix the cause; never edit `assets/`-style kit files, never weaken
  `deny.toml`, `clippy.toml` or coverage floors to pass.
- Commit style: imperative subject, body that says what was broken and how it was
  verified. Push to `main` deploys production, so run `cargo test` and `bun run build`
  first.

## 10. The production loop

`PRODUCTION.md` is executable: a 19-task backlog in `.plan/tasks.json`, one shell gate
per task, state in `.plan/state.json` written only by `scripts/plan-done.mjs`.

```bash
node scripts/plan-next.mjs      # TASK=<id> and its GATE, or STOP=<reason>
node scripts/plan-done.mjs <id> # runs the gate; refuses to mark a failed task done
node scripts/plan-status.mjs    # dashboard, dependencies included
node scripts/check-docs.mjs     # CI: every commit logged, every quoted number true
```

Rules: one task, one commit (`<id>:` prefix), push, verify CI. A task of class `human`
or `human-gated` cannot be closed without `--signed-by "<person>"` and its evidence
file — never write evidence on someone's behalf. Two failed attempts block a task; three
blocked in a row ends the run. `STOP=awaiting-human` is the correct machine end state:
what remains is signatures, a mailbox and a lawyer.

**State:** 13/19 done, 0 blocked, 6 awaiting a person (P0-4, P0-6, P0-7, P1-3, P2-4,
P2-6). Do not invent work to keep the loop alive; when `plan-next.mjs` says
`STOP=awaiting-human`, the next commit is a signature, not a script.

## 11. Where everything is written down

| Question | File |
| --- | --- |
| What is it, how do I run it? | `README.md` |
| What happened, in order? | `CHANGELOG.md` |
| What is missing before a real pilot? | `PRODUCTION.md` + `.plan/tasks.json` |
| What should I do next, automatically? | `node scripts/plan-next.mjs` |
| What do the bytes mean? | `FORMAT_SPEC.md` |
| What does the chain *not* prove? | `THREAT_MODEL.md` |
| How do I change it safely? | `CONTRIBUTING.md` |
| How does the preview work? | `web/README.md` |
| How do I restore a backup? | `docs/restore-drill.md` |
| Who holds which key, and what does rotation cost? | `docs/key-custody.md` |
| The alarm fired at 2am — what now? | `docs/incident-runbook.md` |
| Who is paged, and what does the daily job check? | `docs/monitoring.md` |
| What do I answer on a data-subject request? | `docs/dsar.md` |
| What goes to the auditor, and how is it read? | `docs/evidence.md` |
