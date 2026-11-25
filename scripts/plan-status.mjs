#!/usr/bin/env node
/*
 * plan-status.mjs — the loop's dashboard.
 *
 *   node scripts/plan-status.mjs [--json]
 *
 * Prints every task with its class, status and dependency state, the completion
 * counts, and what stops the loop if nothing is ready. Read-only.
 */
import { readFileSync } from "node:fs";
import { execSync } from "node:child_process";

const root = execSync("git rev-parse --show-toplevel", { encoding: "utf8" }).trim();
const tasks = JSON.parse(readFileSync(`${root}/.plan/tasks.json`, "utf8"));
const state = JSON.parse(readFileSync(`${root}/.plan/state.json`, "utf8"));

const rows = tasks.tasks.map((t) => {
  const s = state.tasks[t.id] ?? { status: "todo", attempts: 0 };
  const unmet = (t.depends_on ?? []).filter((d) => (state.tasks[d]?.status ?? "todo") !== "done");
  return { id: t.id, title: t.title, class: t.class, status: s.status, attempts: s.attempts, commit: s.commit, unmet, gate: t.gate, size: t.size };
});

if (process.argv.includes("--json")) {
  console.log(JSON.stringify({ updated: state.updated, rows }, null, 2));
  process.exit(0);
}

const width = Math.max(...rows.map((r) => r.title.length));
const byStatus = {};
for (const r of rows) byStatus[r.status] = (byStatus[r.status] ?? 0) + 1;
const done = byStatus.done ?? 0;

console.log(`${done}/${rows.length} done · ${JSON.stringify(byStatus)} · updated ${state.updated}`);
console.log("");
for (const r of rows) {
  const mark = { done: "✓", todo: "·", doing: "›", blocked: "!", "awaiting-human": "⧗", superseded: "-" }[r.status] ?? "?";
  const dep = r.unmet.length ? ` wait:${r.unmet.join(",")}` : "";
  console.log(`${mark} ${r.id.padEnd(6)} ${r.class.padEnd(12)} ${r.status.padEnd(15)} ${r.title.padEnd(width)}  ${r.size}${dep}`);
}
console.log("");
if (state.log?.length) {
  console.log("last events:");
  for (const e of state.log.slice(0, 5)) console.log(`  ${e.at.slice(0, 16)} ${e.id} ${e.event}${e.commit ? ` ${e.commit}` : ""}${e.note ? ` — ${e.note}` : ""}`);
}
