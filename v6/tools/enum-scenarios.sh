#!/usr/bin/env bash
# Reasoning for every row: plans/2026-08-08-enum-and-dot-lab.md.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
V6="$(cd "$HERE/.." && pwd)"
COMPILE="$V6/prolog/compile/scripts/compile_dl6.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

pass=0
fail=0

# Expect is `compiles` or a refusal atom. A scenario whose ACTUAL differs from
# EXPECT prints RED, which is how a fix announces itself here.
scenario() {
    local name="$1" expect="$2" source="$3"
    printf '%s\n' "$source" >"$WORK/$name.dl6"
    local out actual
    out="$(bash "$COMPILE" "$WORK/$name.dl6" "$WORK/$name.ts" 2>&1)"
    if printf '%s' "$out" | grep -q '^wrote '; then
        actual=compiles
    else
        actual="$(printf '%s' "$out" | grep -o "refused rule '[a-z_]*'" | head -1 |
                  sed "s/refused rule '//;s/'//")"
        [ -n "$actual" ] || actual="$(printf '%s' "$out" | grep -o 'parse error[^"]*' | head -1)"
        [ -n "$actual" ] || actual=unknown
    fi
    if [ "$actual" != "$expect" ]; then
        printf '  RED  %-34s expected=%s actual=%s\n' "$name" "$expect" "$actual"
        fail=$((fail + 1))
        return
    fi
    # Compiling green is not running: a table this DDL cannot create is a
    # program that dies at boot, which no compile-time check can see.
    if [ "$actual" = compiles ]; then
        local ddl boot_err
        ddl="$(grep -o 'CREATE TABLE[^`]*' "$WORK/$name.ts" | sed 's/$/;/')"
        boot_err="$(printf '%s\n' "$ddl" | sqlite3 ":memory:" 2>&1 | head -1)"
        if [ -n "$boot_err" ]; then
            printf '  RED  %-34s compiles, boot fails: %s\n' "$name" "$boot_err"
            fail=$((fail + 1))
            return
        fi
        actual=compiles+boots
    fi
    printf '  ok   %-34s %s\n' "$name" "$actual"
    pass=$((pass + 1))
}

echo "── enum declaration and variants ──"
scenario enum_two_variants compiles \
'rel door(closed(note: text) ; open(note: text)).
rel seen(id: int, tag: text).
seen(id, tag) <- door_tag(id, tag).'

scenario enum_variant_fields_read compiles \
'rel door(closed(note: text) ; open(note: text)).
rel note_of(id: int, note: text).
note_of(id, note) <- door_closed(id, note).'

scenario enum_three_variants compiles \
'rel grade(ripe(sugar: int) ; green(days: int) ; bruised(reason: text)).
rel seen(id: int, tag: text).
seen(id, tag) <- grade_tag(id, tag).'

echo "── the two known defects ──"
scenario enum_nullary_variant compiles \
'rel maybe_text(none() ; some(value: text)).
rel noted(id: int, value: text).
noted(id, value) <- maybe_text_some(id, value).'

scenario enum_as_column_type compiles \
'rel grade(ripe(sugar: int) ; green(days: int)).
rel picked(id: int, g: grade).
rel seen(id: int).
seen(id) <- picked(id, g).'

scenario enum_tag_as_column_type compiles \
'rel grade(ripe(sugar: int) ; green(days: int)).
rel picked(id: int, g: grade_tag).
rel seen(id: int).
seen(id) <- picked(id, g).'

echo "── type paths, rel referencing rel ──"
scenario plain_rel_as_column_type compiles \
'rel span(start: int, end: int).
rel finding(path: text, at: span).
rel found(path: text).
found(path) <- finding(path, at).'

scenario json_column compiles \
'rel doc(id: int, payload: json).
rel has_name(id: int, name: text).
has_name(id, name) <- doc(id, payload), decode(payload, {name: name}).'

scenario list_column compiles \
'rel tags(id: int, names: list(text)).
rel tagged(id: int).
tagged(id) <- tags(id, names).'

echo "── dot routing ──"
scenario dot_member_access compiles \
'rel doc(id: int, payload: json).
rel has_name(id: int, name: text).
has_name(id, name) <- doc(id, payload), name := payload.name.'

scenario dot_module_path module_path_unresolved \
'rel out(id: int).
out(id) <- other.thing(id).'

scenario dot_onto_enum_variant module_path_unresolved \
'rel grade(ripe(sugar: int) ; green(days: int)).
rel out(id: int).
out(id) <- grade.ripe(id, sugar).'

echo "── catalog self-read ──"
scenario read_rel_catalog compiles \
'rel rel_name(name: text).
rel_name(local_name) <- __rel(rel_id, parent_id, ordinal, local_name, kind, type_id, arity, module_id, h_id, h_schema, h_rule).'

echo
printf 'enum-scenarios: %d ok, %d RED\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
