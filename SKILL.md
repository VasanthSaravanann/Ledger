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
History lives in `CHANGELOG.md`; what is left before a real pilot lives in
`PRODUCTION.md`; how a stranger reproduces it lives in `README.md`.

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

- Rust 2024, MSRV **1.85** (compiled in CI), 11 direct crates, **no network crates** in
  the default graph. Optional `ui` feature adds axum + tokio.
- 34 modules, ~15.7k lines, **155 tests** (163 with `--features ui`).
- 26 top-level CLI commands, `--json` on every one (a single global flag).
- Web: 6 routes, 6 tests, eslint 0 errors (7 known `react-refresh` warnings).
- Deployed at **https://ledger-pilot-chi.vercel.app** from `web/` on every push to
  `main`; CI runs `test`, `msrv`, `web`, `supply-chain`, `coverage`, plus weekly
  `mutants` and `fuzz`.
- The pilot checklist is **0/13 signed** — every legal act is still human and undone.

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

## 7. Running and shipping

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked                 # 136
cargo test --locked --features ui   # 144
cargo build --release --features ui # build the ui feature last: plain build drops it

./target/release/ledger demo seed --data /tmp/pilot --force --record
./verify_log.py /tmp/pilot/send_log.jsonl          # OK, 147 entries
./target/release/ledger --operator <name> ui --port 8420   # loopback workspace
```

Web:

```bash
cd web && bun install --frozen-lockfile
bun run lint && bun run test   # 0 errors, 6 tests
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
