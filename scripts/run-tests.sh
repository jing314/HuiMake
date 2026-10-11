#!/usr/bin/env bash
# Run all HuiMake tests: unit tests + end-to-end tests + example-project tests.
#
# Usage:
#   bash scripts/run-tests.sh                  # everything
#   bash scripts/run-tests.sh example_tests     # only tests whose name matches
#   bash scripts/run-tests.sh --unit-only       # only the unit tests inside the binary
#   bash scripts/run-tests.sh --skip-clippy     # skip clippy
set -euo pipefail
cd "$(dirname "$0")/.."

filter=""
unit_only=0
skip_clippy=0

for arg in "$@"; do
  case "$arg" in
    --unit-only) unit_only=1 ;;
    --skip-clippy) skip_clippy=1 ;;
    -h|--help)
      sed -n '2,10p' "$0"
      exit 0
      ;;
    *) filter="$arg" ;;
  esac
done

echo "== environment =="
cargo --version
rustc --version
if command -v gcc >/dev/null 2>&1; then
  echo "gcc: $(gcc --version | head -n1)"
else
  echo "gcc: not found (tests that need it will be skipped)"
fi

if [ "$skip_clippy" -eq 0 ]; then
  echo
  echo "== clippy =="
  cargo clippy --all-targets -- -D warnings
fi

echo
echo "== cargo test =="
args=(test)
if [ "$unit_only" -eq 1 ]; then
  args+=(--bin hk)
fi
if [ -n "$filter" ]; then
  args+=(-- "$filter")
fi
echo "cargo ${args[*]}"
cargo "${args[@]}"

echo
echo "All tests passed"
