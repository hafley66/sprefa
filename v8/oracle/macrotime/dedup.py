"""Drop byte-identical oracle cases, keeping the first name in sort order."""

import hashlib
import os
import sys

directory = sys.argv[1]
seen = {}
for name in sorted(os.listdir(directory)):
    if not name.endswith(".json"):
        continue
    path = os.path.join(directory, name)
    with open(path, "rb") as handle:
        digest = hashlib.sha256(handle.read()).hexdigest()
    if digest in seen:
        os.remove(path)
        print("duplicate", name, "==", seen[digest])
    else:
        seen[digest] = name
print("cases", len(seen))
