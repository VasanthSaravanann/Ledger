# Changelog — the complete log

Every commit in this repository, what it changed, and the reasoning that is not
recoverable from the diff. Newest first. This is a log, not a release note: it records
what was tried, what was removed on purpose, and the mistakes worth not repeating.

- 28 commits, 24–25 November 2026.
- Specs that drove the work (`plan.md`, `V2.md`…`website.md`) are no longer in the
  tree; they remain readable at `git show 90dfcb6:V2.md`.
- Architecture and current invariants: `SKILL.md`. What is still missing: `PRODUCTION.md`.

## At a glance

| Phase | Commits | Outcome |
| --- | --- | --- |
| v1 pilot | `d6bf2fa` | gate copy, three templates, locked form, chained log, three metrics |
| Static page | `ec66e4a` | first deployable surface (later removed) |
| V2 audit-grade | `acbff64` | GATE/FIELD/ADMIN, versioned gates + facts, paragraph lint, QA sampling, five metrics, routing |
| V3 fixture + site | `7d8d9be` | deterministic fixture, artefact pages, build wiring |
| V4 hardening | `3a7a1b9` | canonical digest v2, crash safety, quarantine, goldens, independent verifier, CI |
| V5 operational | `24145c9`, `bb8ee69`, `c7cc82d`, `16a1caa`, `3f880a2` | CI green, retention/migrate/anchor, Ed25519 + RBAC, drift, drafted dispatch |
| Workspace UI | `7c5326c`, `65b6c22`, `f69e57b`, `43c86bf`, `f71d930` | local `ledger ui`, snapshots, product page + lab, a11y/XSS review |
| Docs | `90dfcb6` | first `SKILL.md` |
| Turn to the web app | `dc1e40b`, `fa19dcc`, `856a35a` | artefact site removed, exported workspace preview, landing page, lint gated |
| Design passes | `959950d`, `18f2f7e`, `047744e`, `bd8ad02`, `f89ac5e` | tasteskill pass, one system, testability, contrast, operator rename |
| Repo quality | `3e26214`, `d2b7a70`, `adf8044`, `7263d0e` | README rewritten, canonical URLs, generator traces removed |

---

## 2026-11-25

### Update the project skill for the completed plan run

`SKILL.md` was still the manual written before the loop ran: §7 claimed 136 and 144 tests,
§2 said nothing about the plan's state, and the repository map had no `scripts/`, `docs/`
or `.github/workflows/` row. It now carries the current state — 155 tests (163 with
`ui`), `--json` on every command, `schema_version` on every state document, release
v0.1.0, the daily `verify` and weekly `update` jobs — and a short log of the run: the 13
tasks shipped, the 6 that await a person, and the reminder that `STOP=awaiting-human` is
the end state, not a reason to invent work.

Also updated: the invariant table gained state versioning, the JSON contract and the
skip-link/tap-size floor; the commands block runs the four new scripts; the gotcha table
gained the `macos-15`/`macos-15-intel` runner labels, the global `--json`, the ignored
`evidence/` directory, the off-canvas drawer that must leave the tab order, and the
`outline-none` utility that makes the checker's `focus-ring` fail. §11 now points at the
six new documents. The **complete log** stays `CHANGELOG.md`; `SKILL.md` now says so in
its first paragraph.

### Record the autonomous run: 13 of 19 tasks done

The plan drained its machine work. `node scripts/plan-next.mjs` now prints
`STOP=awaiting-human`: all 13 `auto` tasks are `done` (P0-1…P0-5, P1-1, P1-2, P1-4…P1-6,
P2-1…P2-3, P2-5), none is blocked, and the six left are the ones a machine may not
finish — P0-4 (access review), P0-6 (purge rehearsal) and P0-7 (the 13 checklist items)
are signatures and decisions; P1-3 needs the escalation phone answered; P2-4 needs branch
protection turned on; P2-6 needs one recorded design decision.

What the run left behind, all checkable: `delivery.jsonl` and the only path from `queued`
to confirmed; `docs/key-custody.md`, `docs/monitoring.md`, `docs/incident-runbook.md`,
`docs/dsar.md`, `docs/evidence.md`, `docs/restore-drill.md`; `scripts/restore-drill.sh`,
`scripts/verify-chain.sh`, `scripts/dsar-drill.sh`, `scripts/evidence-pack.sh`; a daily
verification workflow and a weekly dependency-update workflow; `--json` on every command;
schema versions on every state document; three proptest suites; and release `v0.1.0` with
three targets and checksums.

One red push (P1-5) is recorded above: its changelog entry was missing and the next
commit added it. The status line in `PRODUCTION.md` now reads 13/19. Nothing in this run
signed for a person, and `plan-done.mjs` refused to close any of the six that would need
one.

### P2-5 Automate dependency updates behind the same gate

`cargo-deny` and the MSRV job existed, but nothing brought updates forward, so the tree
aged until it became a project. `.github/workflows/update.yml` runs weekly (Monday 05:00
UTC) and opens two grouped pull requests: one from `cargo update` on `Cargo.lock`, one
from `bun update` in `web/`. Each PR is proven locally first — `cargo test --locked` for
Rust, lint, tests and the Vercel build for the web — and then gated again by the existing
`ci` workflow, so a dependency that raises the MSRV fails the `msrv` job and stays open.
The branches are fixed (`deps/cargo`, `deps/bun`), so next week updates the same PR
instead of multiplying them. `deny.toml`, `clippy.toml` and the coverage floors are
untouched: the update adapts to the checks, never the other way round. Merging stays a
human decision.

### P2-3 Close the accessibility findings on the preview

The checker still reported no skip link and sub-44px controls in the footer at 320. Both
surfaces now carry a `SkipLink` as the first element in the DOM — a fixed, 44px control
that is off-screen until focused and targets `<main id="main" tabindex="-1">` on every
route, including the 404 and error pages. The workspace drawer is a real drawer now: the
menu button gained `aria-expanded` and `aria-controls`, opening moves focus to a
close button, Escape closes it, and leaving it closes it rather than letting it cover
the next control. `visibility: hidden` takes the closed drawer out of the tab order,
immediately — the 220ms visibility delay let focus land in a drawer that had just closed.

The 40 `outline-none` utilities were removed from the shadcn components, so the
stylesheet no longer contains a rule that deletes the focus outline; one ring is defined
in `styles.css` and appears at once (`transition-property: none` while focused). At 480px
and below every link, button, summary, select and field gets a 44px floor, with
icon-only links taking a 44px `.tap-target` box. The landing footer links went from 32px
to 44px on a phone.

Proved by the real checker on all six routes at 390 and 320: `skip-link`, `focus-ring`,
`focus-visible`, `focus-hidden`, `tap-size` and `menu-toggle` all PASS (`consistency.mjs`,
the taste gate's browser). `src/test/accessibility.test.tsx` adds five jsdom tests: the
skip link is the first tab stop on the landing page and in the workspace, it targets
`main#main`, and the stylesheet still hides-then-shows it, draws a ring, and holds the
44px floor.

### P2-2 Generate the audit evidence pack

An auditor would have had to ask six questions. `scripts/evidence-pack.sh` writes
`evidence/<date>/` with the chain verification from both implementations, the signature
report and its exit code, the retention preview, the restore drill's output and timings,
every off-host anchor receipt, the signed access review when it exists, and the release's
`.sha256` assets, plus an `index.json` naming each file, its size and its SHA-256. A copy
of the index lands at `evidence/index.json`, which is the path `PRODUCTION.md` names.

It runs against a fresh fixture by default and against a pilot with `--data … --operator …`.
It collects no candidate data — no names, addresses, bodies or dispositions — so a pack
can be handed over without handing over the people it is about. What is missing is
recorded in the index's `notes` rather than omitted: today that is the anchors (the
fixture publishes none) and `docs/access-review.md` (P0-4 is still a human act).
`evidence/` is git-ignored: the pack is produced on demand, not committed.
`docs/evidence.md` is the ten-minute reading order and what the pack cannot prove.

### P2-1 Add the three properties as proptest suites

The invariants were covered by examples, and a chain that only survives hand-written
cases is not evidence. `tests/proptest_chain.rs` adds three generated-input suites, 24
cases each: **no send without a disposition** (any application id, any candidate, an
empty disposition log → one `MissingDisposition`, zero entries in the send log);
**the chain stays linked under arbitrary appends** (0–5 rendered subjects and bodies,
reopened from disk, every `prev_hash` points at its predecessor and every `entry_hash`
re-hashes from `canonical_payload`); **supersede never rewrites history** (a correction
leaves the earlier line byte-for-byte intact and appends exactly one record, which still
names the record it supersedes).

The cases are few on purpose: each one fsyncs a workspace, so the file costs about four
seconds, well inside the one-minute budget. `proptest = "1.9"` is a dev-dependency; the
lock resolved 1.11.0, whose declared `rust-version` is exactly this crate's 1.85, so the
MSRV job still compiles `--all-targets`. The dependency is dev-only: `cargo build` and
the release tarball are unchanged.

### P1-6 Version every state document, not just the send log

A workspace from an older build was indistinguishable from a corrupt one. Every state
document `Store::load`/`Store::save` touches now carries a top-level `"schema_version":
1` — `gate_library.json`, `templates.json`, `playbook.json`, `requisitions.json`,
`policy.json`, `review.json`, `checklist.json`, `operators.json`, `demo-summary.json` and
each `roster-<REQ>.json`. `Store::load` refuses a version it does not know (exit 3, file
untouched); `ledger migrate` snapshots first, then rewrites only state documents, and
reports each one by name. It never touches `send_log.jsonl` or `delivery.jsonl`, which is
asserted by comparing the log's bytes across a migration.

Four documents were bare JSON arrays before versioning — `operators.json`,
`requisitions.json`, `checklist.json` and `review.json` — and a version-1 document must be
an object to carry the field. Each type now serialises as a named field and still
*deserialises* the legacy array, so a pre-versioning workspace can be read, migrated and
only then written. `Store::save` refuses a non-object rather than silently dropping the
version. `FORMAT_SPEC.md` §8.1 is the normative description.

The fixture moved: the workspace fingerprint is now `e0a91f66…` because the documents
gained a field. The send-log digest, its head `d347a639…` and the dispositions digest did
not move, which is the point. `tests/schema_version.rs` proves the stamp on every file,
refusal before any snapshot on an unknown version, the v0 → v1 upgrade for the four array
shapes and an idempotent second run.

### P1-5 Drill the data-subject request path

"What do you hold about me?" had a different answer for unchained state than for the send
log, and nobody had walked it. `scripts/dsar-drill.sh` now walks it on the fixture:
computes the redacted handle, resolves it with `find --hash`, exports the requisition with
`log export`, previews retention with `purge --dry-run`, and prints the split. On the
fixture: 1 matching disposition, 1 of 147 send rows for `APP-1001`, and 0 dispositions
purged because the 2557-day cutoff lands in 2019 — the retention clock has not run. The
last line states the design plainly: erasing a chained send means deleting the workspace,
never editing a line.

`docs/dsar.md` is the one-page answer: who decides, the three commands, which store holds
what and whether it is tombstoned in place or only with the workspace, why backups count,
and what the drill does not prove (identity, replicas, jurisdiction, erasing one person
from a chain). `purge` is a mutating command even when it only previews, so the drill
reads an approver from the fixture's registry rather than assuming a name.

This commit's CI run was red for one reason: its CHANGELOG entry was missing (the
insertion in the previous step missed its anchor and the replacement silently did
nothing), and `scripts/check-docs.mjs` refused the push. The code, the drill and the docs
were green; the entry was added in P1-6. Recorded here because a red run should not
disappear from the log.

### P1-4 Cut and verify release v0.1.0

`release.yml` existed but no tag had ever been pushed, so there was no downloadable
artefact with a checksum an auditor could keep. The publish job now requires every
tarball to contain `verify_log.py`, `FORMAT_SPEC.md`, `THREAT_MODEL.md` and the licence —
and fails if one is missing — and it composes release notes from the tree instead of
adjectives: the test count at the tag and the fixture chain head, both computed in CI,
plus the filename to verify against the `.sha256`. `generate_release_notes` still adds
GitHub's own summary after them.

Tag `v0.1.0` points at this commit; the three targets (aarch64-apple-darwin,
x86_64-apple-darwin, x86_64-unknown-linux-gnu) build, package and publish. The first
attempt failed before compiling anything: `macos-14` resolved to an unsupported runner
configuration and `macos-13` never left the queue, so the matrix moved to `macos-15` and
`macos-15-intel`. The README's limitation row changed with it: the remaining gap is that
the binary is not code-signed, not that no release exists. Because the gate
(`gh release view v0.1.0`) can only pass after the tag's workflow finishes, the plan state
lands in a one-line follow-up commit rather than the tagged one.

### P1-4 Record the verified v0.1.0 release

The gate ran against the published release: `gh release view v0.1.0` finds 14 assets —
three tarballs, three bare binaries, three `.sha256` files, `verify_log.py`,
`FORMAT_SPEC.md`, `THREAT_MODEL.md` and `LICENSE`. The notes name 148 tests at the tag and
the fixture head `d347a639…`. This commit carries only `.plan/state.json`.

### P1-2 Write the incident runbook

The first responder would have had to read the repository at 2am. `docs/incident-runbook.md`
covers six incidents — a torn tail, failed verification, a lost key, a wrong disposition,
a hostile reply and a data-subject request — and gives each the same shape: copy the
whole workspace first (including `operators/` and `anchors/`), three commands in order,
the decision owner, and the evidence to keep. It names roles through the files that
already name people (`governance show` for the escalation owner, `operators.json` and the
access review for the rest) rather than inventing a roster, and ends each section with
what not to do, because the likely 2am mistake is to edit the thing that proves it.

Every command in it exists and is quoted with its real flags; every failure message is
the one the binary prints. The six headings are the six words the gate greps for, so the
document cannot lose a section silently.

### P1-1 Emit --json from every command

Eleven of 25 commands printed prose, and only that: a runbook could not consume a single
one. `--json` is now a **global** flag on the clap tree, so `ledger status --json`,
`ledger gate approve … --json` and `ledger --json metrics …` all work, and every command
prints exactly one object on stdout with nothing on stderr. Every object carries
`"command"`, the leaf that produced it. The 13 per-command flags were removed in favour
of the one flag, and the handlers were rewritten to emit the real structure: the stored
document for `gate list`/`templates show`/`requisition show`, the appended record for
`disposition record`/`delivery confirm`/`qa record`, the chain report for `log verify`,
and the computed decision for `review route`. `dispatch --json --eml` still prints one
object: the draft manifest is a key on it, not a second document.

`tests/cli_json.rs` enumerates the clap tree through `--help` — so a command added
without the flag fails the build — and then runs 21 read commands and 2 writes against
the fixture, parsing each output as one object and requiring an empty stderr. README
documents the keys of all 26 commands.

The sweep found a real defect: `review criteria` was permitted by no role at all, so once
an operator was registered the command could never run. `Role::permits` now grants
`review` to coordinators, next to `delivery`. `scripts/check-docs.mjs` now counts the
`Command` variants instead of the old per-command flag count, so the number in `SKILL.md`
still means something.

### P0-5 Schedule chain verification with an alarm that fires

Corruption used to be discovered by whoever happened to run a command. `scripts/verify-chain.sh`
now runs four checks and requires them to agree: `ledger log verify`, `verify_log.py`,
`ledger verify-signatures` and `ledger delivery list`. It compares the two heads on
purpose — if the Rust chain and the independent verifier disagree, neither is trusted.
Exit 1 is the alarm, exit 2 is a tooling problem, and both print a `ALARM …` line.

`.github/workflows/verify.yml` runs it daily at 06:17 UTC against a restored copy of the
147-entry fixture (CI has no pilot disk, and a scheduled job must not write to one), runs
the restore drill first, then requires `--fixture-tampered` to fail so a verifier that
always says OK cannot pass. The failure path is also a test: `tests/monitoring.rs` makes
the script fail on an edited value, a torn tail and a broken signature.

`docs/monitoring.md` says who is paged (`escalation_owner` in `policy.json`, 24-hour
SLA), the first fifteen minutes (copy the workspace before touching it, re-run by hand,
compare the head with `anchors/`, restore or stop), how to read each message, and what
the alarm cannot cover: the live workspace unless a timer runs there, anchoring, and
availability.

### P0-3 Write the key custody and rotation procedure

Identity was an Ed25519 keyfile at `0600` with no documented holder, rotation path, or
meaning of a lost key. `docs/key-custody.md` now names the five keys a pilot needs
(two approvers, two coordinators, one auditor), where each private half lives
(`operators/<name>.key`, `0700` directory, `0600` file, no passphrase), what `Role::permits`
lets each sign, and why the fleet ships no real keys and no invented people: the names
are the rows of `operators.json` and the signed access review.

Rotation is by a **new identity**, never an in-place re-key: the registry refuses a
duplicate name, and every `Signed` record embeds the public key it was made with, so
old signatures keep verifying after rotation and `verify-signatures` keeps reporting
them `valid`. Re-signing an unchanged artefact is refused (`AlreadySignedOff`), which
is why the doc says the new key signs the next copy. Loss costs the ability to sign new
records as that name — nothing else — and a leaked key is worse than a lost one because
there is no revocation list.

### P0-2 Prove backup and restore on a fresh directory

`backups/` existed and dispatch snapshotted into it, but no restore had ever been
executed. `scripts/restore-drill.sh` now seeds the fixture, copies the whole workspace
(including `operators/`), restores it into an empty directory, and asserts four things
agree: `ledger log verify`, `ledger verify-signatures`, the recomputed fixture
fingerprint, and the independent verifier's head. It then edits one covered value in a
copy and requires `log verify` to fail, so a verifier that always says OK cannot pass.
It prints per-step milliseconds: seed 3658 ms, backup 40 ms, restore 38 ms, verify all
452 ms, tamper check 84 ms on the author's machine.

`docs/restore-drill.md` states what the drill proves and what it cannot: off-host
copies, recency, key custody, retention, and the anchor receipts remain human
obligations. The fingerprint is computed on both sides rather than read from
`demo-summary.json`, because the CLI appends an operator note after seeding.

### P0-1 Record delivery confirmation as a chained event

The send log recorded `queued` and then nothing ever moved a draft to `sent`: the
"confirmed sent" number could not be non-zero, and the log could not prove anything
arrived. The status in a send entry is fixed when it is written, so the later fact got
its own chain. `delivery.jsonl` holds `DELIVERY_CONFIRMED` entries — `send_id`,
`confirmed_by`, that operator's Ed25519 public key, `confirmed_at`, `seq`,
`prev_hash`, `entry_hash` — chained and verified under the rules already used for the
send log. `ledger delivery confirm --send-id …` appends one; `ledger delivery list`
shows them; `ledger metrics` now reports `confirmed` against `queued`, where a
confirmation wins over the status recorded at dispatch.

What must not move did not move: a workspace that never confirms keeps a byte-identical
send log, and the fixture's head `d347a639…` and fingerprint `64f9e1f9…` are still
pinned by `tests/golden.rs`. The new goldens pin the queued send's bytes
(`626b75e9…`) and the first confirmation's hash (`777907a6…`), and assert that
confirming changes no send-log byte. `FORMAT_SPEC.md` §11 is the normative payload;
`verify_log.py` still covers the send log alone, and says so.

### `af29e3b` — Split the docs into an operating manual, a complete log, and a production plan

`SKILL.md` had grown into a history: 32 decisions, seven spec lineages, an open-work
list — useful once, stale quickly, and duplicating the README. It is now the operating
manual (196 lines): what the two surfaces are and which one records, current verified
numbers, the repo map, twelve decisions that must not be reversed, the invariant-to-test
table, run/test/deploy commands, the ten gotchas that have actually bitten, and pointers.

The history moved here, commit by commit, with the reasoning and the mistakes — including
`3e26214`, which contained only a `.pyc`. `PRODUCTION.md` is new: readiness at a glance,
P0 blockers (delivery confirmation, restore drill, key custody, chain monitoring, purge
rehearsal, the seven human acts), P1/P2/P3, a six-week sequence, a copy-pasteable
go/no-go gate, and the definition of done for each surface.

### `21721b5` — Turn the production plan into a loop an agent can run unattended

The previous plan was a good list and a bad instruction: prose acceptance criteria, a
six-week timeline, no way to tell whether a task was finished. It became data — 19 tasks
in `.plan/tasks.json` with priority, dependencies, acceptance and one shell gate each —
plus `.plan/state.json` and three runners. `plan-done.mjs` runs the gate itself and
refuses to close a red task; `human` and `human-gated` tasks refuse to close without
`--signed-by` and a real evidence file, so an agent cannot sign for a lawyer. Two failed
attempts block a task; three blocked in a row end the run.

### Make the docs checkable, and fix the two defects it found

This commit adds `scripts/check-docs.mjs` and wires it into CI, then fixes what it found:

- `CHANGELOG.md` was missing `af29e3b` and `21721b5` — the log claimed "every commit"
  and did not have them. Both now have entries.
- `P2-5` (dependency automation, class `auto`) declared a dependency on `P2-4` (branch
  protection, `human-gated`). The loop would have run out of runnable work and stopped
  with a person's task as the reason — a deadlock disguised as an empty backlog. The
  dependency is gone: the workflow can land before protection exists.
- `plan-next.mjs` now refuses a plan it cannot drain: a dependency that is not in the
  backlog, a cycle, or an `auto` task waiting on a non-`auto` one exits 2 with the
  offending pair named.

`check-docs.mjs` asserts three things on every push: every commit is in this file (by
sha, or by subject for HEAD), the numbers `SKILL.md` quotes match the tree (33 modules,
6 routes, 11 `--json` flags, 136 tests, 144 with `ui`, counted from `cargo test --list`),
and every promised document exists.

### `7263d0e` — Remove every trace of the generator from the web app

The preview still carried its scaffolding: `.lovable/`, a prompt-managed `AGENTS.md`,
boilerplate README, an error reporter that poked `window.__lovableEvents`, a build-config
package in `package.json`/`bun.lock`, a preview-proxy wrapper whose default target was a
Cloudflare Worker, and the generator's favicon.

Replaced rather than deleted: `vite.config.ts` is now plain TanStack Start (tailwind,
tsconfig paths, Start plugin with the same import protection, Nitro, React), with
Nitro's default preset set to `vercel` so `bun run build` needs no environment variable.
`public/favicon.svg` is the Ledger mark and `/favicon.ico` 308s to it. The three
architecture rules that were ours moved into `web/README.md`.

### `adf8044` — Give every page its own canonical and og:url

No route declared its address, so a share of `/audit` advertised whatever URL the crawler
had. One `SITE_URL` in `web/src/lib/site.ts` now feeds `<link rel="canonical">` and
`og:url` for all six routes.

### `f89ac5e` — Rename the workspace operator to John, and say what he is

The sidebar read "Vasanth S / Coordinator preview" — a personal name where a session
role belongs, and "preview" for the third time on one screen. Now "John" with the role
under it, "Coordinator" (the RBAC vocabulary); the audit trail says "John · Recruitment
coordinator". Plates re-shot because each one shows that block.

### `3e26214`, `d2b7a70` — Rewrite the README around replication

The old quick start **could not run**: it approved gate copy before any operator existed,
and after the first `operator register` every later command was refused because the first
registered operator becomes the acting identity and a coordinator may not run `operator`.
It linked `plan.md`/`V2.md` (not in the tree) and asked for `--file export.csv` without
shipping one.

The new README leads with the problem and the five steps, gives two short paths to try
it, and a full pilot from an empty directory that was executed end to end before being
written down — including the approvers-first order and `--operator` on every command.
`examples/export.csv` was added for the import step; the tracked `__pycache__/*.pyc` was
removed and ignored.

*Mistake worth recording:* `3e26214` was meant to be that change but contained only the
`.pyc` deletion — a `git add` with a stale pathspec aborted the rest. It was not amended
or force-pushed (the repo still synced to a generator then); the content landed in
`d2b7a70`.

### `bd8ad02` — Stop the landing's link rule from painting button labels near-black

`.lp a { color: inherit }` is specificity 0,1,1 and beat shadcn's
`text-primary-foreground` utility at 0,1,0, so every anchor rendered as a button drew
`--foreground` on `--primary`: about **1.9:1**, effectively invisible. The hero, the nav
and the closing CTA were affected; the `Hash this text` control was not, because it is a
real `<button>`. Fixed by dropping the declaration (preflight already inherits at element
specificity). Measured after: 6.33:1 primary, 14.39:1 outline.

### `047744e` — Tell a visitor how to test the preview, and give Import a CSV to eat

The site was live but silent about what to click, and the import screen needed an ATS
export the visitor did not have. The closing section carries three numbered steps;
`/sample-ats.csv` is a real eight-row export with the exact V2 headers, two rows past the
initial screen so the filter is demonstrable.

### `18f2f7e` — Make the landing page and the workspace one system

They read as two products: landing serif/16px/46px buttons/9px cards and a bare
wordmark; workspace sans/14px/36px buttons/7px cards and an indigo tile. The landing's
own plates were showing a different interface than the page around them. The landing now
uses the workspace's grammar — shared `BrandMark`, the app's 70px white top bar with the
`UI preview` pill, the app's sans and 14px base, 10px uppercase micro-labels, the shadcn
button set, 7px cards — with the hero only a scale step above the app's headings.

### `959950d` — Add a tasteskill pass to the landing page

Run under the design-taste checkers (43 FAIL at 1440), the page gained: an h1 that names
category and buyer, one hard fact from the fixture, a hero action, sections that fill
their own screen, one h2 size, hanging list markers on the margin, one button height,
44px targets at 320, restored focus rings, five real captures of the workspace, a hero
entrance, hover/press states, and a chain diagram built from the fixture's own entry
hashes instead of invented ones. **30 FAILs remained** (container grid, one-section
timing, visuals in three sections, no dealt moment) because closing them needs the kit's
`.wrap` grid and no hanging margin — i.e. replacing the design, not adding to it.

### `856a35a` — Make the web lint pass, and gate it in CI

`bun run lint` never terminated: the flat config ignored `dist`, `.output` and `.vinxi`
but not `.vercel`, so ESLint walked the Build Output API bundle and prettier parsed
thousands of bundled `.mjs`. With the ignore fixed it finished in ~4 s and reported 210
formatting errors — the app had never been formatted to its own `.prettierrc`. `bun run
format` fixed them mechanically (byte-identical render proven by screenshot comparison),
`AGENTS.md`/route docs were excluded rather than reflowed, and the `web` CI job now runs
lint before tests.

### `fa19dcc` — Add a landing page that explains the pilot and leads into the workspace

`/` was the triage table, which explains nothing to someone who has not been walked
through the tool. `/` became the explainer; the workspace moved to `/triage`. One live
action: the sample record's text hashed with WebCrypto, showing the value the log stores
as `text_digest`.

### `dc1e40b` — Replace the static artefact site with the exported workspace preview

Removed the generated site, its generator (`src/site.rs`, 1,371 lines), `--export-html`,
`site/`, `vercel.json`, `.vercelignore`, `scripts/build-site.sh`, the CI drift check and
`pulldown-cmark`; added the exported TanStack Start app under `web/` and pointed the
Vercel project's Root Directory at it. The seven specification documents were committed
as removed.

## 2026-11-24

### `90dfcb6` — Add SKILL.md

The project compressed into one file: what it is, seven spec lineages, 32 decisions, the
invariants with their tests, failure modes, open work.

### `f71d930` — Fix the web findings: XSS sink, CSP, table semantics, a11y and motion

A review pass over the generated site: table rows built with `innerHTML` from
operator-authored facts (stored XSS), a skip link permanently clipped, `px` body type
that ignored OS text scaling, smooth scroll and a bulk slide that ignored
`prefers-reduced-motion`.

### `43c86bf`, `f69e57b` — Product page, live lab, real workspace snapshots

The static site became a product page with an interactive `/lab` that re-hashed the
canonical payload in the browser, plus `/workspace` snapshots captured from a live
`ledger ui` run. (All removed later.)

### `7c5326c`, `65b6c22` — Phase 4 start: the local workspace server

`record_batch`, `ArtefactStatus`, `verify_workspace`, the `ui` feature gate, embedded
assets, axum with token→cookie, RBAC per request and `flock` inside `spawn_blocking`.
`65b6c22` fixed a CI-only `ConnectionReset`: the test harness dropped the child's stdout
after reading the banner, and the server's next `println!` hit EPIPE on Linux.

### `3f880a2` — Bind sign-offs to copy, enforce drift, drafted dispatch

Approvals became bound to the copy hash: editing copy makes an approval **stale** (the
old signature is kept so drift is reportable), re-approval appends, and `.eml` batches
record `queued` with a per-file SHA-256 manifest instead of claiming delivery.

### `16a1caa` — Verifiable operator identities, signing and RBAC

Ed25519 keypairs at `0600`, three roles enforced per CLI command and per HTTP request,
signed dispositions, `verify-signatures`, snapshots before mutations.

### `24145c9`, `bb8ee69`, `c7cc82d` — V5 phases 1, 2, 5

MSRV compiled in CI, `0600` fixes, chrono advisory ignored with a review date, coverage
floors, PII-free backtraces, LICENSE; then retention `purge`, `migrate`, `--json`,
`anchor`; then reproducible site provenance and the removal of a duplicated deploy
workflow in favour of the Git integration.

### `3a7a1b9` — Harden the log format, crash safety, and verification (V4)

Canonical digest v2 (sorted keys, no whitespace, integers only, NFC, epoch seconds,
21-field allowlist), atomic append + fsync + dir fsync, torn-tail detection that refuses
instead of repairing, `log quarantine`, golden fixtures, `verify_log.py`, `FORMAT_SPEC.md`,
`THREAT_MODEL.md`, and CI. Resolved four self-contradictions in the spec (recorded in
`SKILL.md` §5 history).

### `7d8d9be` — V3: deterministic fixture and build-time artefact site

`ledger demo seed [--force] [--record]` producing 151 CSV rows → 150 roster, 12 GATE /
129 FIELD / 8 ADMIN, 1 missing disposition, 147 sends, 3 blocked, fingerprint
`64f9e1f9…`, head `d347a639…`. Determinism is asserted by test.

### `acbff64` — V2: the audit-grade rewrite

GATE/FIELD/ADMIN with no free text, versioned gate blocks with fact dependencies, the
comparative paragraph and its lint, the five-script reply playbook, hard stage filter,
QA sampling, five metric bands, routing consequences.

### `ec66e4a` — First deployable page

A static Vercel page because the CLI had no web surface. Superseded by every later web
change and finally removed.

### `d6bf2fa` — v1 pilot

Gate copy, three templates, a locked form, a hash-chained send log, gate blocking, three
metrics and go/no-go routing.

---

## Removed on purpose

| Removed | When | Why |
| --- | --- | --- |
| Generated artefact site + `src/site.rs` | `dc1e40b` | it published HTML nobody ran; the app replaced it |
| `--export-html`, build-site script, drift check | `dc1e40b` | only existed to serve that site |
| `pulldown-cmark` | `dc1e40b` | only the site's markdown rendering used it |
| Seven specification documents | `dc1e40b` | superseded by `SKILL.md`; readable at `90dfcb6` |
| Generator scaffolding, config package, favicon | `7263d0e` | replaced with plain TanStack Start and a Ledger mark |
| `.vercelignore`, pre-built deploy workflow | `c7cc82d`, `dc1e40b` | the Git integration deploys `web/` |

## Incidents worth remembering

1. **EPIPE in CI only** — the test harness dropped a child's stdout; Linux panicked on
   the next write. Fix: hold the reader.
2. **Drift check unwinnable** — provenance read `HEAD`, so pages at N regenerated as N+1.
   Fix: provenance is a committed input.
3. **Sign-offs reported invalid *and* stale** — the signature was checked against the
   current hash instead of the recorded one. Fix: verify against `artefact_hash`.
4. **Fixture rejected by its own rule** — the fixture clock was in the machine's future;
   base time moved to `2026-11-02T09:00Z`.
5. **Button labels at 1.9:1** — a landing `a` rule outranked a component utility
   (`bd8ad02`).
6. **A commit that contained nothing but a `.pyc`** — a failing pathspec aborted
   `git add`; the intent landed one commit later (`3e26214` → `d2b7a70`).
