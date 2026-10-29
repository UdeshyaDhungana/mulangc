#!/usr/bin/env bash
# Compile test.mu with the selected backend and run or show the result.
#   ./test.sh        native: assemble, link, run
#   ./test.sh --ir   write test.irmu and print it
set -euo pipefail

case "$*" in
    "")
        cargo run --quiet -- test.mu

        echo "--- test.s ---"
        cat test.s

        echo "--- building ---"
        clang -g -c test.s -o test.o
        clang -g test.o -o test

        echo "--- running ---"
        set +e
        ./test
        echo "--- exit $? ---"
        ;;
    "--ir")
        cargo run --quiet -- --irtext test.mu

        echo "--- test.irmu ---"
        cat test.irmu
        ;;
    *)
        echo "usage: $0 [--ir]" >&2
        exit 1
        ;;
esac
