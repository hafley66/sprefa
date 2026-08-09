#!/usr/bin/env bash
hits=$(grep -rn 'refusal' v6/prolog v6/tsv2/src v6/tsv2/tests --include='*.pl' --include='*.ts' 2>/dev/null | grep -v 'compile/out/' || true)
[ -z "$hits" ] || { echo "$hits"; echo "FAIL: refusal stem returned"; exit 1; }
echo "PASS: no refusal stem"
