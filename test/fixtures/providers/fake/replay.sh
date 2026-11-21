#!/bin/sh
set -eu

fixture="$1"
delay_ms="${FAKE_DELAY_MS:-20}"

while IFS= read -r line || [ -n "$line" ]; do
    printf '%s\n' "$line"
    sleep "$(printf '%d.%03d' $((delay_ms / 1000)) $((delay_ms % 1000)))"
done < "$fixture"

exit "${FAKE_EXIT:-0}"
