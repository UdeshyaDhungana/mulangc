#!/usr/bin/env bash
# Compile test.muji with the selected backend and run or show the result.
#   ./test.sh              native: assemble, link, run
#   ./test.sh --ir         run the IR interpreter
#   ./test.sh --ir --text  write test.irmu and print it
set -euo pipefail

case "$*" in
    "")
        cargo run --quiet -- test.muji

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
        echo "--- interpreting ---"
        set +e
        cargo run --quiet -- --ir test.muji
        echo "--- exit $? ---"
        ;;
    "--ir --text" | "--text --ir")
        cargo run --quiet -- --ir --text test.muji

        echo "--- test.irmu ---"
        cat test.irmu
        ;;
    *)
        echo "usage: $0 [--ir [--text]]" >&2
        exit 1
        ;;
esac
