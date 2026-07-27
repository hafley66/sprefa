#!/usr/bin/env bash
exec "$(dirname "$0")/pure_wrap.sh" /opt/homebrew/bin/swipl -q -l -- "$@"
