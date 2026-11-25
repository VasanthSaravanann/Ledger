#!/usr/bin/env node
/*
 * plan-next.mjs — print the one task the loop should do now.
 *
 *   node scripts/plan-next.mjs [--json]
 *
 * Reads .plan/tasks.json (the backlog) and .plan/state.json (what has happened).
 * A task is ready when it is `todo`, every dependency is `done`, and its class is
 * `auto` (the only class the loop may execute alone). Lowest priority, then id.
 *
 * Exit 0 with TASK=<id> when there is work, 0 with STOP=<reason> when there is none,
 * 2 on a broken state. Never mutates anything: `plan-done.mjs` is the only writer.
 */
import { readFileSync } from "node:fs";
import { execSync } from "node:child_process";

const root = execSync("git rev-parse --show-toplevel", { encoding: "utf8" }).trim();
const tasks = JSON.parse(readFileSync(`${root}/.plan/tasks.json`, "utf8"));
const state = JSON.parse(readFileSync(`${root}/.plan/state.json`, "utf8"));
const asJson = process.argv.includes("--json");

const byId = new Map(tasks.tasks.map((t) => [t.id, t]));
for (const id of Object.keys(state.tasks)) {
  if (!byId.has(id)) {
    console.error(`state has a task that the backlog does not: ${id}`);
    process.exit(2);
  }
}

const status = (id) => state.tasks[id]?.status ?? "todo";
const unmet = (t) => (t.depends_on ?? []).filter((d) => status(d) !== "done");
const ready = tasks.tasks
  .filter((t) => status(t.id) === "todo" && t.class === "auto" && unmet(t).length === 0)
  .sort((a, b) => a.priority - b.priority || a.id.localeCompare(b.id));

const counts = { done: 0, todo: 0, doing: 0, blocked: 0, "awaiting-human": 0, superseded: 0 };
for (const t of tasks.tasks) counts[status(t.id)] = (counts[status(t.id)] ?? 0) + 1;

if (ready.length) {
  const t = ready[0];
  if (asJson) {
    console.log(JSON.stringify({ task: t, counts, remaining: ready.length - 1 }, null, 2));
  } else {
    console.log(`TASK=${t.id}`);
    console.log(`TITLE=${t.title}`);
    console.log(`SIZE=${t.size}`);
    console.log(`WHY=${t.why}`);
    console.log(`ACCEPT=${t.accept}`);
    console.log(`FILES=${(t.files ?? []).join(",")}`);
    console.log(`GATE=${t.gate}`);
    console.log(`READY_AFTER_THIS=${ready.length - 1}`);
  }
  process.exit(0);
}

const awaiting = tasks.tasks.filter((t) => ["todo", "awaiting-human"].includes(status(t.id)) && t.class !== "auto");
const blocked = tasks.tasks.filter((t) => status(t.id) === "blocked");
const reason = blocked.length
  ? "blocked"
  : awaiting.length
    ? "awaiting-human"
    : counts.done === tasks.tasks.length
      ? "done"
      : "waiting-on-dependencies";

if (asJson) {
  console.log(JSON.stringify({ stop: reason, counts, awaiting: awaiting.map((t) => t.id), blocked: blocked.map((t) => t.id) }, null, 2));
} else {
  console.log(`STOP=${reason}`);
  console.log(`DONE=${counts.done}/${tasks.tasks.length}`);
  if (awaiting.length) console.log(`HUMAN=${awaiting.map((t) => `${t.id} (${t.class})`).join(", ")}`);
  if (blocked.length) console.log(`BLOCKED=${blocked.map((t) => t.id).join(", ")}`);
  console.log("NO_AUTO_WORK=1");
}
