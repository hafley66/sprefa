# Serializer lane report

## Converted families

| Family | Before | After | Result |
|---|---:|---:|---|
| `fixpoint_*_text` | 147 lines | 140 lines | `js_shape/2` field descriptors |

The descriptor renderer preserves field order, nested records, quoted atoms,
nulls, arrays, and scalar values. No new descriptor table was added, so no
sabotage receipt applies.

## Gates

```text
conformance: 346 PASS / 0 FAIL
TEXT_DOOR sweep stage 3: RUN total=246 identical=245 wrong=0 emitted_crash=0 rejection=1 no_oracle_log=0
TEXT_DOOR sweep final: FINAL total=246 final_identical=245 final_wrong=0 no_oracle_final=1
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=1 added=0 removed=0 (informational)
plunit: 602 tests passed, 1 test failed
```

The failed unit is the existing `diag_channel:uri_is_percent_encoded_file_scheme`
case. The sweep rejection is the existing `log_retraction_rejected` case.

## Not converted

The surrounding DDL, snapshot, arrival, relation-plan, aggregate, and expand/
dred serializers remain unchanged because this commit covers one serializer
family as required by the lane method.
