#!/usr/bin/env bash
# The scripted run V3 asks for: seed, preview, blocked cases, QA, dispatch,
# verify, one-byte tamper, verify again. Never a live demo.
set -euo pipefail

LEDGER="${LEDGER:-./target/release/ledger}"
DATA="${DATA:-$(mktemp -d)/pilot}"
REQ="REQ-2026-014"

echo "\$ ledger demo seed --data pilot"
"$LEDGER" demo seed --data "$DATA" --force

echo
echo "\$ ledger dispatch --preview --requisition $REQ"
"$LEDGER" dispatch --data "$DATA" --requisition "$REQ" --preview

echo
echo "\$ ledger qa record --requisition $REQ --application APP-1150 --qa qa-patel"
"$LEDGER" qa record --data "$DATA" --requisition "$REQ" --application APP-1150 --qa qa-patel || true

echo
echo "\$ ledger dispatch --requisition $REQ"
"$LEDGER" dispatch --data "$DATA" --requisition "$REQ"

echo
echo "\$ ledger log verify"
"$LEDGER" log verify --data "$DATA"

echo
echo "\$ printf 'X' | dd of=send_log.jsonl bs=1 seek=120 count=1 conv=notrunc"
printf 'X' | dd of="$DATA/send_log.jsonl" bs=1 seek=120 count=1 conv=notrunc 2>/dev/null

echo
echo "\$ ledger log verify"
"$LEDGER" log verify --data "$DATA" || true
