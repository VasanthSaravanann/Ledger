#!/usr/bin/env bash
# Regenerates the artefact pages from a real CLI run, then leaves `site/` ready
# for Vercel. The fixture is deterministic, so the output is reproducible.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release
cargo build --release --features ui
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

./target/release/ledger demo seed --data "$work/pilot" --force --record

# Provenance is an *input*, not something read from HEAD: pages committed at commit
# N would otherwise always regenerate as N+1 and the CI drift check could never
# pass. `site/provenance.json` records the commit the pages were generated from;
# bump it deliberately with LEDGER_BUMP_PROVENANCE=1 when the pages change.
if [ "${LEDGER_BUMP_PROVENANCE:-0}" = "1" ] || [ ! -f site/provenance.json ]; then
  build_sha="${LEDGER_BUILD_SHA:-$(git rev-parse --short HEAD 2>/dev/null || echo dev)}"
  generated_at="${LEDGER_GENERATED_AT:-$(git log -1 --format=%cI 2>/dev/null || echo 1970-01-01T00:00:00Z)}"
  printf '{\n  "build_sha": "%s",\n  "generated_at": "%s"\n}\n' \
    "$build_sha" "$generated_at" > site/provenance.json
fi
export LEDGER_BUILD_SHA="${LEDGER_BUILD_SHA:-$(sed -n 's/.*"build_sha": "\([^"]*\)".*/\1/p' site/provenance.json)}"
export LEDGER_GENERATED_AT="${LEDGER_GENERATED_AT:-$(sed -n 's/.*"generated_at": "\([^"]*\)".*/\1/p' site/provenance.json)}"

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

./target/release/ledger --operator qa-patel dispatch --data "$work/pilot" \
  --requisition REQ-2026-014 --preview --export-html site

# Snapshot the real workspace screens. These are the app's own HTML from a live
# server run against the same fixture, not mock-ups: triage and governance render
# no timestamps, so the output is byte-deterministic.
PORT="${LEDGER_UI_PORT:-8731}"
./target/release/ledger ui --data "$work/pilot" --operator rec-rui --port "$PORT" \
  > "$work/ui.log" 2>&1 &
UI_PID=$!
for _ in $(seq 1 100); do
  grep -q "token=" "$work/ui.log" 2>/dev/null && break
  sleep 0.1
done
TOKEN="$(sed -n 's/.*token=\([a-f0-9]*\).*/\1/p' "$work/ui.log" | head -1)"
if [ -n "$TOKEN" ]; then
  curl -s -c "$work/cookies" -o /dev/null "http://127.0.0.1:$PORT/?token=$TOKEN"
  snapshot() {
    # The app's own HTML, with its internal links pointed at the sibling snapshot
    # and a banner that says this is a read-only capture. The app's inline script
    # becomes a file reference so the deployed CSP can keep script-src 'self'.
    curl -s -b "$work/cookies" "http://127.0.0.1:$PORT$1" \
      | sed -e 's|href="/"|href="/workspace"|g' \
            -e 's|href="/governance"|href="/workspace-governance"|g' \
            -e 's|<body>|<body><div style="background:#fef6e6;color:#8a5a00;border-bottom:1px solid #e2e4e8;padding:8px 16px;font:13px/1.5 ui-sans-serif,system-ui,sans-serif">Read-only snapshot of the real workspace, captured at build time from the synthetic fixture. <a href="/ui" style="color:#8a5a00;text-decoration:underline">Run it locally</a> for live data.</div>|' \
      | perl -0pe 's{<script>.*?</script>}{<script src="/app.js" defer></script>}s' \
      > "$2"
  }
  snapshot "/" site/workspace.html
  snapshot "/governance" site/workspace-governance.html
  echo "snapshotted the workspace (triage + governance)"
else
  echo "could not capture the workspace snapshot" >&2
fi
kill "$UI_PID" 2>/dev/null || true
wait "$UI_PID" 2>/dev/null || true

# Optional: a real terminal cast for the appendix, when asciinema is installed.
if command -v asciinema >/dev/null; then
  LEDGER="$PWD/target/release/ledger" DATA="$work/cast" \
    asciinema rec --overwrite --quiet --command "bash scripts/demo-walkthrough.sh" site/demo.cast
  echo "wrote site/demo.cast"
else
  echo "asciinema not installed: the transcript on /demo is generated instead"
fi
