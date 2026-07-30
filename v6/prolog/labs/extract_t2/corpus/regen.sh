#!/usr/bin/env bash
# regen.sh : rebuild every DERIVED corpus artifact from the vendored sources.
#
#   bash v6/prolog/labs/extract_t2/corpus/regen.sh
#
# Vendored, byte-unmodified, and NOT regenerated here (they are the inputs):
#   openapi-petstore.json   17,106 B  curl https://petstore3.swagger.io/api/v3/openapi.json
#   avro-interop.avsc        1,238 B  apache/avro share/test/schemas/interop.avsc
#   struct.proto             4,317 B  protoc 35.1's own google/protobuf/struct.proto
#   descriptor.proto        58,877 B  protoc 35.1's own google/protobuf/descriptor.proto
#   repo_contracts/**, repo_consumer/**  protoc 35.1's source_context.proto,
#                                        type.proto, any.proto, laid out as two
#                                        toy repos for the cross-repo receipt
#
# Derived here, by ONE bought tool (protobufjs `pbjs -t json`), because protoc
# emits a BINARY FileDescriptorSet and has no JSON output flag of any kind:
#   proto-struct.json, proto-descriptor.json,
#   proto-repo-contracts.json, proto-repo-consumer.json
#
# pbjs is pinned. Its stderr carries npm's own config warnings, which is why
# every invocation discards it -- an earlier run redirected 2>&1 and put npm
# warnings in front of the JSON.

set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$HERE"

PBJS_PKG="protobufjs-cli@1.1.3"
pbjs() { npx --yes -p "$PBJS_PKG" pbjs "$@" 2>/dev/null; }

echo "== proto -> json descriptors (pbjs $PBJS_PKG) =="
pbjs -t json struct.proto                > proto-struct.json
pbjs -t json descriptor.proto            > proto-descriptor.json

# The cross-repo pair. `-p <root>` is the include path, so each repo resolves
# only its OWN tree. The consumer's type.proto imports source_context.proto,
# which lives in the OTHER repo and is therefore UNRESOLVED here -- deliberately.
# That is what makes the join in xrepo.dl6 a real cross-repo join rather than a
# lookup inside one merged document. See the verdict, Q3.
pbjs -t json -p repo_contracts repo_contracts/google/protobuf/source_context.proto \
  > proto-repo-contracts.json
pbjs -t json -p repo_consumer  repo_consumer/google/protobuf/type.proto \
  > proto-repo-consumer.json

for artifact in proto-struct.json proto-descriptor.json \
                proto-repo-contracts.json proto-repo-consumer.json; do
  python3 -c "import json,sys; json.load(open(sys.argv[1]))" "$artifact"
  printf '%-32s %8s bytes\n' "$artifact" "$(wc -c < "$artifact" | tr -d ' ')"
done
