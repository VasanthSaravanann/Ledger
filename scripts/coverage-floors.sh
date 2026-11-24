#!/usr/bin/env bash
# Enforces per-module line-coverage floors from an lcov.info file.
set -euo pipefail

lcov="${1:-lcov.info}"
floors="$(dirname "$0")/../coverage-floors.toml"
status=0

while IFS='=' read -r module floor; do
  module="$(echo "$module" | tr -d '" ')"
  floor="$(echo "$floor" | tr -d ' ')"
  [ -n "$module" ] || continue
  case "$module" in \#*|\[*) continue ;; esac

  read -r hit found < <(awk -v target="$module" '
    /^SF:/ { file = substr($0, 4); next }
    /^DA:/ && file ~ target"$" { split(substr($0, 4), p, ","); total++; if (p[2] > 0) covered++ }
    END { printf "%d %d\n", covered, total }
  ' "$lcov")

  if [ "${found:-0}" -eq 0 ]; then
    echo "no coverage data for $module" >&2
    status=1
    continue
  fi
  pct=$(( hit * 100 / found ))
  if [ "$pct" -lt "$floor" ]; then
    echo "FAIL $module: ${pct}% < ${floor}%" >&2
    status=1
  else
    echo "ok   $module: ${pct}% >= ${floor}%"
  fi
done < <(sed -n '/^\[floors\]/,$p' "$floors" | grep '=')

exit "$status"
