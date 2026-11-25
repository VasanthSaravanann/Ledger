#!/usr/bin/env bash
# evidence-pack.sh — collect the artefacts an auditor asks for into one directory.
#
#   bash scripts/evidence-pack.sh                          # against a fresh fixture
#   bash scripts/evidence-pack.sh --data <workspace> [--operator <name>]
#   bash scripts/evidence-pack.sh --out <directory>
#
# Writes evidence/<date>/ and a copy of its index at evidence/index.json. Every
# collected file is named with its SHA-256 in index.json, so one directory answers
# "what did you verify, when, and against which bytes".
#
# What goes in:
#   chain-verify.txt        ledger log verify, text and the head
#   independent-verify.txt  verify_log.py on the same send log
#   signatures.txt          verify-signatures: the four verdicts, per artefact
#   purge-preview.json      purge --dry-run: what retention would tombstone, and when
#   restore-drill.txt       the restore drill's own output and timings
#   anchors/                every off-host receipt in the workspace
#   access-review.md        the signed access review, when it exists
#   release/                the release's .sha256 assets and their manifest
#
# No candidate names, addresses or message bodies are collected: the pack is about
# the chain and the process, not the people. Exit 0 pack written, 1 a check failed,
# 2 usage or tooling.

set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
WORK=$(mktemp -d "${TMPDIR:-/tmp}/ledger-evidence.XXXXXX")
trap 'rm -rf "$WORK"' EXIT

LEDGER=${LEDGER_BIN:-$ROOT/target/debug/ledger}
MODE="fixture"
DATA=""
OPERATOR=""
OUT=${OUT:-$ROOT/evidence}
RELEASE=${RELEASE:-v0.1.0}
DATE=$(date -u +%Y-%m-%d)
DIR="$OUT/$DATE"

usage() {
  sed -n '2,26p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
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
    --operator)
      [ $# -ge 2 ] || usage
      OPERATOR=$2
      shift 2
      ;;
    --out)
      [ $# -ge 2 ] || usage
      OUT=$2
      shift 2
      ;;
    --help | -h) usage ;;
    *)
      printf 'unknown argument: %s\n\n' "$1" >&2
      usage
      ;;
  esac
done

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

MISSING=()

if [ "$MODE" = "fixture" ]; then
  PILOT="$WORK/pilot"
  "$LEDGER" demo seed --data "$PILOT" --force --record >"$WORK/seed.out"
  WORKSPACE="fixture (seeded for this pack)"
else
  PILOT=$DATA
  [ -d "$PILOT" ] || {
    printf 'no such workspace: %s\n' "$PILOT" >&2
    exit 2
  }
  WORKSPACE=$PILOT
fi

if [ -z "$OPERATOR" ]; then
  OPERATOR=$("$LEDGER" operator list --data "$PILOT" --json | python3 -c '
import json, sys
operators = json.load(sys.stdin)["operators"]
print(next(operator["name"] for operator in operators if operator["role"] == "approver"))
')
fi

mkdir -p "$DIR"

# 1. The chain, by both implementations.
"$LEDGER" log verify --data "$PILOT" >"$DIR/chain-verify.txt"
"$ROOT/verify_log.py" "$PILOT/send_log.jsonl" >"$DIR/independent-verify.txt"

# 2. Signatures: the verdicts, whether or not they are clean. A red report is
#    evidence too, so the exit code is recorded rather than aborting the pack.
signatures_status=0
"$LEDGER" verify-signatures --data "$PILOT" >"$DIR/signatures.txt" 2>&1 \
  || signatures_status=$?

# 3. Retention, previewed: what would be tombstoned and what stays in the chain.
"$LEDGER" purge --dry-run --operator "$OPERATOR" --data "$PILOT" --json \
  >"$DIR/purge-preview.json"

# 4. Off-host receipts, if the pilot has published any.
if [ -d "$PILOT/anchors" ] && [ -n "$(ls -A "$PILOT/anchors" 2>/dev/null)" ]; then
  mkdir -p "$DIR/anchors"
  cp "$PILOT"/anchors/*.json "$DIR/anchors/"
else
  MISSING+=("anchors/: no receipt published yet (run ledger anchor on the pilot host)")
fi

# 5. The access review is a human act; copy it when signed, say so when not.
if [ -s "$ROOT/docs/access-review.md" ]; then
  cp "$ROOT/docs/access-review.md" "$DIR/access-review.md"
else
  MISSING+=("access-review.md: not signed yet (PRODUCTION.md P0-4)")
fi

# 6. Restore drill: a backup that has never been restored is a guess.
"$ROOT/scripts/restore-drill.sh" >"$DIR/restore-drill.txt" 2>&1 || {
  printf 'restore drill failed; see %s\n' "$DIR/restore-drill.txt" >&2
  exit 1
}

# 7. Release checksums, when gh is available and authenticated.
mkdir -p "$DIR/release"
if command -v gh >/dev/null && gh auth status >/dev/null 2>&1; then
  if gh release view "$RELEASE" --json tagName,assets,publishedAt >"$DIR/release/release.json" 2>/dev/null; then
    gh release download "$RELEASE" --pattern "*.sha256" --dir "$DIR/release" >/dev/null 2>&1 \
      || MISSING+=("release/$RELEASE: no .sha256 assets downloaded")
  else
    MISSING+=("release/$RELEASE: no such release")
  fi
else
  MISSING+=("release/: gh is unavailable or not authenticated")
fi

# 8. The index: every file, its size and its SHA-256.
python3 - "$DIR" "$WORKSPACE" "$DIR/signatures.txt" "$signatures_status" "${MISSING[@]}" <<'PY'
import datetime, hashlib, json, os, sys

directory, workspace, signatures_path, signatures_status = sys.argv[1:5]
missing = sys.argv[5:]
files = []
for root, _, names in os.walk(directory):
    for name in sorted(names):
        path = os.path.join(root, name)
        if os.path.basename(path) == "index.json":
            continue
        with open(path, "rb") as handle:
            digest = hashlib.sha256(handle.read()).hexdigest()
        files.append(
            {
                "file": os.path.relpath(path, directory),
                "bytes": os.path.getsize(path),
                "sha256": digest,
            }
        )

def line(path, prefix):
    with open(path) as handle:
        for text in handle.read().splitlines():
            if text.startswith(prefix):
                return text[len(prefix):].strip()
    return None

index = {
    "generated_at": datetime.datetime.now(datetime.timezone.utc)
    .replace(microsecond=0)
    .isoformat()
    .replace("+00:00", "Z"),
    "workspace": workspace,
    "directories": {"dated": os.path.basename(directory), "root": directory},
    "chain": {
        "entries": line(os.path.join(directory, "chain-verify.txt"), "send log verified: "),
        "head": line(os.path.join(directory, "chain-verify.txt"), "head: "),
        "independent_head": line(
            os.path.join(directory, "independent-verify.txt"), "head: "
        ),
        "signatures_exit": int(signatures_status),
        "signed_records": line(signatures_path, "signed records: "),
    },
    "notes": missing,
    "files": files,
}
with open(os.path.join(directory, "index.json"), "w") as handle:
    json.dump(index, handle, indent=2)
    handle.write("\n")
print(f"{len(files)} file(s) indexed · head {index['chain']['head']}")
PY

cp "$DIR/index.json" "$OUT/index.json"

printf 'pack:  %s\n' "$DIR"
printf 'index: %s\n' "$OUT/index.json"
