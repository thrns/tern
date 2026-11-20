#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
OUT_ROOT="$ROOT/test/fixtures/providers"
PROMPT="Reply with exactly: hello fixture"
RESUME_PROMPT="Repeat the previous answer"
CLAUDE_FLAGS=(--output-format stream-json --verbose --include-partial-messages --permission-mode dontAsk --tools "Read,Glob,Grep")
CODEX_FLAGS=(--json --sandbox read-only --skip-git-repo-check)
LIST=0
WORK=""

usage() {
    printf 'usage: %s [--list]\n' "$(basename "$0")"
}

if [ "$#" -gt 1 ]; then
    usage >&2
    exit 2
fi
case "${1:-}" in
    "") ;;
    --list) LIST=1 ;;
    -h | --help)
        usage
        exit 0
        ;;
    *)
        usage >&2
        exit 2
        ;;
esac

have() {
    command -v "$1" >/dev/null 2>&1
}

skip_cli() {
    printf 'skip %s: not found on PATH\n' "$1"
}

print_command() {
    printf '    '
    printf '%q ' "$@"
    printf '\n'
}

run_case() {
    local id=$1 name=$2 input=$3
    shift 3
    local dir="$OUT_ROOT/$id"
    if [ "$LIST" -eq 1 ]; then
        printf '  %s/%s\n' "$id" "$name"
        if [ -n "$input" ]; then
            printf '    stdin: %s\n' "$input"
        fi
        print_command "$@"
        return 0
    fi
    mkdir -p "$dir"
    local status=0
    if [ -n "$input" ]; then
        (cd "$WORK" && "$@") <<<"$input" >"$dir/$name.jsonl" 2>"$dir/$name.stderr.txt" || status=$?
    else
        (cd "$WORK" && "$@") </dev/null >"$dir/$name.jsonl" 2>"$dir/$name.stderr.txt" || status=$?
    fi
    printf '%s\n' "$status" >"$dir/$name.exit.txt"
    printf '  %s/%s exit %s\n' "$id" "$name" "$status"
}

skip_case() {
    local id=$1 name=$2 reason=$3
    printf '  %s/%s skipped: %s\n' "$id" "$name" "$reason"
    if [ "$LIST" -eq 0 ]; then
        mkdir -p "$OUT_ROOT/$id"
        printf '%s\n' "$reason" >"$OUT_ROOT/$id/$name.stderr.txt"
    fi
}

session_from() {
    local file=$1 key=$2 placeholder=$3
    if [ "$LIST" -eq 1 ]; then
        printf '%s' "$placeholder"
        return 0
    fi
    grep -o "\"$key\":\"[^\"]*\"" "$file" 2>/dev/null | head -n 1 | cut -d'"' -f4 || true
}

exit_status() {
    local status=""
    read -r status <"$1" 2>/dev/null || true
    printf '%s' "$status"
}

record_version() {
    if [ "$LIST" -eq 0 ]; then
        mkdir -p "$OUT_ROOT/$1"
        "$1" --version >"$OUT_ROOT/$1/VERSION" 2>&1 || true
    fi
}

prepare_inputs() {
    head -c 1024 /dev/zero | tr '\0' 'a' >"$WORK/note.txt"
    if ! screencapture -x -m "$WORK/full.png"; then
        printf 'screencapture failed; grant Screen Recording to your terminal app\n' >&2
        exit 1
    fi
    sips -Z 200 "$WORK/full.png" --out "$WORK/tiny.png" >/dev/null
    rm -f "$WORK/full.png"
}

write_agy_mcp_config() {
    if [ "$LIST" -eq 1 ]; then
        printf '  writes %s\n' "$WORK/.agents/mcp_config.json"
        return 0
    fi
    mkdir -p "$WORK/.agents"
    printf '%s\n' '{"mcpServers":{"tern_dummy":{"command":"/usr/bin/true","args":[]}}}' >"$WORK/.agents/mcp_config.json"
}

capture_claude() {
    local sid
    record_version claude
    run_case claude text "" claude -p "${CLAUDE_FLAGS[@]}" "$PROMPT"
    sid=$(session_from "$OUT_ROOT/claude/text.jsonl" session_id '<session_id>')
    if [ -n "$sid" ]; then
        run_case claude resume "" claude -p --resume "$sid" "${CLAUDE_FLAGS[@]}" "$RESUME_PROMPT"
    else
        skip_case claude resume "no session_id found in text.jsonl"
    fi
    run_case claude image "" claude -p "${CLAUDE_FLAGS[@]}" "Describe ./tiny.png in five words. Use the Read tool."
    run_case claude denied "" claude -p "${CLAUDE_FLAGS[@]}" "Create a file named x.txt"
    run_case claude error "" claude -p --model does-not-exist "${CLAUDE_FLAGS[@]}" "$PROMPT"
}

capture_codex_resume() {
    local tid=$1 variant=none status
    if [ "$LIST" -eq 1 ]; then
        run_case codex resume "$RESUME_PROMPT" codex exec "${CODEX_FLAGS[@]}" -C "$WORK" resume "$tid" -
        run_case codex resume "$RESUME_PROMPT" codex exec resume "$tid" "${CODEX_FLAGS[@]}" -C "$WORK" -
        return 0
    fi
    run_case codex resume "$RESUME_PROMPT" codex exec "${CODEX_FLAGS[@]}" -C "$WORK" resume "$tid" -
    status=$(exit_status "$OUT_ROOT/codex/resume.exit.txt")
    if [ "$status" = 0 ]; then
        variant=exec-flags-then-resume
    else
        run_case codex resume "$RESUME_PROMPT" codex exec resume "$tid" "${CODEX_FLAGS[@]}" -C "$WORK" -
        status=$(exit_status "$OUT_ROOT/codex/resume.exit.txt")
        if [ "$status" = 0 ]; then
            variant=resume-then-flags
        fi
    fi
    printf '%s\n' "$variant" >"$OUT_ROOT/codex/resume.variant.txt"
}

capture_codex() {
    local tid
    record_version codex
    run_case codex text "$PROMPT" codex exec "${CODEX_FLAGS[@]}" -C "$WORK" -
    tid=$(session_from "$OUT_ROOT/codex/text.jsonl" thread_id '<thread_id>')
    if [ -n "$tid" ]; then
        capture_codex_resume "$tid"
    else
        skip_case codex resume "no thread_id found in text.jsonl"
    fi
    run_case codex image "Describe ./tiny.png in five words." codex exec -i tiny.png "${CODEX_FLAGS[@]}" -C "$WORK" -
    run_case codex mcp_override "$PROMPT" codex exec "${CODEX_FLAGS[@]}" -C "$WORK" -c 'mcp_servers.tern_dummy.command="/usr/bin/true"' -c 'mcp_servers.tern_dummy.args=[]' -
    run_case codex error "$PROMPT" codex exec -m does-not-exist "${CODEX_FLAGS[@]}" -C "$WORK" -
}

capture_agy() {
    local sid
    record_version agy
    run_case agy text "" agy -p "$PROMPT" --output-format stream-json
    sid=$(session_from "$OUT_ROOT/agy/text.jsonl" conversation_id '<conversation_id>')
    if [ -n "$sid" ]; then
        run_case agy resume "" agy --conversation "$sid" -p "$RESUME_PROMPT" --output-format stream-json
    else
        skip_case agy resume "no conversation_id found in text.jsonl"
    fi
    run_case agy image "" agy -p "Describe the image at $WORK/tiny.png in five words" --output-format stream-json
    run_case agy denied "" agy -p "Create a file named x.txt" --output-format stream-json
    write_agy_mcp_config
    run_case agy mcp_workspace "" agy -p "List your MCP servers" --output-format stream-json
}

capture_gemini() {
    local sid
    record_version gemini
    run_case gemini text "" gemini -p "$PROMPT" --output-format stream-json
    sid=$(session_from "$OUT_ROOT/gemini/text.jsonl" session_id '<session_id>')
    if [ -n "$sid" ]; then
        run_case gemini resume "" gemini --resume "$sid" -p "$RESUME_PROMPT" --output-format stream-json
    else
        skip_case gemini resume "no session_id found in text.jsonl"
    fi
    run_case gemini error "" gemini -m does-not-exist -p "$PROMPT" --output-format stream-json
}

main() {
    if [ "$LIST" -eq 1 ]; then
        printf 'dry run: planned commands, nothing executed\n'
        WORK="<tmp>"
    else
        WORK=$(mktemp -d)
        trap 'rm -rf -- "$WORK"' EXIT
        prepare_inputs
    fi

    if have claude; then capture_claude; else skip_cli claude; fi
    if have codex; then capture_codex; else skip_cli codex; fi
    if have agy; then capture_agy; else skip_cli agy; fi
    if have gemini; then capture_gemini; else skip_cli gemini; fi

    printf '\nFixtures can contain your account email, organization names and session ids. Review every file under %s before committing.\n' "$OUT_ROOT"
}

main
