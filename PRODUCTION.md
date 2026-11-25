# Production readiness — autonomous execution plan

This is a loop an agent can run without asking questions until there is nothing left a
machine may do. The backlog is data, the state is data, and "ready" is a command that
exits 0 — not a judgement.

- **Backlog (authoritative):** `.plan/tasks.json` — 19 tasks, priorities, dependencies,
  acceptance text, and one shell gate each.
- **State (written only by the loop):** `.plan/state.json` — status, attempts, commit.
- **Runner:** `scripts/plan-next.mjs`, `scripts/plan-done.mjs`, `scripts/plan-status.mjs`.

Nothing here is aspirational: every task names the command that proves it done.

---

## 1. Definition of production ready

Two surfaces, two answers.

**The recorder is production ready when**

1. all 19 tasks are `done`, except the ones marked `human`/`human-gated`, which are
   `done` with a named signer and their evidence file present;
2. `node scripts/plan-next.mjs` prints `STOP=done` (or `STOP=awaiting-human` with only
   signed-evidence tasks left);
3. the go/no-go gate in §8 returns **GO** on a rehearsal workspace.

**The preview is production ready when** it stays what it is: reachable, honest about
being a demo, carrying no personal data, and green on the repo-wide gate. It gains no
authentication and no persistence until a human decides the product is a hosted service
— that decision is task `P2-6`'s sibling and explicitly out of scope (§7).

The pilot itself is ready only when a person has signed the 13 checklist items. **A
machine can never finish that**, and the loop must stop rather than pretend.

## 2. The loop

Run from the repository root. Each iteration is one task, one commit, one push.

```bash
# 0. Never start dirty. A dirty tree means the last iteration died mid-way.
git status --short && git pull --ff-only

# 1. Ask what to do. Prints TASK=<id> … GATE=<command>, or STOP=<reason>.
node scripts/plan-next.mjs

# 2. Do exactly that task. Planned files only; scope creep is a failed task.
#    The acceptance line in the task is the contract, not a suggestion.

# 3. Prove it. The task's own gate first.
bash -c "$(node scripts/plan-next.mjs | sed -n 's/^GATE=//p')"

# 4. Prove the repo still works (see §4). Never skip because "it is only docs".

# 5. Close it. This runs the gate again and refuses to lie.
node scripts/plan-done.mjs <id>              # auto tasks
node scripts/plan-done.mjs <id> --signed-by "M. Okafor"   # only a person runs this

# 6. Record and ship. One commit per task, subject prefixed with the id.
git add -A && git commit -m "P0-1 Record delivery confirmation as a chained event" \
  && git push origin main

# 7. Verify the push: CI green, and for web tasks the deployment READY.

# 8. Back to step 1.
```

If the gate fails: one repair attempt, then

```bash
node scripts/plan-done.mjs <id> --fail "what the gate said"
```

Two failed attempts mark the task `blocked` automatically, and step 1 moves on to the
next ready task. Three blocked tasks in a row is `STOP=blocked`; write the report (§9)
and end the run.

## 3. Guard rails the loop may not break

1. **No `--force`, no skipping a gate.** A task is done because its command exited 0.
2. **No signing for a person.** `plan-done.mjs` refuses `human`/`human-gated` without
   `--signed-by`, and the signer's evidence file must exist. Writing "APPROVED" into an
   evidence file to close a task is fabrication, not completion — it is the one failure
   that would make the whole pilot worthless.
3. **No weakening the checks.** `deny.toml`, `clippy.toml`, the coverage floors, the MSRV
   job and the golden fixtures are the product. A task that fails them is not done; it is
   blocked.
4. **No format change without a `digest_version` bump.** If a task touches the covered
   bytes, update `FORMAT_SPEC.md`, regenerate the goldens, and re-run `verify_log.py` in
   the same commit.
5. **No second product.** No generated site, no committed HTML, no second design system,
   no auth on the preview, no ATS integration, no model in the send path (§7).
6. **The plan must be able to drain.** `plan-next.mjs` exits 2 on a dependency that does
   not exist, a cycle, or an `auto` task waiting on a non-`auto` one — the loop refuses to
   idle on a person without saying so.
7. **A red push is reverted, not patched forward.** `git revert` the task commit, mark it
   `--fail "reverted: <reason>"`, and continue.

## 4. Repo-wide gate (step 4, every task)

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features --offline -- -D warnings
cargo test --locked --offline
cd web && bun install --frozen-lockfile && bun run lint && bun run test && bun run build
```

For a web task also: `curl -s -o /dev/null -w '%{http_code}' <preview URL>` after the
deployment is `READY`. For a task touching the chain: `./verify_log.py` and
`./target/release/ledger log verify` must agree on the same head.

## 5. Task classes

| Class | Who can close it | State it may reach alone |
| --- | --- | --- |
| `auto` (13) | the loop | `done` on a green gate |
| `human-gated` (3) | a person, `--signed-by` | `todo` → the agent produces the artefact, then it waits |
| `human` (3) | a person, `--signed-by` | the agent may draft support material; never `done` |

`node scripts/plan-status.mjs` shows the split and what each waiting task is blocked on.

## 6. Ordering

Priority, then id, then dependencies — the runner does this for you.

| Phase | Tasks | Intent |
| --- | --- | --- |
| P0 (7) | delivery confirmation, restore drill, key custody, access review, chain monitoring, purge rehearsal, the seven human acts | nothing real may be sent until these hold |
| P1 (6) | `--json` everywhere, incident runbook, SLA cover, release tag, DSAR drill, state versioning | a pilot at volume, not a rehearsal |
| P2 (6) | proptest, evidence pack, a11y, branch protection, dependency automation, design-gate decision | hardening and the long tail |

`P2-3` (accessibility) and `P2-4` are deliberately dependency-free so the loop always has
something to do while waiting on a human.

## 7. Out of scope — the loop must stop, not invent

- Wiring the preview to the CLI, or giving it auth/persistence/database.
- Windows support, WebAssembly, typestate refactors, `og:image` cards.
- Anything requiring a real candidate, a real mailbox, a lawyer or a signature.
- Anything that changes what the product claims: if a task would require softening an
  invariant, the task is wrong, not the invariant.

## 8. Go/no-go gate (run by a human, at the end, on a rehearsal workspace)

```bash
ledger --operator "<name>" log verify --data <pilot>
./verify_log.py <pilot>/send_log.jsonl
ledger --operator "<name>" verify-signatures --data <pilot>
ledger --operator "<name>" governance show --data <pilot>
ledger --operator "<name>" status --data <pilot>        # must print 13/13
ledger --operator "<name>" dispatch --preview --data <pilot> --requisition <REQ>
```

GO requires: 13/13 signed by named people; both verifiers agree on the same head; zero
`Invalid` signatures and no unreconciled `Stale`; every blocked row has a reason a person
accepts; a restore from the newest backup verified on a second machine; the escalation
phone answered a test message inside the SLA. Anything else is NO-GO with the failing
line quoted.

## 9. Reporting

Every iteration appends one line to `.plan/state.json` (`state.log`), and the run ends
with a report in this shape:

```text
PLAN: 9/19 done, 1 blocked, 3 awaiting human
SHIPPED: P0-1, P0-2, P0-3, P0-5, P1-4, P1-5, P2-3 (commits …)
BLOCKED: P1-6 after 2 attempts — <gate output, one line>
HUMAN: P0-4 (access review) · P0-6 (purge) · P0-7 (13 checklist items) · P1-3 (SLA) · P2-4 (branch protection) · P2-6 (design decision)
NEXT RUN: P1-1 (ready), P1-2 (needs P0-5), …
```

`node scripts/plan-status.mjs` produces the table; the report is written by the agent
that ran the loop.

## 10. When the loop stops on its own

`STOP=awaiting-human` is a **successful** end state for a machine: the remaining work is
signatures, a lawyer, a mailbox and a decision. `STOP=done` means every machine task is
finished. `STOP=blocked` means somebody has to read §9 and unblock a gate.

In all three cases: update this file's status line, commit, and tell the human exactly
what to sign next.

```text
STATUS 2026-11-25: 13/19 done · 0 blocked · 6 awaiting a person · 0 awaiting evidence
```

## 11. Risks the loop cannot remove

| Risk | Mitigation (already in the plan) |
| --- | --- |
| A gate answer contradicts the ATS export for a real candidate | breaker 3 blocks; a named reviewer handles it (`P0-7`) |
| A lost operator key | `P0-3` rotation and loss procedure |
| Chain corruption goes unnoticed | `P0-5` scheduled verification + off-host anchors |
| Retention purge surprises Legal | `P0-6` rehearsal and signature |
| The preview is mistaken for the product | every screen labels it; `P2-6` decides its future |
| Scope creep into a hosted service | §7, and the loop is forbidden from starting it |
