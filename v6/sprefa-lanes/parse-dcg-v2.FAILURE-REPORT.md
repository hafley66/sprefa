# DCG parser migration failure

## Command

```sh
test "$(rg -c 'parse_dl:parse_dl_source' v6/prolog/compile/parse_dl_dcg.pl)" -eq 0
```

## Output

```text
rg: v6/prolog/compile/parse_dl_dcg.pl: IO error for operation on v6/prolog/compile/parse_dl_dcg.pl: No such file or directory (os error 2)
/opt/homebrew/bin/bash: line 1: test: : integer expected
```

## Result

The attempted DCG entry delegated to `parse_dl:parse_dl_source/5`, which is a
classic-parser fallback and violates the required migration constraint. The
attempt was removed before commit. No parser or toggle implementation is
available in this commit.
