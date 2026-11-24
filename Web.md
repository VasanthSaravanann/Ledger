Read-only repository reconnaissance task — do NOT edit any files yet. Inspect `Cargo.toml`, `Cargo.lock`, `deny.toml`, `src/`, `scripts/build-site.sh`, and `site/`, then reply with a single structured Markdown report answering all 10 items below using exact file paths, struct definitions, and function signatures.

1. **Supply-Chain & Feature Flags (`Cargo.toml`, `Cargo.lock`, `deny.toml`):**
   - Paste the exact `[bans]` section of `deny.toml`.
   - State whether `tokio` is currently present in `Cargo.lock`.
   - Verify whether adding `axum`, `tokio` (`rt-multi`, `macros`, `net`), `tower-http`, and `rust-embed` (or `include_str!`) under an optional `ui` feature passes `cargo deny check` (with `hyper` removed from bans while keeping HTTP client crates banned).

2. **Public Library API Surface (`src/lib.rs` & `src/cli.rs` / `src/main.rs`):**
   - List the exact `pub` module exports in `src/lib.rs`.
   - For each operation below, state the exact `pub fn` signature (or state "currently private inside CLI handler at `file:line`" if not exposed):
     1. Load workspace / store
     2. Import/parse ATS CSV
     3. Record single or batch dispositions (`AppendLog::append_batch`)
     4. Render email preview for an applicant + disposition
     5. Execute `.eml` batch export + `manifest.json` (`dispatch --eml`)
     6. Compute workspace metrics
     7. Verify operator signatures & sign-off drift

3. **Candidate, ATS & Disposition Schemas (`src/pipeline.rs`, `src/store.rs`):**
   - Paste the exact Rust struct definitions for an imported ATS candidate/applicant record and a disposition record.
   - Show the exact code/logic used to determine whether an applicant is currently `Pending` vs. already disposed (`GATE`, `FIELD`, `ADMIN`, `Shortlisted`).

4. **Gate Library & Comparative Fields:**
   - Paste the exact struct definitions for the gate library (`GATE-01`..`GATE-05`) and comparative fields (`FIELD-*`), plus the validation rules/required inputs (`documented_facts`, `notes`, `operator`, etc.) needed to record a disposition.

5. **Template Rendering & `.eml` Export (`src/template.rs`):**
   - Paste the exact function signatures and return types for rendering an email preview and generating `.eml` draft files + `manifest.json`.

6. **Operator Auth & RBAC (`src/operator.rs`):**
   - Paste the exact struct definitions for `Operator` / role enum and the function signature that loads an operator + reads `operators/<name>.key`.
   - Confirm whether `ledger ui --operator <name>` should bind a single operator session at launch (protected by a generated one-time URL token on `127.0.0.1`) or allow switching operators in the browser.

7. **Sign-Offs & 13-Item Checklist State:**
   - Paste the exact structs and file paths storing the 13 operational/legal readiness items and the 5 signed artefacts (`library`, `templates`, `playbook`, `requisition`, `paragraph`), plus the exact function signature that computes `Valid` vs. `Stale` vs. `Unsigned`.

8. **Frontend Asset & Offline Policy:**
   - Confirm whether `ledger ui` must work 100% air-gapped with zero external CDN/Google Fonts requests (all CSS and vanilla JS/HTMX embedded in the binary via `include_str!`).

9. **Concurrency & File Locking (`src/store.rs`):**
   - State whether `Store` uses OS file locks (`fs2` / `flock`) or relies on single-process sequential access, and confirm whether wrapping workspace state in `tokio::sync::Mutex` inside Axum guarantees safe serial `append_batch` writes.

10. **Existing Site CSS & Layout (`scripts/build-site.sh`, `site/`):**
    - Show how HTML/CSS is currently generated (inline `<style>` vs. external `.css` file, font stack, color variables) so we know whether to share tokens or build a standalone HR SaaS stylesheet for `ledger ui`.
