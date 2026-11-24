#!/usr/bin/env bash
# Regenerates the artefact pages from a real CLI run, then leaves `site/` ready
# for Vercel. The fixture is deterministic, so the output is reproducible.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

./target/release/ledger demo seed --data "$work/pilot" --force --record

# Provenance is derived from the commit, not the clock, so two runs of this script
# on the same commit are byte-identical and the CI drift check stays meaningful.
export LEDGER_BUILD_SHA="${LEDGER_BUILD_SHA:-$(git rev-parse --short HEAD 2>/dev/null || echo dev)}"
export LEDGER_GENERATED_AT="${LEDGER_GENERATED_AT:-$(git log -1 --format=%cI 2>/dev/null || echo 1970-01-01T00:00:00Z)}"

# Capture the CLI surface for /cli, straight from the binary, before the export.
{
  ./target/release/ledger --help
  echo
  for command in init demo status governance gate requisition templates playbook \
                 ats disposition qa dispatch log reply signal metrics review \
                 checklist purge migrate anchor find; do
    echo "\$ ledger $command --help"
    ./target/release/ledger "$command" --help 2>&1 || true
    echo
  done
} > site/cli.txt

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
