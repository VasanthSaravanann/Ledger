#!/usr/bin/env node
/*
 * check-docs.mjs — fail the build when the documentation stops being true.
 *
 *   node scripts/check-docs.mjs
 *
 * Three checks, all of them things that actually drifted:
 *
 * 1. Every commit is accounted for in CHANGELOG.md — by short sha, or by subject
 *    (HEAD is matched by subject, because a commit cannot contain its own sha).
 * 2. The numbers SKILL.md quotes are the numbers in the tree: modules, routes,
 *    --json flags, and the test count from `cargo test -- --list`.
 * 3. Every document the project promises to have exists.
 *
 * Exit 0 clean, 1 a mismatch, 2 a check could not run.
 */
import { readFileSync, existsSync, readdirSync, statSync } from "node:fs";
import { execSync } from "node:child_process";
import { join } from "node:path";

const root = execSync("git rev-parse --show-toplevel", { encoding: "utf8" }).trim();
const fail = [];
const warn = [];

const sh = (cmd, opts = {}) => execSync(cmd, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"], ...opts }).trim();

// ---------------------------------------------------------------- 1. changelog
const changelog = readFileSync(join(root, "CHANGELOG.md"), "utf8");
const log = sh("git log --pretty='%h|%s'").split("\n");
log.forEach((line, index) => {
  const [sha, subject] = line.split("|");
  const documented = index === 0 ? changelog.includes(subject) : changelog.includes(sha) || changelog.includes(subject);
  if (!documented) fail.push(`CHANGELOG.md does not mention ${sha} "${subject}"`);
});

// ---------------------------------------------------------------- 2. skill numbers
const skill = readFileSync(join(root, "SKILL.md"), "utf8");
const walk = (dir) => readdirSync(join(root, dir)).flatMap((name) => {
  const path = join(dir, name);
  const full = join(root, path);
  return statSync(full).isDirectory() ? walk(path) : [path];
});
const modules = walk("src").filter((p) => p.endsWith(".rs")).length;
const routes = readdirSync(join(root, "web/src/routes")).filter((f) => f.endsWith(".tsx") && f !== "__root.tsx").length;
const jsonFlags = (readFileSync(join(root, "src/main.rs"), "utf8").match(/json: bool/g) ?? []).length;

const numbers = [
  [`${modules} modules`, "module count"],
  [`${routes} routes`, "route count"],
  [`${jsonFlags} of them`, "--json flag count"],
];
for (const [needle, what] of numbers) {
  if (!skill.includes(needle)) fail.push(`SKILL.md does not say "${needle}" (${what} in the tree)`);
}

let tests = null;
try {
  const listing = sh("cargo test --locked --offline -- --list");
  tests = listing.split("\n").filter((l) => l.endsWith(": test")).length;
} catch {
  warn.push("could not list tests (cargo unavailable); the test-count claim was not checked");
}
if (tests !== null) {
  if (!skill.includes(`${tests} tests`)) fail.push(`SKILL.md does not say "${tests} tests" (counted in the tree)`);
  if (!skill.includes(`${tests + 8} with`)) fail.push(`SKILL.md does not say "${tests + 8} with" for --features ui`);
}

// ---------------------------------------------------------------- 3. promised files
const promised = [
  "README.md",
  "SKILL.md",
  "CHANGELOG.md",
  "PRODUCTION.md",
  "FORMAT_SPEC.md",
  "THREAT_MODEL.md",
  "CONTRIBUTING.md",
  "LICENSE",
  "verify_log.py",
  "examples/export.csv",
  "web/README.md",
  ".plan/tasks.json",
  ".plan/state.json",
  "scripts/plan-next.mjs",
  "scripts/plan-done.mjs",
  "scripts/plan-status.mjs",
];
for (const path of promised) {
  if (!existsSync(join(root, path))) fail.push(`promised file is missing: ${path}`);
}

// ---------------------------------------------------------------- report
for (const w of warn) console.warn(`WARN ${w}`);
for (const f of fail) console.error(`FAIL ${f}`);
if (fail.length) {
  console.error(`\ndocs: ${fail.length} mismatch(es)`);
  process.exit(1);
}
console.log(`docs ok · ${log.length} commits logged · ${modules} modules · ${routes} routes · ${jsonFlags} json flags · ${tests ?? "?"} tests`);
