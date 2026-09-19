#!/usr/bin/env bash
#
# Cross-language conformance check for the Thadam registry.
#
# The registry's environment list is maintained by hand in each language. That
# duplication is a deliberate choice (see ../README.md), and this script is the
# price of it: every implementation dumps its catalogue as canonical JSON, and
# each dump is diffed against environments.json. If two implementations drift,
# this fails.
#
# What is compared is identity only — id, chain, environment, display name,
# kind. Consensus constants exist in exactly one implementation (Rust), so
# there is nothing to compare them against; `cargo test` guards those instead.
#
# Usage:
#   ./check.sh                  strict: a missing toolchain is a failure
#   ./check.sh --allow-missing  local dev: skip implementations that can't run
#
# Strict is the default on purpose. A conformance check that quietly skips the
# implementation that drifted is worse than no check.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
registry="$(dirname "$here")"
fixture="$here/environments.json"

allow_missing=false
if [[ "${1:-}" == "--allow-missing" ]]; then
  allow_missing=true
fi

failures=0
checked=0

fail() {
  printf 'FAIL  %s\n' "$1" >&2
  failures=$((failures + 1))
}

missing() {
  if [[ "$allow_missing" == true ]]; then
    printf 'SKIP  %s (%s not found)\n' "$1" "$2" >&2
  else
    fail "$1: $2 not found. Install it, or re-run with --allow-missing."
  fi
}

compare() {
  local name="$1" actual="$2"
  checked=$((checked + 1))
  if diff -u "$fixture" "$actual" > /tmp/thadam-conformance.diff 2>&1; then
    printf 'ok    %s\n' "$name"
  else
    fail "$name does not match environments.json"
    cat /tmp/thadam-conformance.diff >&2
  fi
}

if [[ ! -f "$fixture" ]]; then
  printf 'FAIL  fixture missing: %s\n' "$fixture" >&2
  exit 1
fi

# --- core (Rust) ------------------------------------------------------------
if command -v cargo > /dev/null 2>&1; then
  cargo run --quiet --manifest-path "$registry/core/Cargo.toml" \
    --bin dump-environments > /tmp/thadam-core-environments.json
  compare "core" /tmp/thadam-core-environments.json
else
  missing "core" "cargo"
fi

# --- ui-catalog (TypeScript) ------------------------------------------------
if command -v node > /dev/null 2>&1; then
  node --experimental-strip-types "$registry/ui-catalog/bin/dump-environments.ts" \
    > /tmp/thadam-ui-catalog-environments.json
  compare "ui-catalog" /tmp/thadam-ui-catalog-environments.json
else
  missing "ui-catalog" "node"
fi

# --- go (not implemented) ---------------------------------------------------
# Not implemented. The chain service does not exist yet, and a package with no
# consumer is the structure-before-content that ARCHITECTURE.md §10 rejects.
# When the service lands, add a dump command here; the fixture is already the
# thing it has to agree with.

printf '\n%d implementation(s) checked, %d failure(s)\n' "$checked" "$failures"
[[ "$failures" -eq 0 ]]
