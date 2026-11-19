#!/usr/bin/env bash
set -uo pipefail

pattern='--dangerously-skip-permissions|--yolo|bypassPermissions|danger-full-access|--full-auto'

cd "$(dirname "$0")/.." || exit 2

grep -RInE -- "$pattern" crates shell xtask
case $? in
    0) exit 1 ;;
    1) exit 0 ;;
    *) exit 2 ;;
esac
