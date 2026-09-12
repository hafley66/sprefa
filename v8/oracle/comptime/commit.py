"""Merge the refreeze table into each evaluate_checked case, then commit the
shared prelude leg once plus the own-unit leg of the fixtures named below."""
import glob
import hashlib
import json
import os
import shutil
import sys

OWN_UNIT_COMMITTED = ['8_hosted', '2_partial']
ENTRY_ORDER = [
    'assemble_generated_program',
    'validate_functional_rows',
    'generated_expression_environment',
    'validate_hosted_relations',
    'erase_host_planning_rows',
    'evaluate_checked',
]


def case_files(scratch, stem):
    rows = []
    for path in glob.glob(os.path.join(scratch, f'{stem}-*.json')):
        name = os.path.basename(path)[len(stem) + 1:-len('.json')]
        entry, index = name.rsplit('-', 1)
        rows.append((int(index), entry, path))
    rows.sort()
    return rows


def leg_counts(scratch, stem):
    """v7 calls per evaluate_checked leg: one assemble per compiler round, one
    refreeze per source-refreeze pass."""
    out = []
    assemble = 0
    refreeze = 0
    for _, entry, _ in case_files(scratch, stem):
        if entry == 'assemble_generated_program':
            assemble += 1
        elif entry == 'refreeze':
            refreeze += 1
        elif entry == 'evaluate_checked':
            out.append({'v7_assemble': assemble, 'v7_refreeze': refreeze})
            assemble = 0
            refreeze = 0
    return out


def merged(scratch, stem):
    """Refreeze cases become one evaluate_checked case's replay table."""
    pending = []
    out = []
    for index, entry, path in case_files(scratch, stem):
        case = json.load(open(path))
        if entry == 'refreeze':
            pending.append({
                'deferred': case['input']['deferred'],
                'facts_len': case['input']['facts_len'],
                'generated_len': case['input']['generated_len'],
                'checked': case['expected']['checked'],
                'diagnostics': case['expected']['diagnostics'],
            })
            continue
        if entry == 'evaluate_checked':
            case['input']['replay'] = pending
            pending = []
        out.append((index, entry, case))
    return out


def digest(case):
    return hashlib.sha256(
        json.dumps(case, sort_keys=True, separators=(',', ':')).encode()
    ).hexdigest()


def main(scratch, here):
    stems = sorted({
        os.path.basename(p).rsplit('-', 2)[0]
        for p in glob.glob(os.path.join(scratch, '*-*-*.json'))
    })
    per_stem = {stem: merged(scratch, stem) for stem in stems}

    # The first call of every entry belongs to the userland type prelude and is
    # byte-identical across every fixture.
    prelude = {}
    for stem in stems:
        seen = set()
        for index, entry, case in per_stem[stem]:
            if entry in seen:
                continue
            seen.add(entry)
            prelude.setdefault(entry, {}).setdefault(digest(case), case)

    for old in glob.glob(os.path.join(here, '*.json')):
        if os.path.basename(old) != 'status.json':
            os.remove(old)

    kept = []

    def keep(name, case):
        path = os.path.join(here, name)
        with open(path, 'w') as handle:
            json.dump(case, handle, sort_keys=True, separators=(',', ':'))
        kept.append(os.path.basename(path))

    shared = {}
    for entry in ENTRY_ORDER:
        cases = prelude.get(entry, {})
        if len(cases) == 1:
            hash_, case = next(iter(cases.items()))
            shared[hash_] = True
            keep(f'prelude_{entry}.json', case)

    checked_only = 0
    total_calls = 0
    round_counts = {}
    by_fixture = {}
    for stem in stems:
        legs = leg_counts(scratch, stem)
        by_fixture[stem] = legs
        leg = 0
        for index, entry, case in per_stem[stem]:
            total_calls += 1
            name = None
            if digest(case) in shared:
                name = f'prelude_{entry}.json'
            elif stem in OWN_UNIT_COMMITTED and entry == 'evaluate_checked':
                name = f'{stem}_{entry}_{index}.json'
                keep(name, case)
            else:
                checked_only += 1
            if entry == 'evaluate_checked':
                if name is not None and leg < len(legs):
                    round_counts[name] = legs[leg]
                leg += 1

    distinct = set()
    for stem in stems:
        for _, _, case in per_stem[stem]:
            distinct.add(digest(case))

    bytes_total = sum(os.path.getsize(os.path.join(here, n)) for n in kept)
    status_path = os.path.join(here, 'status.json')
    status = json.load(open(status_path)) if os.path.exists(status_path) else {}
    status['round_counts'] = round_counts
    status['v7_rounds_by_fixture'] = by_fixture
    status['numbers'] = {
        'calls_dumped': total_calls,
        'byte_distinct': len(distinct),
        'committed': len(kept),
        'committed_bytes': bytes_total,
        'checked_but_not_committed': checked_only,
        'fixtures': len(stems),
    }
    with open(status_path, 'w') as handle:
        json.dump(status, handle, indent=2, sort_keys=True)
    print(f'froze {len(kept)} cases, {bytes_total / 1e6:.2f} MB, '
          f'{len(distinct)} byte-distinct of {total_calls} calls')


if __name__ == '__main__':
    main(sys.argv[1], sys.argv[2])
