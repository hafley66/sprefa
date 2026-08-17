#!/usr/bin/env python3
# classify_unsupported.py: split the sweep manifest's `unsupported` bucket by
# what the SAME fixture expects on the ORACLE door.
#
#   throws(unsupported_construct(...))  -> both doors agree, intended negative
#   any behavioural expectation         -> DOOR SPLIT: the oracle runs the
#                                          program, the compiler will not
#
# conformance/go.pl loads every fixtures/*.pl and grades every fixture/5 term
# against the oracle with no skip list, so the two doors are graded over the
# same 452 rows. Run from v6/prolog.
import json, re, glob, collections


def fixture_expectation_kind():
    """name -> 'throws' | 'behaviour', read out of the fixture source text."""
    kind = {}
    for path in glob.glob('conformance/fixtures/*.pl'):
        src = open(path).read()
        # split on top-level `fixture(` occurrences
        parts = src.split('\nfixture(')
        for chunk in parts[1:]:
            name = re.match(r'\s*([a-z_0-9]+)', chunk)
            if not name:
                continue
            kind[name.group(1)] = 'throws' if 'throws(' in chunk else 'behaviour'
    return kind


def main():
    rows = json.load(open('compile/out/manifest.json'))
    kind = fixture_expectation_kind()
    buckets = collections.Counter()
    split = []
    for r in rows:
        if r['bucket'] == 'compiled':
            continue
        k = kind.get(r['name'], 'UNKNOWN')
        buckets[k] += 1
        if k != 'throws':
            split.append(r)
    total = sum(buckets.values())
    print('unsupported rows: %d' % total)
    for k, n in buckets.most_common():
        print('  %-10s %3d' % (k, n))
    print()
    print('DOOR SPLITS (oracle grades behaviour, compiler stops):')
    by_reason = collections.Counter()
    for r in sorted(split, key=lambda x: (x['file'], x['name'])):
        print('  %-34s %-50s %s' % (r['file'], r['name'], r['reason'][:60]))
        by_reason[re.match(r'([a-z_0-9]+)', r['reason']).group(1)] += 1
    print()
    print('door splits by stop name:')
    for n, c in by_reason.most_common():
        print('  %3d  %s' % (c, n))


if __name__ == '__main__':
    main()
