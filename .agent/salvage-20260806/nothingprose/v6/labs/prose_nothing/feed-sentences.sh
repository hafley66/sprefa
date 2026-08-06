#!/usr/bin/env bash
# The sh host body of prose-nothing.dl6: dispatch the node feed script. The
# second argument is the probe token (a chunk index in corpus mode); the feed
# command comes from PROSE_NOTHING_MODE (feed|fixture).

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MODE="${1:-feed}"
TOKEN="${2:-all}"

case "$MODE" in
  feed)
    exec node "$SCRIPT_DIR/feed-sentences.mjs" feed "$TOKEN"
    ;;
  fixture)
    if [ -z "${PROSE_NOTHING_FIXTURE_FILE:-}" ] || [ ! -f "$PROSE_NOTHING_FIXTURE_FILE" ]; then
      printf 'feed-sentences.sh: fixture mode needs PROSE_NOTHING_FIXTURE_FILE\n' >&2
      exit 2
    fi
    exec node "$SCRIPT_DIR/feed-sentences.mjs" fixture "$PROSE_NOTHING_FIXTURE_FILE"
    ;;
  *)
    printf 'usage: feed-sentences.sh feed <token> | fixture\n' >&2
    exit 2
    ;;
esac
