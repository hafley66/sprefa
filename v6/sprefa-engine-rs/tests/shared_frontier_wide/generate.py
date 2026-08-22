import json
import sys
from pathlib import Path

count = int(sys.argv[1])
out = Path(sys.argv[2])
out.mkdir(parents=True, exist_ok=True)

lines = []
for index in range(count):
    lines.append(f"rel source_{index}(name: text, weight: int).")
lines.append("")
for index in range(count):
    lines.append(f"heavy_{index}(Name) <-")
    lines.append(f"  source_{index}(Name, Weight),")
    lines.append("  Weight >= 10.")
    lines.append("")
(out / f"wide_{count}.dl6").write_text("\n".join(lines) + "\n")

schedule = []
for tick in range(3):
    batch = []
    for index in range(count):
        batch.append({
            "rel": f"source_{index}",
            "sign": "add",
            "row": [f"row_{tick}_{index}", 10 + tick + index],
        })
    schedule.append(batch)
(out / f"wide_{count}.schedule.json").write_text(json.dumps(schedule, indent=2) + "\n")
print(f"wide_{count}.dl6 rels={count * 2} rules={count}")
