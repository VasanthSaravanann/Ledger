---
name: ledger-pilot
description: >-
  Build, change, review or operate Ledger — the audit-grade candidate feedback
  pilot in Rust at ~/Desktop/Ledger (CLI + optional loopback `ledger ui` server;
  no published site). Trigger whenever a task touches this repo or its concepts:
  plan.md, V2.md, V3.md, V4.md, V5.md, Web.md, website.md, dispositions
  (GATE/FIELD/ADMIN), gate copy (GATE-01..05), comparative paragraph, templates
  A/B/C, reply playbook, ATS CSV import, QA sampling, send log / hash chain /
  canonical digest / schema_version, signatures / operators / RBAC / sign-off
  drift, metrics and go-no-go routing, or the demo fixture. Also use it when the
  user says "the pilot", "feedback pilot", "disposition", "gate blocking",
  "immutable log", "verify-signatures", "ledger ui", or names any V-task id.
---

# Ledger — project skill

One file that carries everything an agent needs to work on this repository: what
the product is, what the seven specification documents demanded, what was actually
built and decided across the whole build conversation, the invariants that must not
break, and how to run and test it.

The specification documents are no longer in the tree; they remain readable at
`git show 90dfcb6:V2.md` (likewise `plan.md`, `V3.md`, `V4.md`, `V5.md`, `Web.md`,
`website.md`). This file compresses them, keeps the decisions that were made while
executing them, and records what is still open.

---

## 1. What this is

**Ledger** is operational tooling for a *manual* candidate-feedback pilot: one
employer, one requisition, initial screen only, 100% of rejected candidates. Every
rejection carries a reason a named human recorded, copy that Legal signed, and an
append-only log that proves the exact text the candidate received.

Non-goals, by design (do not "helpfully" add them):

- No LLM or model anywhere in the send path. A person selects; the tool renders.
- No email sending. It renders RFC 5322 drafts, records them `queued`, and leaves
  delivery to a mail client.
- No ATS API. One CSV in, drafts out.
- No hosted multi-tenant service. The workspace is a directory the team holds.
- No Windows binary. macOS and Linux only, and that is stated publicly.
- **No generated site.** No static site generator, no committed HTML: the `web/`
  `web/` preview is the only published surface, and the loopback-only `ledger ui`
  server is the only one the Rust code runs.
- The pilot's *legal* acts (counsel's privacy opinion, a real monitored mailbox,
  HM/Legal signatures, the real requisition) are human; the tool records them, it
  cannot perform them. Never claim a pilot is "live" because the code is ready.

Stack: Rust 2024, MSRV **1.85**, 11 direct crates, zero networking crates in the
default graph. Optional `ui` feature adds axum + tokio. 16k lines across 33 modules,
136 tests (144 with `--features ui`).

---

## 2. Repository map

| Path | Responsibility |
| --- | --- |
| `src/main.rs` | CLI (clap). 25 top-level commands; the only place RBAC and the workspace lock are applied to commands. |
| `src/lib.rs` | 30 `pub mod`; no `cli.rs` — the binary is `main.rs`. |
| `src/store.rs` | Workspace directory, atomic JSON writes, `flock`, `0600`/`0700`, `Store::load/save/lock`. |
| `src/jsonl.rs` | `AppendLog<T>`: atomic line append, `append_batch`, fsync file+dir, torn-tail detection. |
| `src/digest.rs` | Canonical encoder (`CanonicalValue`), NFC, epoch seconds, SHA-256. Normative per `FORMAT_SPEC.md`. |
| `src/log.rs` | `SendEntry`, hash chain, `SendLog`, `BlockedLog`, `DeliveryStatus`, schema version, monotonic verify. |
| `src/disposition.rs` | `Disposition` (GATE/FIELD/ADMIN), `NewDisposition`, `Dispositions` (head/history/record/`record_batch`), signatures. |
| `src/gate.rs` | `GateKind` GATE-01..05, `FactKey`, `GateCopy` (versioned), `GateLibrary`, `content_hash`. |
| `src/requisition.rs` | `Requisition`, `RequisitionDraft`, documented facts, `Comparative` + lint, approvals, `blocking_issues`. |
| `src/template.rs` | Templates A/B/C, slot allowlist, `render`, `FEEDBACK_DECLINE`, `merge`/`slot_value`. |
| `src/playbook.rs` | Five reply scripts, escalation, approval. |
| `src/ats.rs` | CSV import, `KO_ANSWER_01..05` → gates, hard initial-screen filter. |
| `src/qa.rs` | Second-person QA: 100% GATE + deterministic 10% sample keyed on `(REQ, APP)`. |
| `src/pipeline.rs` | `dispatch`, three circuit-breakers, QA gate, preview/execute, abort hook for tests. |
| `src/metrics.rs` | Replies, value signals, five metrics with pre-registered bands. |
| `src/review.rs` | Criteria feasibility, routing consequences in V2 order. |
| `src/policy.rs` | Retention days (default 2557), escalation owner, SLA hours (24). |
| `src/retention.rs` | `purge` (tombstones dispositions + rosters; keeps send log), `preview`. |
| `src/migrate.rs` | Schema inspection, snapshot to `backups/`, never rewrites chained logs. |
| `src/anchor.rs` | Chain-head receipt in `anchors/`, optional local `git commit`, no network. |
| `src/operator.rs` | `operator.jsonl` audit log, `quarantine`, identity resolution helpers. |
| `src/identity.rs` | `Role`, `Operator`, `Registry` (Ed25519), `Signed`, `active`, keyfiles at `0600`. |
| `src/verify.rs` | `verify_workspace` → `VerificationReport` (dispositions + 5 sign-off sites). |
| `src/approval.rs` | `SignOff` (actor/role/at/copy hash/signature), `Approval`, `drift`, `ArtefactStatus`. |
| `src/checklist.rs` | The 13 plan tasks, `TASKS`, `Checklist` (`checklist.json`). |
| `src/demo.rs` | Deterministic fixture, five planted failure cases, fingerprint. |
| `src/eml.rs` | RFC 5322 draft batch + `manifest.json` + `verify_batch`. |
| `src/assets.rs` | `include_str!` of `src/ui/assets/{tokens.css,app.css,app.js}` for the server. |
| `src/ui/` | `ledger ui` axum server (feature-gated), assets, HTTP tests in `tests/ui.rs`. |
| `src/redact.rs` | `#hash` handles for ids/emails in console output; `--debug-ids`. |
| `verify_log.py` | Stdlib-only independent verifier (chain + digests + monotonic time). |
| `FORMAT_SPEC.md`, `THREAT_MODEL.md` | Normative byte format; and what the chain does/does not prove. |
| `scripts/demo-walkthrough.sh`, `scripts/coverage-floors.sh` | Scripted demo; per-module coverage gate. |
| `.github/workflows/ci.yml` | test · msrv · supply-chain · coverage · web (+ weekly mutants, nightly fuzz). |
| `web/` | Workspace preview: TanStack Start, session-only sample data, deployed to Vercel. |

---

## 3. The specification lineage

Seven documents, each superseding the last in ambition. Read them for prose; this is
the task inventory and where each landed.

### 3.1 `plan.md` — v1 manual pilot (13 tasks)

Scope: one employer, one 150+ requisition, initial screen, 100% of rejected
candidates, no ATS API.

| Phase | Tasks |
| --- | --- |
| 1 Governance | 1.1 APP 1.7–1.9 privacy check · 1.2 HM+Legal approval workflow · 1.3 monitored reply mailbox |
| 2 Artefacts | 2.1 locked single-screen disposition form · 2.2 Tier 0 gate copy · 2.3 role-level comparative paragraph · 2.4 three master templates |
| 3 Pipeline | 3.1 ATS CSV export + mapping · 3.2 immutable send log · 3.3 gate-blocking protocol |
| 4 Pilot | 4.1 execute · 4.2 three v2 metrics · 4.3 go/no-go review |

Built as commit `d6bf2fa`. Everything except the human acts (counsel, mailbox,
signatures, real requisition) has a code path; the checklist is still 0/13 because
no pilot has been run.

### 3.2 `V2.md` — audit-grade v2 (12 tasks)

Adds the parts that make evidence defensible. All implemented in `acbff64`.

- 1.1 retention sign-off → `policy.retention_days`; 1.2 monitored mailbox + escalation owner/SLA → `Mailbox` refuses `no-reply@`, `policy` holds owner + hours; 1.3 single-pass sign-off across five artefacts → `approval.rs`.
- 2.1 form with `DECISION_MAKER_ID`, `LOCKED`, `SUPERSEDES` → `disposition.rs` (append-only; corrections supersede, never overwrite).
- 2.2 versioned `GATE-01..05` with `TEXT_VERSION` and fact dependencies → `gate.rs`; a gate missing facts is **unavailable** and capture must fall back to `FIELD`.
- 2.3 comparative paragraph elicited into five slots, rejected at sign-off by a lint (relative superiority, implied deficit, subjective proxy, years bar) → `requisition.rs`.
- 2.4 templates A/B/C, slot allowlist, fixed feedback-decline + re-engagement lines, three one-click value signals → `template.rs`.
- 2.5 five-scenario reply playbook with ER/Legal escalation → `playbook.rs`.
- 3.1 hard initial-screen filter, `KO_ANSWER_0n` mapping → `ats.rs`; 3.2 every V2 send-log column → `log.rs`; 3.3 three circuit-breakers → `pipeline.rs`.
- 4.1 QA runbook (100% GATE, deterministic 10%) → `qa.rs`; 4.2 five metrics with bands → `metrics.rs`; 4.3 six routing consequences → `review.rs`.

### 3.3 `V3.md` — demo fixture (9 tasks; the site half was later removed)

- 0.1 one correct run block, 0.2 `work-rights-duration` everywhere, 0.3 synthetic-fixture disclosure.
- 1.1 `ledger demo seed [--force] [--record]`; 1.2 five planted failure cases; 1.3 determinism test.
- 2.1–2.6 and 3.1–3.2 built the artefact site and its build wiring. **Removed:** `src/site.rs`,
  `--export-html`, `site/`, `vercel.json`, `scripts/build-site.sh` and the CI drift check
  are gone; only the fixture (1.x) remains, which is what the tests and demo need.

### 3.4 `V4.md` — hardening (30 tasks, 8 priorities)

1. Crash safety: atomic append + fsync + dir fsync + torn-tail detection, atomic state writes, `flock`, `log quarantine`, interrupted-dispatch test.
2. Input/injection/filesystem: single-pass renderer, import-time validation, HTML escaping (incl. `'`), CSV edge cases, path/perm hardening.
3. Boundaries: zero/NaN guards, centralised band tests, checked arithmetic, stable determinism key.
4. Chain canonicalisation: canonical bytes, genesis + monotonic time, mutation/schema tests, `schema_version` + golden fixtures + `migrate`, MSRV in CI.
5. Evidence-grade tests: coverage floors, mutants, proptest, fuzz, snapshot/trybuild, unsafe ban.
6. Lints/CLI/audit: no panic paths, exit codes + `--json`, PII-safe errors, operator log + completions.
7. Supply chain: `cargo-deny`, reproducible releases, CI drift check, timezone/SLA tests, backup/restore.
8. Independent verifiability: `verify_log.py`, `FORMAT_SPEC.md`, `THREAT_MODEL.md`.

Built in `3a7a1b9`. Delivered: 1.1–1.5, 2.1–2.5, 3.1–3.4, 4.1–4.5 (spec+impl+goldens), 5.1/5.2/5.4 (wired, scoped), 5.5 unsafe ban, 6.1/6.2 (exit codes, partial `--json`), 6.4 (audit log), 7.1/7.3/7.4/7.5, 8.1/8.2. Not delivered: 5.5 typestate (deliberately deferred; the public claim was softened to "enforced in code and logs"), 5.3 proptest, 4.4 state-document versioning.

### 3.5 `V5.md` — operational & user-ready (20 tasks, 5 phases)

1. Unblock CI (MSRV, `0600`, chrono advisory, coverage floors, PII backtraces, LICENSE + release).
2. Audit trail, retention (`purge`), `migrate`, `--json` everywhere, `anchor`.
3. Ed25519 identities, RBAC, pre-mutation backups, sign-off drift.
4. Recruiter web UI (axum): triage, bulk, mapper, `.eml`/zip, in-UI checklist.
5. Public site (later removed): build command + provenance, specs/verifier pages, 404/robots/sitemap/OG, wasm demo.

Built across `24145c9`, `16a1caa`, `3f880a2`, `7c5326c`. Phase 5.3 (wasm) deferred.
Phase 4 delivered its foundation, not all screens (see §10).

### 3.6 `Web.md` — read-only reconnaissance

Ten questions answered before any UI work: exact `[bans]`, absence of tokio,
public API surface, schemas, gate/comparative structs, template/eml signatures,
operator identity, checklist/sign-off state, offline policy, locking, and the
existing site CSS. It corrected two assumptions worth remembering: `src/operator.rs`
is the *audit log* (identity lives in `src/identity.rs`), and there is **no**
`Pending`/`Shortlisted` status type — that had to be derived, and `Shortlisted` does
not exist in the model at all.

### 3.7 `website.md` — Phase 4 execution plan

HR-SaaS design system, three core prerequisites (`record_batch`,
`Approval::artefact_status`, queue-state derivation), supply-chain/session/concurrency
architecture, five screens, and slices A–E. Executed with the nine defaults in §5;
slices A, B, C, D-core and E landed in `7c5326c` (+`65b6c22` fix).

---

## 4. What happened, in order

The build conversation, compressed. Commit refs are real; use `git show` for detail.

| # | Request | Outcome |
| --- | --- | --- |
| 1 | Clone `VasanthSaravanann/Ledger` into Desktop | Empty repo cloned; nothing to pull. |
| 2 | "build plan.md in rust" | v1: gate copy, 3 templates, locked form, hash-chained log, gate-blocking, 3 metrics, go/no-go (`d6bf2fa`). |
| 3 | Deploy to Vercel | Blocked on "no web surface": Rust CLI has no runtime. Added a static page + `vercel.json`, linked project, preview → production (`ec66e4a`). |
| 4 | Push | `main` pushed; git integration deploys. |
| 5 | "has the plan been executed?" | No: 0/13 signed; only a demo workspace existed. |
| 6 | Execute `V2.md` | Full v2 rewrite: GATE/FIELD/ADMIN, versioned gates + facts, paragraph lint, playbook, hard stage filter, QA sampling, five metrics, routing (`acbff64`). |
| 7 | Execute `V3.md` | Deterministic fixture with five failure cases, `--export-html` artefact pages, build-time wiring, site fixes (`7d8d9be`). |
| 8 | Analyse/execute `V4.md` | Found 4 contradictions + 2 CI/automation conflicts; user chose resolutions; implemented canonical digest v2, crash safety, quarantine, goldens, `verify_log.py`, CI (`3a7a1b9`). |
| 9 | "push retaining production" | Pushed; production alias verified. |
| 10 | List executed tasks/features | Status table delivered (all 13 V2 tasks mapped to code). |
| 11 | Analyse/execute `V5.md` | Found 6 contradictions; user adopted defaults; phases 1, 2, 5 delivered, CI turned green for the first time (`24145c9`), reproducible provenance (`bb8ee69`), deploy duplicated-workflow removed (`c7cc82d`). |
| 12 | "go" (phase 3) | Ed25519 operators, disposition signing, RBAC, `verify-signatures`, snapshots (`16a1caa`); then sign-off hash binding + drift + `record_batch` + `.eml` queued drafts (`3f880a2`). |
| 13 | "go" (phase 4) | Slices A–E: `record_batch`, `ArtefactStatus`, `verify_workspace`, `ui` feature gate, embedded assets, axum server with token→cookie + RBAC + flock, HTTP tests (`7c5326c`); fixed a CI-only EPIPE in the harness (`65b6c22`). |
| 14 | "the site isn't a product" | Product landing, interactive `/lab` (WebCrypto chain verification), real `ledger ui` snapshots at `/workspace` (`43c86bf`). |
| 15 | "review" | 10 findings from modern-web-guidance; all fixed: XSS sink, CSP, table semantics, a11y, motion (`f71d930`). |
| 16 | "make a skill file" | This document. |
| 17 | "remove the complete website and static html" | Deleted `site/`, `src/site.rs`, `--export-html`, `vercel.json`, `.vercelignore`, `scripts/build-site.sh` and the CI drift step; dropped `pulldown-cmark`. Rows 3, 4, 14 and 15 describe work that no longer exists in the tree. |
| 18 | "connect the exported workspace, push to main, new prod" | Moved the exported UI into `web/` (TanStack Start, preview), added `web/vercel.json`, pointed the Vercel project's Root Directory at `web`, built with the Nitro `vercel` preset, pushed to `main` and deployed production. |

Production is the `web/` preview at **https://ledger-pilot-chi.vercel.app** — sample
data only, not connected to the Rust ledger. The removed artefact pages are gone; do
not cite their URLs.

---

## 5. Decisions ledger (do not silently reverse these)

Each was chosen deliberately, often against the obvious alternative. The rationale is
the useful part.

### Identity, sign-off, RBAC

1. **One operator per process.** Identity is a name plus a `0600` keyfile, not an
   authenticated account, so `ledger ui` binds the operator at launch and switching is
   a relaunch. A per-request `operator=` parameter would be RBAC theatre.
2. **Signatures live outside the send-log digest.** The chain proves the log is
   unedited; signatures prove who decided. So `verify_log.py` reports "signatures not
   checked" and `ledger verify-signatures` does that in Rust.
3. **Every sign-off is bound to the copy hash it approved.** Editing a block makes the
   approval *stale* rather than silently inherited; the old sign-off is kept so the
   drift is reportable, and re-approval appends.
4. **Four verdicts, not three:** `Valid | Stale{reasons} | Unsigned | Invalid`. A
   signature that fails to verify is worse than a missing one; collapsing them shows
   forgery as "unsigned".
5. **Role matrix is enforced per CLI command and per HTTP request**, not in the
   library: `coordinator` (import/disposition/qa/dispatch/reply/signal/anchor/log),
   `approver` (gate/templates/playbook/requisition/governance/purge/migrate/checklist/
   operator/init/demo), `auditor` (read-only). A registry-less workspace is bootstrap:
   only `init`, `demo seed`, `operator`.

### Format and integrity

6. **Canonical digest v2, and layout is not covered.** Sorted keys, no whitespace,
   integers only, NFC strings, epoch-second timestamps, 21-field allowlist. Reordering
   keys verifies; editing a value fails. This is the trade that makes an independent
   verifier (and a browser) possible.
7. **`schema_version` sits outside the digest** so a version bump cannot invalidate
   earlier entries, while an unknown version still fails closed.
8. **Stored `DISPOSITION`/`GATE_CODE` are the exact codes the digest covers** (`GATE`,
   `GATE-01`). The old kebab spellings are accepted on read only.
9. **Timestamps are truncated to whole seconds on write**, so the digest covers the
   whole stored instant, and `captured_at <= sent_at` with non-decreasing `sent_at` is
   verified.
10. **Migrations never rewrite chained logs.** Snapshot, then migrate unchained state;
    fall back to refusing.
11. **Purge tombstones dispositions and ATS rosters only.** The send log's PII is
    inside the chain: erasure there means deleting the workspace at end of retention,
    not selective editing. Default retention 2557 days, SLA 24 h — both recorded.
12. **`append_batch` is dispositions; the chain is `SendLog::append`.** They are
    different logs with different guarantees; never conflate them in copy or code.

### CLI contract

13. **Exit codes**: `0` ok, `1` internal/IO, `2` circuit-breaker block, `3` invalid
    input/schema/state (incl. torn tail), `4` workspace lock contention. `--allow-blocked`
    opts out of `2`.
14. **Destructive commands require an explicit flag** (`purge --yes`, `migrate --yes`,
    `log quarantine --yes`, `demo seed --force`). Everything else runs non-interactively
    so scripts and CI work.
15. **No `RUST_BACKTRACE` manipulation.** Edition 2024 makes `set_var` unsafe and the
    crate forbids unsafe; the tool simply never prints a backtrace, and console output
    redacts ids/emails to `#hash` handles (`--debug-ids` reveals).
16. **`expect` is allowed at three const sites only** (`GateLibrary::seed`,
    `demo::base_time`, `digest::truncate_to_seconds`); `unwrap_used`, `panic` and
    `indexing_slicing` are denied crate-wide, tests exempt via `clippy.toml`.
17. **Coverage floors are per module and rebaselined from measurement**, not a global
    percentage; `CONTRIBUTING.md` says why each is where it is.

### Supply chain and platform

18. **MSRV 1.85 is compiled in CI.** Let-chains need 1.88 — twice a let-chain hid in
    code and broke the MSRV job. Do not use `if let … && …` until the MSRV moves.
19. **`hyper` is not banned; every outbound client is.** The `ui` feature adds a
    loopback server, so the claim is "no networking in the dispatch path", not "no
    networking crates". `reqwest`/`ureq`/`curl`/TLS/gRPC/QUIC/websockets stay banned.
20. **macOS and Linux only.** No Windows binary is shipped because none has been run.
21. **`chrono`'s unmaintained advisory is ignored with a reason and a review date**
    (2026-12-31), not silently; migrating to `jiff` is scheduled, not urgent.

### Web surface

22. **The public site is the `web/` app, and only that:** `/` is the landing page that
    explains the pilot (gate copy, chain, the five metric bands, what the tool refuses),
    shows real captures of the workspace (`web/public/product/`), and links into the
    preview at `/triage`. The plates are screenshots of the app, so re-shoot them with
    `node /tmp/shots.mjs`-style CDP capture when a screen changes; they are not generated
    by the Rust build. The page is a UI preview. The Rust artefact
    generator, the committed pages, `vercel.json` at the root and the drift check were
    removed together; what replaced them is a separate TanStack Start app that Vercel
    builds from `web/`. Do not reintroduce a generator, a `site/` directory or
    committed HTML.
23. **`web/` is a preview, not a ledger.** Session-only sample roster, no persistence,
    no signatures, no chain. Every screen says so. The next honest step is wiring it to
    the Rust core, not dressing the fixture up as evidence.
24. **One system, two surfaces.** `web/src/components/brand-mark.tsx` is the only brand
    lockup; the workspace sidebar and the landing bar both render it, at the same 30px
    tile and 24px word. Both surfaces share the sans stack, the 70px top bar with its
    hairline, the uppercase 10px micro-label, the 7px card radius and the shadcn
    `Button` set. The landing is bigger in scale (its h1 is 42px against the app's
    27px) and keeps the hanging margin; it is not a different design language. If a
    landing style and a workspace style disagree, the workspace wins.
25. **`ledger ui` is a loopback app, not a website.** It binds 127.0.0.1 behind the
    `ui` feature, serves HTML from `include_str!` assets, and has no external asset,
    no CDN and no build step. That is the only HTTP surface the Rust code serves.
24. **No CDN, no webfonts, system font stack** — air-gapped by construction, so the
    server works on a machine that has never had a network.

### Deferred on purpose

29. **Typestate for approved-vs-unapproved requisitions** — runtime checks plus tests
    are enough; the public wording was softened rather than the code refactored.
30. **wasm interactive demo** — needs a `ledger-core` split (the core pulls `store`/
    `std::fs`), not a target flag.
31. **proptest suites** — the three properties are covered deterministically; property
    generation is not yet wired.
32. **`--json` on all 25 commands** — 11 emit JSON (`demo seed`, `dispatch`, `metrics`,
    `log verify`, `log quarantine`, `review route`, `purge`, `migrate`, `anchor`,
    `verify-signatures`, `find`); the rest print prose.

---

## 6. Inconsistencies found, and how they were settled

When a spec contradicted itself or the code, the contradiction was surfaced before
execution. These are the resolutions in force.

| Where | Contradiction | Resolution |
| --- | --- | --- |
| V4 4.1 vs 4.3.1 | "canonical keys" vs "reordering keys must fail" | Canonical form covers values, not layout; reordering **verifies**, editing fails. Documented in `FORMAT_SPEC.md` §4. |
| V4 4.1 vs 5.3 | "verify passes iff bytes unmutated" | Restated: any change to a covered value fails; formatting changes do not. |
| V4 1.1 vs 1.5 | "refuse torn tails" vs "restart with and without quarantine gives 77 sends" | Without quarantine restart exits `3` and sends nothing; with quarantine it commits exactly 77. Both branches tested. |
| V4 4.4 vs 4.3.2 | "fail closed on unknown schema" vs "old entries still verify" | `schema_version` outside the digest; unknown version still refuses. |
| V4 6.2 vs CI | exit `2` on block aborts scripted flows | `--preview` always `0`; commit exits `2` unless `--allow-blocked`. |
| V4 6.3 vs automation | `--yes` on every mutation | Only destructive commands require it. |
| V5 4.1 vs deny.toml | axum needs hyper; hyper banned | hyper unbanned, claim narrowed, clients still banned. |
| V5 2.5 vs offline | webhook needs a network call | Receipt file + optional local git commit; webhook is a separate script. |
| V5 2.2 vs chain | purge PII but keep hashes | Tombstones for unchained records only; send log retained; whole-workspace erasure at end of retention. |
| V5 3.1 vs stdlib | verify Ed25519 in a stdlib-only script | Signatures verified in Rust; the Python verifier says so. |
| V5 1.5 vs unsafe ban | `set_var("RUST_BACKTRACE")` | Not set; the tool never prints backtraces. |
| V5 4.5 vs exactness | `.eml` + "mark dispatched" vs "exact text sent" | Drafts recorded `queued` with the file hash; no claim of delivery. |
| V5 4.3 vs append-only | "atomic bulk with rollback" | Validate-then-single-write; a rejected batch writes zero bytes (there is nothing to roll back). |
| website.md 2 vs model | "Pending/GATE/FIELD/ADMIN/Shortlisted" tabs | `Shortlisted` does not exist; queue state is derived from `head()`. |
| website.md 4 vs RBAC | `/audit` "all roles" + snapshot button | Snapshot is approver-only; auditors read. |
| Web.md 6 | `src/operator.rs` assumed to hold identity | Identity is `src/identity.rs`; `operator.rs` is the audit log. |

Also fixed during review: the lab built table rows with `innerHTML` from
operator-authored facts (stored XSS); the skip link was permanently clipped; body type
was in `px`, ignoring OS text scaling; smooth scroll and the bulk slide ignored
`prefers-reduced-motion`. Those fixes lived in `src/site.rs`/`lab.js` and went with the
site; `src/ui/assets/app.css` keeps the type-scale and motion rules.

---

## 7. Invariants, and where they are enforced

Break one of these and you have broken the product, not just a test.

| Invariant | Enforced by | Test |
| --- | --- | --- |
| Three dispositions only; no free text | `Disposition` enum | `disposition.rs` unit tests |
| A gate answer must be one the gate offers, and the gate must be a requisition knockout | `Disposition::validate_against` | `a_gate_the_requisition_does_not_screen_on_is_rejected` |
| A gate without documented facts cannot be captured | `Error::GateUnavailable` | `a_gate_without_requisition_facts_cannot_be_captured` |
| FIELD requires the paragraph to exist | `validate_against` | `demands` tests |
| Records are locked and corrections supersede | `DispositionRecord.locked`, `Dispositions::record`/`admit` | `capture_is_append_only_and_single_shot`, `a_batch_can_supersede_a_record_staged_in_the_same_batch` |
| A rejected batch writes nothing | `record_batch` validates before `append_batch` | `a_batch_with_one_bad_form_writes_nothing`, `a_rejected_batch_writes_nothing` (HTTP) |
| No send without a disposition | `pipeline::dispatch` | `dispatch_refuses_to_run_before_the_copy_is_signed_off` |
| A contradicting gate answer blocks and escalates | `BlockReason::AnswerMismatch` | `a_form_answer_that_contradicts_the_ats_answer_blocks_the_send` |
| GATE emails need second-person QA | `qa::requires_second_person` | `the_second_person_check_is_required_for_gate_emails` |
| The log stores exactly what was sent | `SendEntry.body` + `text_digest` | `chain_links_and_carries_every_required_column` |
| Editing/truncating the log is detected | `verify_chain` | `the_log_chain_detects_an_edited_email`, `a_truncated_log_is_detected` |
| A torn tail refuses, never repairs | `AppendLog::read_all` | `a_torn_trailing_line_is_detected_not_dropped`, `a_torn_tail_blocks_restart_until_it_is_quarantined` |
| Capture precedes send; sends never go backwards | `verify_chain` | golden + crash tests |
| Sign-offs cannot be inherited by edited copy | `content_hash` + `Approval::is_bound_to` | `editing_approved_copy_blocks_dispatch_until_reapproved` |
| Roles gate commands | `Role::permits` at CLI and HTTP | `roles_gate_the_commands_they_should`, `an_auditor_can_read_but_not_dispose` |
| One writer per workspace | `Store::lock` (flock) | `a_second_operator_cannot_hold_the_workspace`, `a_lock_held_elsewhere_is_a_conflict` |
| The fixture is reproducible | fixed clock, derived keys | `the_seed_is_byte_identical_across_runs` |
| Independent verification agrees | `verify_log.py` | `the_python_verifier_agrees_with_the_rust_chain` |

---

## 8. Working on it

### Everyday commands

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings   # includes the UI
cargo clippy --all-targets -- -D warnings                  # proves core builds without ui
cargo test --locked
cargo test --locked --features ui                          # 8 extra HTTP tests
cargo build --release && cargo build --release --features ui
```

`cargo build` (no feature) overwrites `target/debug/ledger` with a binary that has no
`ui` subcommand — build with `--features ui` last when you intend to run the server.

### Try the product locally

```bash
./target/release/ledger demo seed --data /tmp/pilot --force --record
./target/release/ledger status --data /tmp/pilot
./target/release/ledger verify-signatures --data /tmp/pilot
./verify_log.py /tmp/pilot/send_log.jsonl
./target/release/ledger --data /tmp/pilot --operator rec-rui ui --port 8420
```

Fixture facts: 151 CSV rows → 150 roster (1 dropped), 12 GATE / 129 FIELD / 8 ADMIN,
1 missing disposition, 147 sends, 3 blocked (mismatch, missing QA, missing
disposition), 1 stale `TEXT_VERSION`, fingerprint `64f9e1f9…`, head `d347a639…`.

### Regenerating the fixture

```bash
rm -rf /tmp/pilot && ./target/release/ledger demo seed --data /tmp/pilot --force --record
```

Two seeds in two directories must produce the same fingerprint and log head; the test
suite asserts it. There is no build step to regenerate and nothing to commit.

### Working on the `web/` site

```bash
cd web && bun install --frozen-lockfile
bun run dev                        # landing at /, workspace at /triage
bun run lint                       # eslint + prettier; ignores .vercel/output or it hangs
bun run test                       # vitest (6 tests)
NITRO_PRESET=vercel bun run build  # what Vercel runs; writes .vercel/output
```

The Vercel project's Root Directory is `web`, so a push to `main` deploys it and no
`vercel.json` is read from the repo root. `vite.config.ts` sets Nitro's default preset
to `vercel`, so `bun run build` writes `.vercel/output` unaided; `NITRO_PRESET` overrides
it for any other target.

### CI

`ci.yml`: `test` (fmt, clippy all-features, clippy no-features, tests, ui tests),
`msrv` (1.85 compile), `supply-chain` (`cargo-deny`), `coverage` (per-module floors),
`web` (bun install, vitest, Vercel build), plus scheduled `mutants`
(digest/log/pipeline/metrics/qa/template) and `fuzz` (`parse_csv`, `parse_log`).

### Format changes

`FORMAT_SPEC.md` is normative. A digest change is a `digest_version` bump: update the
spec, recompute the `tests/golden.rs` constants (`LOG_HEAD`, `SEND_LOG_SHA256`,
`DISPOSITIONS_SHA256`, `FINGERPRINT`) from a fresh fixture, and re-run `verify_log.py`
in the same commit.

---

## 9. Failure modes that actually bit (and their fixes)

| Symptom | Cause | Fix |
| --- | --- | --- |
| `E0658: let expressions in this position are unstable` on 1.85 | `if let … && …` let-chain (needs 1.88) | Nested `if let`; check the MSRV job before pushing |
| UI tests pass locally, `ConnectionReset` in CI | Test harness dropped the child's stdout after reading the banner; the server's next `println!` hit EPIPE and panicked on Linux | Hold the `BufReader` in the `Server` struct |
| `expected ',', found '"'` inside a `format!` raw string | HTML contained `"#` (e.g. `href="#content"`), terminating `r#"…"#` | Use single quotes in that attribute |
| `cargo deny` passes but `--all-features` clippy breaks bans | cargo-deny is feature-sensitive | Decide which graph you mean; keep a no-feature clippy leg |
| Sign-offs reported both invalid and stale | Signature checked against the *current* hash instead of the recorded one | Verify against `artefact_hash`; staleness is a separate check |
| Category mismatch between stored JSON and digest | `serde(rename_all)` stored kebab slugs while the digest used codes | Store the codes; accept legacy on read |
| Fixture rejected by `captured_at <= sent_at` | Fixture clock was in the future relative to the machine | Base time is a past instant (`2026-11-02T09:00Z`) |
| `cargo build` without the feature removed `ledger ui` | Feature-gated subcommand | Rebuild with `--features ui` before running the server |
| Inserting a nav link shifted `format!` placeholders | Positional `{}` arguments | Use named arguments when the markup grows |

---

## 10. Current state and open work

Done: v1, V2, V3 (fixture only), V4 (all but three items below), V5 phases 1, 2 and 5,
V5 phase 3 (identities, signing, RBAC, drift, backups), Phase 4 slices A–C and the core
of D/E. Removed on request: the static artefact site and its generator. Added in its
place: `web/`, the TanStack Start preview, live on Vercel.

Open, with the obstacle named:

1. **Phase 4.4 ATS column mapper** — needs a `MappingProfile` model plus storage
   (`mappings.json` is already reserved in `store.rs`); `Roster::from_csv` requires exact
   V2 headers today.
2. **Phase 4.5 `.eml`/zip dispatch screen** — `eml::write_batch` exists; the `zip` crate
   is deliberately not yet a dependency.
3. **Phase 4.6 one-click re-approval** — `artefact_status` is displayed; the POST
   handler is not written.
4. **Delivery confirmation** — nothing moves `queued` → `sent`; needs a chained
   confirmation event before the "confirmed Sent" KPI can be non-zero.
5. **`--json` on all 25 commands** — 11 emit JSON.
6. **Typestate (V4 5.5)** — deferred; do not refactor for the slogan.
7. **Wasm demo (V5 5.3)** — needs the `ledger-core` split.
8. **Proptest (V4 5.3)** — not written.
9. **State-document `schema_version` (V4 4.4)** — only the send log is versioned.
10. **No published release** — `release.yml` exists on tags but no tag has been cut.

---

## 11. Quick reference

### Commands (25; `ui` only with `--features ui`)

Top level (25): `init` `demo` `status` `governance` `gate` `requisition` `templates`
`playbook` `ats` `disposition` `qa` `dispatch` `log` `reply` `signal` `metrics` `review`
`checklist` `purge` `migrate` `anchor` `find` `operator` `verify-signatures` `ui`.
Named subcommands used in this document: `demo seed`, `ats import`, `log verify`,
`log quarantine`, `log export`, `review criteria`, `review route`.

### Exit codes

`0` ok · `1` internal/IO · `2` blocked (use `--allow-blocked`) · `3` invalid input/schema/
state, incl. torn tail · `4` lock contention.

### RBAC

| Role | Mutating commands |
| --- | --- |
| `coordinator` | `ats import`, `disposition`, `qa`, `dispatch`, `reply`, `signal`, `anchor`, `log`, `init` |
| `approver` | `gate`, `templates`, `playbook`, `requisition`, `governance`, `purge`, `migrate`, `checklist`, `operator`, `init`, `demo seed` |
| `auditor` | none; read-only is open to all |

### Metric bands (pre-registered, not configurable)

| Metric | Green | Amber | Red |
| --- | --- | --- | --- |
| Reply volume / 1,000 sends | < 30 | 30–80 | > 80 |
| Form-block rate | < 10% | 10–25% | > 25% |
| Gate-mismatch rate | < 2% | 2–10% | > 10% |
| Candidate value ("Yes" share) | > 50% | 30–50% | < 30% |
| Criteria feasibility | agreed, 1 session | agreed, external input | not agreed |

### Routing (applied in this order)

1. candidate value red → **stop**
2. gate mismatch red → **flag discrimination exposure**
3. form-block red → **capture-discipline middleware**
4. form-block green + criteria agreed → **reassess venture viability**
5. reply volume red → **scope reply triage**
6. criteria not agreed → **pivot to criteria validation**

### Workspace files

`gate_library.json` · `templates.json` · `playbook.json` · `requisitions.json` ·
`policy.json` · `operators.json` + `operators/<name>.key` (0600) · `roster-<REQ>.json` ·
`dispositions.jsonl` · `qa.jsonl` · `send_log.jsonl` (chained) · `blocked.jsonl` ·
`replies.jsonl` · `signals.jsonl` · `review.json` · `checklist.json` · `operator.jsonl` ·
`anchors/` · `backups/` · `demo-summary.json` · `demo-ats.csv`. All `*.jsonl` are
append-only; only `send_log.jsonl` is chained.

### Environment variables

| Variable | Effect |
| --- | --- |
| `LEDGER_OPERATOR` | Default operator when `--operator` is absent |
| `LEDGER_DEBUG_IDS=1` | Reveal raw ids/emails in console output |
| `LEDGER_ABORT_AFTER_SENDS=n` | Test-only: abort dispatch after n sends |

### UI routes (`ledger ui`, `--features ui`)

`GET /` (triage + 4 KPIs) · `GET /governance` · `GET /audit` ·
`POST /api/preview` · `POST /api/dispose` · `POST /api/dispose/batch`.
Token→cookie at `GET /?token=`; `401` without session, `403` on role, `409` on lock.

### The three circuit-breakers

1. **Completeness** — no locked, timestamped disposition → block.
2. **Gate consistency** — form answer ≠ the candidate's `KO_ANSWER_0n` → block and
   escalate, never auto-correct.
3. **Version resolution** — the logged `TEXT_VERSION` is the copy that rendered; nothing
   renders from unsigned copy.
Plus: 100% of GATE and 10% of FIELD/ADMIN need a recorded second-person QA check.

---

## 12. Glossary

- **Disposition** — the reason recorded for one rejection: `GATE` (a stated requirement
  not met), `FIELD` (requirements met, stronger field appointed), `ADMIN` (closed for a
  reason unrelated to the candidate).
- **Gate** — one of five Tier 0 deterministic knockouts; versioned copy that may require
  requisition facts before it can be used.
- **Fact** — a requisition-level input a gate block needs (e.g. `REGULATOR_URL`),
  documented before the requisition posts.
- **Comparative paragraph** — the one static "what a competitive application looked
  like" paragraph for `FIELD`; linted at sign-off.
- **QA check** — second-person review of a rendered email, required before dispatch.
- **Seal / anchor** — publishing the chain head off-host so a later verifier can tell
  the log was not rewritten.
- **Torn tail** — a partial final line, the trace of a crash mid-append; refused, never
  repaired, only quarantined deliberately.

---

## 13. How this file was produced

Compressed from the seven spec documents (`plan.md`, `V2.md`, `V3.md`, `V4.md`, `V5.md`,
`Web.md`, `website.md`), the implementation (`src/`, 33 modules), the CI configuration,
and the full build conversation — including the contradictions found in each spec, the
decisions taken to resolve them, and the failures that only showed up in CI. Where this
file and a spec disagree, the spec's *intent* and the code's *behaviour* win; record the
divergence here rather than silently following either.
