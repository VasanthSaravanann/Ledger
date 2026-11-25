#!/usr/bin/env bash
# dsar-drill.sh — walk the data-subject request path on the fixture.
#
#   bash scripts/dsar-drill.sh
#
# Answers the question "what do you hold about me?" the way an operator would:
#
#   1. resolve a redacted handle with `find --hash`;
#   2. export the sends for one requisition (`log export`);
#   3. preview retention (`purge --dry-run`), and separate what is erasable from
#      what is only deletable with the whole workspace.
#
# It writes nothing outside a temporary directory. Exit 0 when the drill ran; 1 when a
# step failed; 2 when a tool is missing.

set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
WORK=$(mktemp -d "${TMPDIR:-/tmp}/ledger-dsar.XXXXXX")
trap 'rm -rf "$WORK"' EXIT

LEDGER=${LEDGER_BIN:-$ROOT/target/debug/ledger}
APPLICATION=${APPLICATION:-APP-1001}
PILOT="$WORK/pilot"
EXPORT="$WORK/dsar-export.csv"

fail() {
  printf 'FAIL %s\n' "$*" >&2
  exit 1
}

command -v python3 >/dev/null || {
  printf 'tooling: python3 is required\n' >&2
  exit 2
}

if [ ! -x "$LEDGER" ]; then
  cargo build --quiet --locked --manifest-path "$ROOT/Cargo.toml" || {
    printf 'tooling: cannot build ledger\n' >&2
    exit 2
  }
fi

"$LEDGER" demo seed --data "$PILOT" --force --record >"$WORK/seed.out"
REQUISITION=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["requisition"])' "$PILOT/demo-summary.json")

# redact::id: '#' + the first eight hex of SHA-256(lowercased, trimmed value).
HANDLE=$(python3 - "$APPLICATION" <<'PY'
import hashlib, sys
print("#" + hashlib.sha256(sys.argv[1].strip().lower().encode()).hexdigest()[:8])
PY
)

printf 'fixture      %s (%s)\n' "$PILOT" "$REQUISITION"
printf 'subject      %s\n' "$APPLICATION"
printf 'handle       %s\n\n' "$HANDLE"

printf -- '--- find --hash %s ---\n' "$HANDLE"
find_out=$("$LEDGER" find --hash "$HANDLE" --data "$PILOT" --json 2>&1) || fail "find --hash failed"
printf '%s\n\n' "$find_out"

printf -- '--- log export --requisition %s ---\n' "$REQUISITION"
"$LEDGER" log export --requisition "$REQUISITION" --out "$EXPORT" --data "$PILOT" >/dev/null \
  || fail "log export failed"
read -r total_rows subject_rows < <(python3 - "$EXPORT" "$APPLICATION" <<'PY'
import csv, sys
with open(sys.argv[1], newline="") as handle:
    rows = list(csv.DictReader(handle))
mine = [row for row in rows if row["APPLICATION_ID"] == sys.argv[2]]
print(len(rows), len(mine))
PY
)
printf 'wrote %s\n' "$EXPORT"
printf '%s of %s send row(s) belong to %s\n\n' "$subject_rows" "$total_rows" "$APPLICATION"

# `purge` is a mutating command even when it only previews: it takes the workspace
# lock and records who asked. Use an approver the fixture registered.
OPERATOR=${OPERATOR:-$(python3 -c '
import json, sys
operators = json.loads(sys.argv[1])["operators"]
print(next(operator["name"] for operator in operators if operator["role"] == "approver"))
' "$("$LEDGER" operator list --data "$PILOT" --json)")}

printf -- '--- purge --dry-run ---\n'
"$LEDGER" purge --dry-run --operator "$OPERATOR" --data "$PILOT" >"$WORK/purge.out" \
  || fail "purge --dry-run failed"
cat "$WORK/purge.out"
printf '\n'

DISPATCHED=$(python3 -c 'import json,sys; print(len(json.loads(sys.argv[1])["matches"]))' "$find_out")

printf -- '--- the answer ---\n'
printf 'erasable in place (tombstoned once retention allows): %s disposition record(s)\n' "$DISPATCHED"
printf 'erasable in place: ATS roster rows, via `purge --yes`\n'
printf 'only deletable with the workspace: %s send-log entr%s (the chain is the evidence)\n' \
  "$subject_rows" "$([ "$subject_rows" = "1" ] && echo y || echo ies)"
printf 'retained for: the policy retention window (see the cutoff above)\n\n'
printf 'OK dsar drill: an export plus, at end of retention, workspace deletion — never an edited chain\n'
