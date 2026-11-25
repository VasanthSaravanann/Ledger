#!/usr/bin/env bash
# restore-drill.sh — prove a backup can be restored and still verifies.
#
#   bash scripts/restore-drill.sh
#
# Seeds the deterministic fixture, backs the whole workspace up, restores it into an
# empty directory, and asserts that nothing observable changed:
#
#   1. `ledger log verify` prints the same entries and head;
#   2. `ledger verify-signatures` prints the same report and exits 0;
#   3. the fixture fingerprint, recomputed here, is unchanged;
#   4. `verify_log.py` — the independent verifier — agrees on the same head;
#   5. a single edited value is still detected, so a broken verifier cannot pass.
#
# It prints the wall-clock milliseconds each step took. Exit 0 all checks passed,
# 1 a check failed, 2 the tools could not run.

set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
WORK=$(mktemp -d "${TMPDIR:-/tmp}/ledger-restore.XXXXXX")
trap 'rm -rf "$WORK"' EXIT

PILOT="$WORK/pilot"
BACKUP="$WORK/backup"
RESTORED="$WORK/restored"
TAMPERED="$WORK/tampered"

fail() {
  printf 'FAIL %s\n' "$*" >&2
  exit 1
}

now_ms() {
  python3 -c 'import time; print(int(time.time() * 1000))'
}

# Mirrors `demo::fingerprint`: SHA-256 over "<name>:<sha256(file)>\n" for every
# top-level file except the summary and the lock, name-sorted.
fingerprint() {
  python3 - "$1" <<'PY'
import hashlib, os, sys

root = sys.argv[1]
skip = {"demo-summary.json", ".lock"}
combined = b""
for name in sorted(os.listdir(root)):
    path = os.path.join(root, name)
    if not os.path.isfile(path) or name in skip:
        continue
    with open(path, "rb") as handle:
        digest = hashlib.sha256(handle.read()).hexdigest()
    combined += f"{name}:{digest}\n".encode()
print(hashlib.sha256(combined).hexdigest())
PY
}

command -v python3 >/dev/null || fail "python3 is required"

printf 'building ledger…\n'
cargo build --quiet --locked --manifest-path "$ROOT/Cargo.toml"
LEDGER="$ROOT/target/debug/ledger"
[ -x "$LEDGER" ] || fail "ledger binary missing at $LEDGER"

# 1. Seed the fixture. `--record` signs dispositions and copy with registered keys,
#    so verify-signatures has something real to check.
t0=$(now_ms)
"$LEDGER" demo seed --data "$PILOT" --force --record >"$WORK/seed.out"
seed_ms=$(($(now_ms) - t0))

head_before=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["log_head"])' "$PILOT/demo-summary.json")
finger_before=$(fingerprint "$PILOT")
"$LEDGER" log verify --data "$PILOT" >"$WORK/verify-seed.out"
"$LEDGER" verify-signatures --data "$PILOT" >"$WORK/signatures-seed.out"

# 2. Back up every file, including subdirectories such as operators/.
t0=$(now_ms)
mkdir -p "$BACKUP"
cp -a "$PILOT/." "$BACKUP/"
backup_ms=$(($(now_ms) - t0))

# 3. Restore into an empty directory: the only thing a fresh machine would have.
t0=$(now_ms)
mkdir -p "$RESTORED"
cp -a "$BACKUP/." "$RESTORED/"
restore_ms=$(($(now_ms) - t0))

# 4. Assert the restored workspace is indistinguishable where it matters.
t0=$(now_ms)
"$LEDGER" log verify --data "$RESTORED" >"$WORK/verify-restored.out"
cmp -s "$WORK/verify-seed.out" "$WORK/verify-restored.out" \
  || fail "log verify differs after restore:
$(diff -u "$WORK/verify-seed.out" "$WORK/verify-restored.out" || true)"
grep -q "^head: $head_before$" "$WORK/verify-restored.out" \
  || fail "restored head is not $head_before"

"$LEDGER" verify-signatures --data "$RESTORED" >"$WORK/signatures-restored.out"
cmp -s "$WORK/signatures-seed.out" "$WORK/signatures-restored.out" \
  || fail "verify-signatures differs after restore:
$(diff -u "$WORK/signatures-seed.out" "$WORK/signatures-restored.out" || true)"

finger_after=$(fingerprint "$RESTORED")
[ "$finger_after" = "$finger_before" ] \
  || fail "fingerprint moved: $finger_before -> $finger_after"

"$ROOT/verify_log.py" "$RESTORED/send_log.jsonl" >"$WORK/independent.out"
grep -q "^head: $head_before$" "$WORK/independent.out" \
  || fail "verify_log.py disagrees on the head: $(cat "$WORK/independent.out")"
verify_ms=$(($(now_ms) - t0))

# 5. Negative control: the drill must be able to fail. Edit one covered value.
t0=$(now_ms)
cp -a "$RESTORED" "$TAMPERED"
python3 - "$TAMPERED/send_log.jsonl" <<'PY'
import json, sys

path = sys.argv[1]
with open(path) as handle:
    lines = handle.read().splitlines()
first = json.loads(lines[0])
first["subject"] = "Tampered after restore"
lines[0] = json.dumps(first, separators=(",", ":"))
with open(path, "w") as handle:
    handle.write("\n".join(lines) + "\n")
PY
if "$LEDGER" log verify --data "$TAMPERED" >"$WORK/tampered.out" 2>&1; then
  fail "an edited send log still verified; the check is not live"
fi
tamper_ms=$(($(now_ms) - t0))

entries=$(grep -c . "$RESTORED/send_log.jsonl")
signed=$(sed -n 's/^signed records:[[:space:]]*//p' "$WORK/signatures-restored.out")

printf '\n'
printf 'seed          %6d ms\n' "$seed_ms"
printf 'backup        %6d ms\n' "$backup_ms"
printf 'restore       %6d ms\n' "$restore_ms"
printf 'verify all    %6d ms\n' "$verify_ms"
printf 'tamper check  %6d ms\n' "$tamper_ms"
printf '\n'
printf 'OK restore drill: %s entries · head %s · fingerprint %s (this run) · signatures %s\n' \
  "$entries" "${head_before:0:8}" "${finger_after:0:8}" "$signed"
