#!/usr/bin/env python3
"""stamp.py -- `cat` with a millisecond clock in front of every line.

Reads stdin, writes `<epoch_ms> <line>` to stdout. The origin is NOT subtracted
here: run.sh only knows the step-0 midpoint after the SSE connection is up, so
the stamp stays absolute and lib/lines.py subtracts `--t0-ms` later.

This is the only reason python is in the leg-B path at all: bash on macOS has
no sub-second clock, and `curl -N`'s SSE output has to be timestamped as it
arrives rather than afterwards. It drives nothing and knows no HTTP; run.sh's
own curl calls are the driver.
"""

import sys
import time

def main() -> int:
    for line in sys.stdin:
        sys.stdout.write(f"{int(round(time.time() * 1000.0))} {line.rstrip()}\n")
        sys.stdout.flush()
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
