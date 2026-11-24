#!/usr/bin/env bash
# Regenerates the artefact pages from a real CLI run, then leaves `site/` ready
# for Vercel. The fixture is deterministic, so the output is reproducible.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

./target/release/ledger demo seed --data "$work/pilot" --force --record
./target/release/ledger dispatch --data "$work/pilot" --requisition REQ-2026-014 \
  --preview --export-html site

# Optional: a real terminal cast for the appendix, when asciinema is installed.
if command -v asciinema >/dev/null; then
  LEDGER="$PWD/target/release/ledger" DATA="$work/cast" \
    asciinema rec --overwrite --quiet --command "bash scripts/demo-walkthrough.sh" site/demo.cast
  echo "wrote site/demo.cast"
else
  echo "asciinema not installed: the transcript on /demo is generated instead"
fi
