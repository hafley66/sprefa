#!/usr/bin/env python3
# coverage_matrix.py: which construct PAIRS does the corpus already compose?
# Reads compile/dl_view/*.dl6 (401 rendered fixtures) + dl/fixtures/golden-flex.dl6
# and reports, per construct pair, how many single files carry both.
# Run from v6/prolog.
import re, glob, itertools, collections, sys

DET = {
    'latest':     r'\blatest\(',
    'finalize':   r'\bfinalize\(',
    'next':       r'\bnext\(',
    'combine':    r'\bcombine\(',
    'not':        r'\bnot\(|(?<![\w])!\w+\(',
    'coalesce':   r'\bcoalesce\(',
    'pre':        r'\bpre\(',
    'seq':        r'\bseq\(',
    'now':        r'\bnow\(',
    'decode':     r'\bdecode\(',
    'regexp':     r'\bregexp\(',
    'match':      r'^\s*match\s',
    'probe':      r'\bprobe\(',
    'ts_query':   r'\bts_query\(',
    'edge_rule':  r'<\+',
    'level_rule': r'<-',
    'keyed':      r'\bkey\(',
    'log':        r'\blog\b',
    'keep':       r'\bkeep\(',
    'enum_decl':  r'rel\s+\w+\([^)]*;\s',
    'aggregate':  r'\b(count|sum|min|max|avg|group_concat|json_group_array|json_object)\(',
    'json_obj':   r'\{',
    'spread':     r'\[\.\.\.',
    'json_hole':  r'\$\w',
    'descent':    r'\*\*',
    'bind':       r':=',
    'cmp':        r'(>=|=<|==|\\==|=:=|=\\=|>|<)\s',
    'sh_decl':    r'^\s*sh\b',
    'bind_decl':  r'^\s*bind\b',
    'option_col': r'\?\s*[,)]',
    'list_col':   r'\blist\(',
    'module_path': r'\w+\.\w+\(',
}


def main():
    files = sorted(glob.glob('compile/dl_view/*.dl6')) + ['../dl/fixtures/golden-flex.dl6']
    present = collections.defaultdict(set)
    pair = collections.Counter()
    for f in files:
        try:
            src = open(f).read()
        except OSError:
            continue
        src = '\n'.join(l for l in src.split('\n') if not l.strip().startswith('#'))
        hit = [k for k, p in DET.items() if re.search(p, src, re.M)]
        for k in hit:
            present[k].add(f)
        for a, b in itertools.combinations(sorted(hit), 2):
            pair[(a, b)] += 1
    ks = sorted(DET)
    print('files scanned:', len(files))
    print()
    print('SINGLE-CONSTRUCT PRESENCE (files carrying it)')
    for k in ks:
        print('  %-12s %4d' % (k, len(present[k])))
    print()
    print('UNCOVERED PAIRS (each construct present somewhere, never in one file):')
    n = 0
    for a, b in itertools.combinations(ks, 2):
        if present[a] and present[b] and pair[(a, b)] == 0:
            print('  %-12s x %s' % (a, b))
            n += 1
    print('uncovered pair count:', n)
    total = sum(1 for a, b in itertools.combinations(ks, 2) if present[a] and present[b])
    print('coverable pair count:', total)


if __name__ == '__main__':
    main()
