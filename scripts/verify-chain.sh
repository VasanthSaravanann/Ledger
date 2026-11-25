#!/usr/bin/env bash
# verify-chain.sh — the scheduled alarm: verify a workspace and exit non-zero if
# anything about it stopped being true.
#
#   bash scripts/verify-chain.sh --data <workspace>     # the real check
#   bash scripts/verify-chain.sh --fixture              # seed, copy, verify the copy
#   bash scripts/verify-chain.sh --fixture-tampered     # the same, then break it
#   bash scripts/verify-chain.sh --fixture --tamper tear|signature|value
#
# Every mode runs the same four checks and requires them to agree:
#
#   1. `ledger log verify`        — chain, sequence, digests, time order
#   2. `verify_log.py`            — the independent implementation of the same rules
#   3. `ledger verify-signatures` — dispositions and every signed artefact
#   4. `ledger delivery list`     — the delivery-confirmation chain, when present
#
# Exit 0 every check passed · 1 a check failed (the alarm) · 2 usage or tooling.
#
# The clean and tampered fixture modes exist so the alarm's failure path can be
# exercised on a machine with no pilot data; that is what CI runs daily.

set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
WORK=$(mktemp -d "${TMPDIR:-/tmp}/ledger-verify.XXXXXX")
trap 'rm -rf "$WORK"' EXIT

LEDGER=${LEDGER_BIN:-$ROOT/target/debug/ledger}
MODE=""
DATA=""
TAMPER="value"
TAMPER_SET=""

usage() {
  sed -n '2,20p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
  exit 2
}

while [ $# -gt 0 ]; do
  case $1 in
    --data)
      [ $# -ge 2 ] || usage
      MODE="data"
      DATA=$2
      shift 2
      ;;
    --fixture)
      MODE=${MODE:-fixture}
      shift
      ;;
    --fixture-tampered)
      MODE="tampered"
      shift
      ;;
    --tamper)
      [ $# -ge 2 ] || usage
      TAMPER=$2
      TAMPER_SET=1
      shift 2
      ;;
    --help | -h)
      usage
      ;;
    *)
      printf 'unknown argument: %s\n\n' "$1" >&2
      usage
      ;;
  esac
done

# `--tamper` turns a fixture run into the failure path, whichever flag came first.
if [ -n "$TAMPER_SET" ]; then
  case $MODE in
    "" | fixture) MODE="tampered" ;;
  esac
fi

[ -n "$MODE" ] || usage

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
[ -x "$LEDGER" ] || {
  printf 'tooling: ledger binary missing at %s\n' "$LEDGER" >&2
  exit 2
}

alarm() {
  printf 'ALARM %s\n' "$1" >&2
}

# Edits exactly one covered value, so the chain must fail at the first entry.
tamper_value() {
  python3 - "$1/send_log.jsonl" <<'PY'
import json, sys

path = sys.argv[1]
with open(path) as handle:
    lines = handle.read().splitlines()
first = json.loads(lines[0])
first["subject"] = "Edited after the fact"
lines[0] = json.dumps(first, separators=(",", ":"))
with open(path, "w") as handle:
    handle.write("\n".join(lines) + "\n")
PY
}

# Leaves a partial line, the shape of a crash mid-append.
tamper_tear() {
  printf '{"seq":0,' >>"$1/send_log.jsonl"
}

# Changes a signed disposition without touching its signature, so the signature
# no longer covers the record and `verify-signatures` must fail.
tamper_signature() {
  python3 - "$1/dispositions.jsonl" <<'PY'
import json, sys

path = sys.argv[1]
with open(path) as handle:
    lines = [line for line in handle.read().splitlines() if line.strip()]
target = None
for index, line in enumerate(lines):
    record = json.loads(line)
    if record.get("signature"):
        target = index
        break
if target is None:
    raise SystemExit("no signed disposition to tamper with")
record = json.loads(lines[target])
record["candidate_first_name"] = "Tampered"
lines[target] = json.dumps(record, separators=(",", ":"))
with open(path, "w") as handle:
    handle.write("\n".join(lines) + "\n")
PY
}

verify_workspace() {
  local dir=$1
  local failed=0
  local ledger_out="" independent_out="" signatures_out="" delivery_out=""

  if ! ledger_out=$("$LEDGER" log verify --data "$dir" 2>&1); then
    alarm "ledger log verify failed for $dir:"$'\n'"$ledger_out"
    failed=1
  fi
  if ! independent_out=$("$ROOT/verify_log.py" "$dir/send_log.jsonl" 2>&1); then
    alarm "verify_log.py failed for $dir:"$'\n'"$independent_out"
    failed=1
  fi
  if ! signatures_out=$("$LEDGER" verify-signatures --data "$dir" 2>&1); then
    alarm "verify-signatures failed for $dir:"$'\n'"$signatures_out"
    failed=1
  fi
  if ! delivery_out=$("$LEDGER" delivery list --data "$dir" 2>&1); then
    alarm "the delivery chain failed to verify for $dir:"$'\n'"$delivery_out"
    failed=1
  fi

  if [ "$failed" -eq 0 ]; then
    local ledger_head independent_head
    ledger_head=$(printf '%s\n' "$ledger_out" | sed -n 's/^head: //p')
    independent_head=$(printf '%s\n' "$independent_out" | sed -n 's/^head: //p')
    if [ -z "$ledger_head" ] || [ "$ledger_head" != "$independent_head" ]; then
      alarm "the two verifiers disagree on the head: ledger=${ledger_head:-none} independent=${independent_head:-none}"
      failed=1
    fi
  fi

  if [ "$failed" -ne 0 ]; then
    printf 'ALARM chain verification FAILED for %s\n' "$dir" >&2
    return 1
  fi

  local entries signed
  entries=$(printf '%s\n' "$ledger_out" | sed -n 's/^send log verified: \([0-9]*\) entries/\1/p')
  signed=$(printf '%s\n' "$signatures_out" | sed -n 's/^signed records:[[:space:]]*//p')
  printf 'OK verify-chain: %s entries · head %s · signatures %s · independent verifier agrees\n' \
    "${entries:-0}" "${ledger_head:0:8}" "${signed:-0}"
}

case $MODE in
  data)
    [ -d "$DATA" ] || {
      printf 'no such workspace: %s\n' "$DATA" >&2
      exit 2
    }
    verify_workspace "$DATA"
    ;;
  fixture | tampered)
    PILOT="$WORK/pilot"
    RESTORED="$WORK/restored"
    # A scheduled job has no business writing to the live workspace, so it checks a
    # restored copy — the same bytes on a machine that only has the backup.
    "$LEDGER" demo seed --data "$PILOT" --force --record >"$WORK/seed.out"
    mkdir -p "$RESTORED"
    cp -a "$PILOT/." "$RESTORED/"
    if [ "$MODE" = "tampered" ]; then
      case $TAMPER in
        value) tamper_value "$RESTORED" ;;
        tear) tamper_tear "$RESTORED" ;;
        signature) tamper_signature "$RESTORED" ;;
        *)
          printf 'unknown tamper kind: %s\n' "$TAMPER" >&2
          exit 2
          ;;
      esac
      # `--fixture-tampered` is the failure path itself: verification must exit
      # non-zero, and the check that the alarm is live is `! … --fixture-tampered`.
      verify_workspace "$RESTORED"
    else
      verify_workspace "$RESTORED"
    fi
    ;;
esac
