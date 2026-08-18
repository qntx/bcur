#!/usr/bin/env bash
# Extract quoted `ur:…` literals from pinned URKit / bc-ur test files.
# Drift detection only — not BCR-2020-005 compliance. Prints sorted unique
# strings to stdout. Do not vendor the fetched sources.
set -euo pipefail

: "${URKIT_REPO:=BlockchainCommons/URKit}"
: "${URKIT_REF:=15.1.0}"
: "${URKIT_UR_TESTS:=Tests/URKitTests/URTests.swift}"
: "${URKIT_FOUNTAIN_TESTS:=Tests/URKitTests/FountainCodesTests.swift}"
: "${BCUR_CPP_REPO:=BlockchainCommons/bc-ur}"
: "${BCUR_CPP_REF:=4479fb81b2350ae8bafa042a5572b9c64c2c32ca}"
: "${BCUR_CPP_TESTS:=test/test.cpp}"

USER_AGENT="bcur-vector-extract/0.3 (+https://github.com/qntx/bcur)"

fetch_raw() {
  local repo="$1" path="$2" ref="$3" dest="$4"
  local api="repos/${repo}/contents/${path}?ref=${ref}"
  local raw="https://raw.githubusercontent.com/${repo}/${ref}/${path}"
  if command -v gh >/dev/null 2>&1; then
    if gh api "$api" -H "Accept: application/vnd.github.raw" >"$dest" 2>/dev/null \
      && [[ -s "$dest" ]]; then
      return 0
    fi
  fi
  curl -fsSL -A "$USER_AGENT" "$raw" >"$dest"
  [[ -s "$dest" ]]
}

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

fetch_raw "$URKIT_REPO" "$URKIT_UR_TESTS" "$URKIT_REF" "$tmpdir/urkit_ur.swift" \
  || { echo "extract_published_urs: failed to fetch ${URKIT_REPO} ${URKIT_UR_TESTS} @ ${URKIT_REF}" >&2; exit 1; }
fetch_raw "$URKIT_REPO" "$URKIT_FOUNTAIN_TESTS" "$URKIT_REF" "$tmpdir/urkit_fountain.swift" \
  || { echo "extract_published_urs: failed to fetch ${URKIT_REPO} ${URKIT_FOUNTAIN_TESTS} @ ${URKIT_REF}" >&2; exit 1; }
fetch_raw "$BCUR_CPP_REPO" "$BCUR_CPP_TESTS" "$BCUR_CPP_REF" "$tmpdir/bcur_test.cpp" \
  || { echo "extract_published_urs: failed to fetch ${BCUR_CPP_REPO} ${BCUR_CPP_TESTS} @ ${BCUR_CPP_REF}" >&2; exit 1; }

# Quoted literals only (`"ur:type/…"` / Swift `"""ur:type/…"""`).
# Bare `ur:` hits comments and concatenations.
{
  grep -h -oE '"ur:[a-z0-9-]+/[^"]+"' "$tmpdir"/* || true
  grep -h -oE '"""ur:[a-z0-9-]+/[^"]+"""' "$tmpdir"/* || true
} | sed -E 's/^"+//; s/"+$//' | LC_ALL=C sort -u
