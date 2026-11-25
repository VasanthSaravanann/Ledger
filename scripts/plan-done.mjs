#!/usr/bin/env node
/*
 * plan-done.mjs — the only writer of .plan/state.json.
 *
 *   node scripts/plan-done.mjs <id>                 run the task's gate; mark done on exit 0
 *   node scripts/plan-done.mjs <id> --signed-by "M. Okafor"
 *                                                  close a human / human-gated task; requires its
 *                                                  evidence file to exist and records the signer
 *   node scripts/plan-done.mjs <id> --fail "reason"  record a failed attempt
 *
 * Rules the script enforces, because a loop is only as honest as its guard rails:
 *   - a gate that fails never marks a task done (no --force exists);
 *   - a task of class `human` or `human-gated` cannot be closed without --signed-by,
 *     so the agent cannot sign for a person;
 *   - two failed attempts mark the task `blocked` instead of burning the loop on it.
 *
 * Exit: 0 changed, 1 gate failed, 2 usage or broken state, 3 refused (human task).
 */
import { readFileSync, writeFileSync } from "node:fs";
import { execSync } from "node:child_process";

const root = execSync("git rev-parse --show-toplevel", { encoding: "utf8" }).trim();
const tasks = JSON.parse(readFileSync(`${root}/.plan/tasks.json`, "utf8"));
const statePath = `${root}/.plan/state.json`;
const state = JSON.parse(readFileSync(statePath, "utf8"));

const [id, ...rest] = process.argv.slice(2);
const signedByIndex = rest.indexOf("--signed-by");
const signedBy = signedByIndex >= 0 ? rest[signedByIndex + 1] : null;
const failIndex = rest.indexOf("--fail");
const failReason = failIndex >= 0 ? rest[failIndex + 1] : null;

if (!id) {
  console.error("usage: plan-done.mjs <id> [--signed-by <name> | --fail <reason>]");
  process.exit(2);
}
const task = tasks.tasks.find((t) => t.id === id);
if (!task) {
  console.error(`no such task: ${id}`);
  process.exit(2);
}

const now = new Date().toISOString();
const entry = state.tasks[id] ?? (state.tasks[id] = { status: "todo", attempts: 0, commit: null, note: null });
entry.attempts ??= 0;
const save = () => {
  state.updated = now;
  writeFileSync(statePath, JSON.stringify(state, null, 2) + "\n");
};

if (failReason) {
  entry.attempts += 1;
  entry.note = failReason;
  entry.status = entry.attempts >= 2 ? "blocked" : "todo";
  state.log.unshift({ at: now, id, event: entry.status === "blocked" ? "blocked" : "attempt-failed", note: failReason });
  save();
  console.log(`${id}: attempt ${entry.attempts} failed → ${entry.status}`);
  console.log(entry.status === "blocked" ? "BLOCKED=1" : "RETRY=1");
  process.exit(0);
}

if (task.class !== "auto" && !signedBy) {
  console.error(`${id} is class "${task.class}": it cannot be closed without --signed-by "<person>".`);
  console.error(`evidence file: ${task.evidence ?? "(none recorded)"}`);
  process.exit(3);
}

if (task.class !== "auto") {
  const evidence = task.evidence ? `${root}/${task.evidence}` : null;
  if (!evidence) {
    console.error(`${id} has no evidence file declared; add one to .plan/tasks.json before signing.`);
    process.exit(2);
  }
  try {
    readFileSync(evidence);
  } catch {
    console.error(`${id}: evidence file is missing: ${task.evidence}`);
    process.exit(3);
  }
}

// A human-signed task still runs its gate, so a stale artefact cannot be signed twice.
const gate = task.gate;
console.log(`gate: ${gate}`);
let gateOk = true;
try {
  execSync(`set -o pipefail; ${gate}`, { cwd: root, stdio: "inherit", shell: "/bin/bash" });
} catch {
  gateOk = false;
}

entry.attempts += 1;
if (!gateOk) {
  entry.status = entry.attempts >= 2 ? "blocked" : "todo";
  entry.note = "gate failed";
  state.log.unshift({ at: now, id, event: "gate-failed", note: "gate exited non-zero" });
  save();
  console.log(`GATE=FAIL ${id} → ${entry.status}`);
  process.exit(1);
}

let commit = null;
try {
  commit = execSync("git rev-parse --short HEAD", { encoding: "utf8" }).trim();
} catch {
  commit = null;
}
entry.status = "done";
entry.commit = commit;
entry.note = signedBy ? `signed by ${signedBy}` : "gate green";
entry.signed_by = signedBy ?? entry.signed_by ?? null;
state.log.unshift({ at: now, id, event: "done", commit, signed_by: entry.signed_by });
save();
console.log(`DONE=${id} commit=${commit ?? "?"}${signedBy ? ` signed_by=${signedBy}` : ""}`);

const remaining = tasks.tasks.filter((t) => state.tasks[t.id]?.status !== "done");
console.log(`REMAINING=${remaining.length}`);
