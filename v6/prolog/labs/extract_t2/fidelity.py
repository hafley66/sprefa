#!/usr/bin/env python3
"""fidelity.py <source.json> <oracle.jsonl>

Two receipts over one OpenAPI document and the facts a reifier derived from it.

1. CENSUS (question Q1). Walk every key that appears inside
   `components.schemas` in the SOURCE and classify it:

       covered  -- some rule in openapi.dl6 reads this key
       hole     -- nothing reads it; named, counted, and printed

   This is the "every construct in the source doc lands in the algebra or gets
   a named hole" obligation, answered by walking the source rather than by
   reading the rules and hoping.

2. ROUND TRIP (question Q5). Rebuild a `components.schemas` fragment FROM THE
   FACTS ALONE and diff it against the source restricted to the subset the
   algebra claims (type, properties, required, $ref, enum, items, format).

   The rebuild happens HERE, in python, and that is itself the finding: the
   compiler refuses both dl6 spellings that could construct a document --
   `unsupported_construct(json_value_expression(...))` for a braces literal in
   value position, and the aggregate `json_object/2` head. The reference engine
   accepts both. So a round trip is expressible on ONE door, which makes it not
   expressible. See the verdict, Q5.

Exit 0 if the round trip is exact over the claimed subset.
"""
import json
import sys
from collections import defaultdict

# Keys openapi.dl6 actually reads. Everything else in a schema object is a hole.
COVERED_KEYS = {"type", "properties", "required", "$ref", "enum", "items", "format"}


def load_facts(path: str) -> dict[str, list[tuple]]:
    facts: dict[str, list[tuple]] = defaultdict(list)
    for raw in open(path, encoding="utf-8"):
        raw = raw.strip()
        if not raw:
            continue
        for rel, delta in json.loads(raw)["deltas"].items():
            for row in delta.get("add", []):
                facts[rel].append(tuple(row))
    return facts


def census(source: dict) -> tuple[dict[str, int], dict[str, int]]:
    covered: dict[str, int] = defaultdict(int)
    holes: dict[str, int] = defaultdict(int)

    def walk(node, inside_schema: bool) -> None:
        if isinstance(node, dict):
            for key, value in node.items():
                if inside_schema:
                    (covered if key in COVERED_KEYS else holes)[key] += 1
                # property NAMES and enum members are data, not constructs
                walk(value, inside_schema and key not in {"properties"})
                if inside_schema and key == "properties" and isinstance(value, dict):
                    for schema in value.values():
                        walk(schema, True)
        elif isinstance(node, list):
            for item in node:
                walk(item, inside_schema)

    for schema in source.get("components", {}).get("schemas", {}).values():
        walk(schema, True)
    return dict(covered), dict(holes)


def rebuild(facts: dict[str, list[tuple]]) -> dict:
    schemas: dict[str, dict] = {}
    for _repo, name, kind in facts.get("type_def", []):
        if kind == "record":
            schemas.setdefault(name, {"type": "object", "properties": {}})
    for _repo, name, field in facts.get("field_def", []):
        schemas.setdefault(name, {"type": "object", "properties": {}})
        schemas[name]["properties"].setdefault(field, {})
    for _repo, name, field, prim in facts.get("field_prim", []):
        schemas[name]["properties"][field]["items_or_type"] = prim
    for _repo, name, field in facts.get("field_repeated", []):
        schemas[name]["properties"][field]["repeated"] = True
    for _repo, name, field, target in facts.get("field_ref", []):
        schemas[name]["properties"][field]["ref"] = target
    for _repo, name, field, fmt in facts.get("field_format", []):
        schemas[name]["properties"][field]["format"] = fmt
    for _repo, name, field in facts.get("field_required", []):
        schemas[name].setdefault("required", set()).add(field)
    enums: dict[str, set] = defaultdict(set)
    for _repo, enum_name, variant in facts.get("enum_variant", []):
        enums[enum_name].add(variant)
    for enum_name, variants in enums.items():
        owner, _, field = enum_name.rpartition(".")
        if owner in schemas and field in schemas[owner]["properties"]:
            schemas[owner]["properties"][field]["enum"] = sorted(variants)
    return schemas


def claimed_subset(source: dict) -> dict:
    """The source, restricted to exactly what the algebra says it captures."""
    out: dict[str, dict] = {}
    for name, schema in source.get("components", {}).get("schemas", {}).items():
        if schema.get("type") != "object":
            continue
        entry: dict = {"type": "object", "properties": {}}
        if "required" in schema:
            entry["required"] = set(schema["required"])
        for field, prop in schema.get("properties", {}).items():
            shape: dict = {}
            if "$ref" in prop:
                shape["ref"] = prop["$ref"].rsplit("/", 1)[-1]
            elif prop.get("type") == "array":
                shape["repeated"] = True
                items = prop.get("items", {})
                if "$ref" in items:
                    shape["ref"] = items["$ref"].rsplit("/", 1)[-1]
                elif "type" in items:
                    shape["items_or_type"] = items["type"]
            elif "type" in prop:
                shape["items_or_type"] = prop["type"]
            if "format" in prop:
                shape["format"] = prop["format"]
            if "enum" in prop:
                shape["enum"] = sorted(prop["enum"])
            entry["properties"][field] = shape
        out[name] = entry
    return out


def normalize(value):
    if isinstance(value, set):
        return sorted(value)
    if isinstance(value, dict):
        return {key: normalize(inner) for key, inner in sorted(value.items())}
    if isinstance(value, list):
        return [normalize(item) for item in value]
    return value


def main(argv: list[str]) -> int:
    source = json.load(open(argv[1], encoding="utf-8"))
    facts = load_facts(argv[2])

    covered, holes = census(source)
    print("== Q1 construct census, inside components.schemas ==")
    print(f"{'key':<24} {'occurrences':>12}  status")
    for key in sorted(covered, key=lambda k: -covered[k]):
        print(f"{key:<24} {covered[key]:>12}  covered")
    for key in sorted(holes, key=lambda k: -holes[k]):
        print(f"{key:<24} {holes[key]:>12}  HOLE")
    total = sum(covered.values()) + sum(holes.values())
    print(f"\ncovered {sum(covered.values())} / {total} key occurrences; "
          f"{len(holes)} distinct constructs unread")

    print("\n== Q5 round trip, over the claimed subset ==")
    rebuilt = normalize(rebuild(facts))
    expected = normalize(claimed_subset(source))
    if rebuilt == expected:
        print(f"EXACT: {len(expected)} schemas, "
              f"{sum(len(s['properties']) for s in expected.values())} properties")
        return 0
    print("DIFFERS")
    for name in sorted(set(rebuilt) | set(expected)):
        if rebuilt.get(name) != expected.get(name):
            print(f"  {name}:")
            print(f"    facts  {json.dumps(rebuilt.get(name), sort_keys=True)}")
            print(f"    source {json.dumps(expected.get(name), sort_keys=True)}")
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
