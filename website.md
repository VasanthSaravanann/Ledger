# Phase 4 Execution Plan: `ledger ui` (HR SaaS Recruiter Web Workspace)

**Target:** Local-first, air-gapped Axum web application (`127.0.0.1:8420`) gated behind `--features ui`  
**Design Direction:** Enterprise HR SaaS (Crisp & Analytical compliance clarity + Warm & People-First surfaces)  
**Core Invariants Preserved:** Zero outbound network calls in core binary, OS advisory file locks (`fs2`), Ed25519 single-operator session binding, and atomic `append_batch` SHA-256 chain integrity.

---

## 1. Visual & UX Architecture (HR SaaS Design System)

Built as a zero-CDN, embedded CSS/JS asset bundle (`src/ui/assets/`) loaded via `include_str!` so `cargo package` and air-gapped deployments remain self-contained.

### Color Palette (`src/ui/assets/tokens.css` & `app.css`)
* **Surfaces:** Warm alabaster canvas (`--bg: #fbfbfa`), crisp elevated cards (`--card: #ffffff`), subtle slate hover rows (`#f6f7f9`), and hairline borders (`--line: #e2e4e8`).
* **Primary & Human Accents:** Deep Indigo/Slate (`--accent: #1a4fd6`) for primary CTAs and active selection, paired with soft indigo wash (`#eef3ff`) for selected bulk rows.
* **Semantic Status Pills:**
  * **Valid / Signed / Dispatched (`--pass`):** `#1c6b3a` text on `#eaf6ef` pill background
  * **Pending / Needs Attention / Stale (`--warn`):** `#8a5a00` text on `#fef6e6` pill background
  * **GATE Knockout / Blocked / Drift Error (`--fail`):** `#a51b1b` text on `#fdeeee` pill background
  * **FIELD / ADMIN (`--neutral`):** `#3f4650` text on `#f1f3f5` pill background

### Information Hierarchy & Typography
* **Font Stack:** System sans-serif (`ui-sans-serif, -apple-system, "Segoe UI", Roboto, Helvetica, Arial, sans-serif`) and monospace (`ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace`).
* **Tabular Numbers:** Apply `font-variant-numeric: tabular-nums` across KPI summary cards, application IDs (`APP-0142`), disposition IDs (`DSP-0001`), and SHA-256 hash badges.
* **Top Summary Strip (4 Metric Cards):**
  1. **Pending Triage** (count + % of `InitialScreen` roster)
  2. **Disposed Breakdown** (`GATE` vs. `FIELD` vs. `ADMIN` split)
  3. **Dispatch Queue** (`Queued` `.eml` drafts vs. confirmed `Sent`)
  4. **Governance Health** (`5/5` signed artefacts status + `13/13` checklist attestation)
* **Contextual Micro-Copy & Tooltips:** Inline rule guidance inside the triage drawer (e.g., explaining why `GATE-02` is disabled when `requisition.facts` lacks required evidence, prompting the recruiter to select `FIELD` instead).

### Snappy Micro-Interactions (Vanilla JS + CSS Transitions)
* **Slide-Over Candidate Drawer (`220ms cubic-bezier(0.16, 1, 0.3, 1)`):** Clicking any candidate row slides out a right-hand inspection drawer without losing scroll position in the main table.
* **Sticky Bulk Action Bar:** Selecting 2+ checkboxes smoothly animates a floating bottom bar (`"14 candidates selected — Bulk Dispose | Export Preview"`).
* **Live Email Preview:** Changing a `GATE` or `FIELD` dropdown immediately updates the rendered subject and body preview via `POST /api/preview` (`<15ms` local round-trip).

---

## 2. Pre-UI Core Library Prerequisites

Close these 3 library gaps before writing Axum handlers so the web UI delegates 100% of business logic to core modules:

1. **Atomic Batch Disposition (`src/disposition.rs`):**
   * Implement `pub fn record_batch(&mut self, forms: Vec<NewDisposition>) -> Result<Vec<DispositionRecord>>` on `Dispositions`.
   * Validate every `NewDisposition` first, assign sequential `DSP-XXXX` IDs in memory, and commit via a single `AppendLog::append_batch` (`write_all` + `fsync(file)` + `fsync(dir)`).
2. **Library-Level Artefact Verdict (`src/approval.rs`):**
   * Promote the CLI verification logic at `src/main.rs:1578` into `pub enum ArtefactStatus { Valid, Stale { reasons: Vec<String> }, Unsigned }` and `pub fn artefact_status(&self, current_hash: &str) -> ArtefactStatus` on `Approval`.
3. **Triage Queue State Derivation:**
   * Derive the 4 queue tabs directly from existing domain types: **`All`**, **`Pending`** (`dispositions.head(req, app).is_none()`), **`GATE`**, **`FIELD`**, and **`ADMIN`**.
   * Display `roster.excluded_not_initial_screen` as an informational badge in the workspace header (*"N non-InitialScreen rows excluded at import"*).

---

## 3. Supply-Chain, Session & Concurrency Architecture

* **Supply-Chain & Feature Gating (`Cargo.toml`, `deny.toml`, `.github/workflows/ci.yml`):**
  * Add `ui = ["dep:axum", "dep:tokio", "dep:tower-http"]` as an optional Cargo feature (`default = []`).
  * Remove `hyper` from `[bans]` in `deny.toml` while keeping HTTP clients (`reqwest`, `ureq`, `curl`), TLS, and websocket crates banned.
  * Add `cargo deny check --no-default-features` alongside `cargo clippy --all-targets --all-features` in CI.
* **Single-Operator Process Binding (`src/identity.rs`):**
  * Launch via `ledger ui --data <workspace> --operator <name> --port 8420`.
  * Validate `operators/<name>.key` (`0600`) at startup, bind strictly to `127.0.0.1:8420`, print a one-time 32-byte hex session token URL (`http://127.0.0.1:8420/?token=...`), and enforce `Role::{Coordinator, Approver, Auditor}` in Axum middleware (`403 Forbidden` on unauthorized mutations). Do not allow in-browser operator switching.
* **OS-Level File Locking (`src/store.rs`):**
  * Execute every workspace read/mutation inside `tokio::task::spawn_blocking`, acquiring `Store::lock()` (`fs2` `flock`) and reloading fresh workspace state from disk on each request. Never hold a lock guard across `.await`. Return `409 Conflict` with the lock holder details if `Error::Locked` occurs.

---

## 4. Application Screens & Route Map

| Route / View | Role Access | Core Functionality & UX |
| :--- | :--- | :--- |
| **`GET /`** (Triage Workspace) | All (`Coordinator` mutates) | KPI summary cards, search/filter pills (`All`, `Pending`, `GATE`, `FIELD`, `ADMIN`), tabular candidate list, slide-over candidate drawer with live email preview, and sticky bulk-disposition bar (`record_batch`). |
| **`GET /import`** (ATS CSV Mapper) | `Coordinator` | Drag-and-drop CSV upload with auto-detection and saved column mapping profiles for **Workday**, **Greenhouse**, **PageUp**, and **SmartRecruiters** into `APPLICATION_ID`, `FIRST_NAME`, `EMAIL`, `STAGE`, and `KO_ANSWER_01..05`. |
| **`GET /dispatch`** (`.eml` Drafts & Queue) | `Coordinator` | Lists undispatched dispositions; generates RFC 5322 `.eml` drafts + `manifest.json` (`eml::write_batch`), provides `.zip` bundle download and integrity check (`eml::verify_batch`), and displays `DeliveryStatus::Queued` status. |
| **`GET /governance`** (Approvals & Checklist) | All (`Approver` signs) | Displays the 5 hash-bound artefacts (`gate_library`, `templates`, `playbook`, `requisition`, `comparative`) with `Valid` / `Stale` / `Unsigned` badges and one-click Ed25519 re-approval for `Approver` sessions, plus the 13-item operational checklist labeled **"Attested (Unsigned)"**. |
| **`GET /audit`** (Chain & Signatures) | All | Live verification banner running `verify_signatures` + chain check, recent `operator.jsonl` activity feed, and one-click `migrate::snapshot_workspace` backup trigger. |

---

## 5. Implementation Task Checklist

### Slice A — Core Library Prep (`src/disposition.rs`, `src/approval.rs`)
- [ ] Implement `Dispositions::record_batch(&mut self, forms: Vec<NewDisposition>) -> Result<Vec<DispositionRecord>>` using `AppendLog::append_batch`.
- [ ] Implement `Approval::artefact_status(&self, current_hash: &str) -> ArtefactStatus` (`Valid | Stale { reasons } | Unsigned`) and refactor `src/main.rs:1578` to call it.
- [ ] Add unit tests for atomic batch rollback on validation error and `ArtefactStatus` transitions.

### Slice B — Feature & Supply-Chain Wiring (`Cargo.toml`, `deny.toml`, `.github/workflows/ci.yml`)
- [ ] Add `ui = ["dep:axum", "dep:tokio", "dep:tower-http"]` with `tokio = { version = "1", features = ["rt-multi-thread", "macros", "net"], optional = true }`.
- [ ] Remove `hyper` from `deny.toml` bans (updating the comment to reflect optional server feature) and add `cargo deny check --no-default-features` to CI.

### Slice C — Design Tokens & Embedded Assets (`src/ui/assets/`)
- [ ] Extract CSS variables into `src/ui/assets/tokens.css` and refactor `src/site.rs` to share it (bumping `site/provenance.json` with `LEDGER_BUMP_PROVENANCE=1`).
- [ ] Create `src/ui/assets/app.css` (HR SaaS cards, tabular tables, semantic status pills, slide-over drawer, sticky bulk action bar).
- [ ] Create `src/ui/assets/app.js` (drawer controller, bulk checkbox state, `POST /api/preview` live rendering, and toast alerts) embedded via `include_str!`.

### Slice D — Axum Server, Auth Middleware & Triage Routes (`src/ui/mod.rs`, `src/main.rs`)
- [ ] Add `Ui` subcommand in `src/main.rs` (`ledger ui --data <dir> --operator <name> --port 8420`) validating `operators/<name>.key` and generating a one-time session token.
- [ ] Implement session token cookie/query middleware and RBAC role guards (`Coordinator`, `Approver`, `Auditor`).
- [ ] Implement `spawn_blocking` + `Store::lock()` workspace transaction helper returning `409 Conflict` on lock contention.
- [ ] Build handlers and HTML templates for `GET /`, `POST /api/preview`, `POST /api/dispose`, `POST /api/dispose/batch`, `GET/POST /import`, `GET/POST /dispatch`, `GET/POST /governance`, and `GET/POST /audit`.

### Slice E — HTTP & End-to-End Integration Tests
- [ ] Verify unauthenticated requests without the launch token return `401 Unauthorized`.
- [ ] Verify `Auditor` or `Approver` POST requests to `/api/dispose/batch` return `403 Forbidden`.
- [ ] Verify `Coordinator` bulk disposition writes atomically in a single `append_batch` and passes `verify-signatures` + `verify_log.py`.
- [ ] Verify `.eml` batch export from `/dispatch` generates valid `manifest.json` SHA-256 digests and logs `DeliveryStatus::Queued`.