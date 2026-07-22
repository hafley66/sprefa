#!/bin/bash
# chat-find.sh - search across chat logs, design docs, and transcripts
# Usage:
#   chat-find.sh PATTERN           search and rank; pipe to fzf if available
#   chat-find.sh --sessions PATTERN  reverse ref: rank files by hit count
#   chat-find.sh --pin path:line "note"  append a pin manually
#   chat-find.sh -h | --help       show this help

set -euo pipefail

# Directories and files to search
CHAT_LOG_DIR="chat_log"
DESIGN_DOCS="plans v6/plans v6"
TRANSCRIPT_DIR="${HOME}/.claude/projects/-Users-chrishafley-projects-sprefa"
PINS_FILE="v6/findings/PINS.md"

# Helper: print usage
usage() {
    cat <<'EOF'
chat-find.sh - search chatlog, design docs, and transcripts

USAGE:
    chat-find.sh PATTERN
        Search across chat_log/*.md, design docs, and *.jsonl transcripts.
        Output: path:line: snippet
        If fzf is on PATH, pipe through it. Selected line appends to PINS.md.

    chat-find.sh --sessions PATTERN
        Reverse reference: rank files by hit count.
        Output: file:count (sorted descending by count).

    chat-find.sh --pin path:line "note"
        Append a pin manually to PINS.md.

    chat-find.sh -h | --help
        Show this help.

ENVIRONMENT:
    CHAT_LOG_DIR     chat_log
    DESIGN_DOCS      plans v6/plans v6
    TRANSCRIPT_DIR   ~/.claude/projects/-Users-chrishafley-projects-sprefa
    PINS_FILE        v6/findings/PINS.md

NOTES:
    - Snippets from .jsonl are cleaned JSON; rough sed applied.
    - Path:line references are reproducible with rg.
    - fzf selection appends: - [YYYY-MM-DD] PATTERN — path:line — "snippet"
EOF
}

# Helper: extract today's date (YYYY-MM-DD format)
get_date() {
    date +%Y-%m-%d
}

# Helper: search markdown files
search_markdown() {
    local pattern="$1"
    # Search in chat_log and design doc directories
    rg -i --color=never "$pattern" \
        "$CHAT_LOG_DIR"/*.md \
        plans/*.md v6/plans/*.md v6/*.md \
        2>/dev/null || true
}

# Helper: search transcript (.jsonl) files
search_transcripts() {
    local pattern="$1"
    if [ ! -d "$TRANSCRIPT_DIR" ]; then
        return
    fi
    # Search .jsonl files and extract quoted strings
    rg -i --color=never "$pattern" "$TRANSCRIPT_DIR"/*.jsonl 2>/dev/null || true
}

# Helper: combine and rank search results
search_all() {
    local pattern="$1"
    {
        search_markdown "$pattern"
        search_transcripts "$pattern"
    } | sort | uniq
}

# Helper: reverse reference — rank files by hit count
rank_by_files() {
    local pattern="$1"
    {
        search_markdown "$pattern" | cut -d: -f1
        if [ -d "$TRANSCRIPT_DIR" ]; then
            rg -i --color=never "$pattern" "$TRANSCRIPT_DIR"/*.jsonl 2>/dev/null | cut -d: -f1 || true
        fi
    } | sort | uniq -c | sort -rn | awk '{print $2 ":" $1}'
}

# Helper: append to PINS.md
append_pin() {
    local line="$1"
    local pattern="$2"
    local date=$(get_date)

    # Ensure PINS.md exists with a header
    if [ ! -f "$PINS_FILE" ]; then
        mkdir -p "$(dirname "$PINS_FILE")"
        cat >"$PINS_FILE" <<'HEADER'
# Pins

Pinned search results from chat-find.sh.

## Search results

HEADER
    fi

    # Extract path and line number (first two colon-separated fields)
    local path_line=$(echo "$line" | cut -d: -f1-2)
    # Extract snippet (everything after the second colon), clean quotes and truncate
    local snippet=$(echo "$line" | cut -d: -f3- | sed 's/^[[:space:]]*//; s/"//g' | head -c 80)

    # Append to PINS.md
    echo "- [$date] $pattern — $path_line — \"$snippet\"" >> "$PINS_FILE"
}

# Main
main() {
    if [ $# -eq 0 ]; then
        usage
        exit 0
    fi

    case "$1" in
        -h|--help)
            usage
            exit 0
            ;;
        --sessions)
            if [ $# -lt 2 ]; then
                echo "Error: --sessions requires a PATTERN"
                exit 1
            fi
            rank_by_files "$2"
            ;;
        --pin)
            if [ $# -lt 3 ]; then
                echo "Error: --pin requires path:line and a note"
                exit 1
            fi
            append_pin "$2" "$3"
            echo "Pinned: $2 ($3)"
            ;;
        *)
            # Default: search and pipe to fzf if available
            local pattern="$1"
            local results
            results=$(search_all "$pattern")

            if [ -z "$results" ]; then
                echo "No results for: $pattern"
                exit 0
            fi

            # Check if fzf is available
            if command -v fzf &> /dev/null; then
                local selected
                selected=$(echo "$results" | fzf --preview 'echo {}')
                if [ -n "$selected" ]; then
                    append_pin "$selected" "$pattern"
                    echo "Pinned: $selected"
                fi
            else
                # No fzf: just print ranked results
                echo "$results"
                echo ""
                echo "(fzf not found; run 'brew install fzf' to enable interactive selection)"
            fi
            ;;
    esac
}

main "$@"
