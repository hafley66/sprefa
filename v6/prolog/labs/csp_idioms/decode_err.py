#!/usr/bin/env python3
# decode_err.py : turn a dl_parse_error char-code list back into readable text.
# The lab needed this to read its OWN error messages; that need is finding E1.
import re, sys
raw = sys.stdin.read()
m = re.search(r'\[(\d[\d,]*)\]', raw)
if not m:
    print("no char-code list found")
    sys.exit(1)
print(''.join(chr(int(c)) for c in m.group(1).split(',')))
