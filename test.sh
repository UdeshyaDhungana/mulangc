#!/usr/bin/env bash
# Compile test.muji, assemble and link the result, then run it.
set -euo pipefail

cargo run --quiet -- test.muji

echo "--- test.s ---"
cat test.s

echo "--- building ---"
clang -g -o test test.s

echo "--- running ---"
set +e
./test
echo "--- exit $? ---"
