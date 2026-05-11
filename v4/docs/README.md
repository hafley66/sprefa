# V4 Docs

Reading order for getting back into v4 without replaying chat logs:

1. [V4 Language Vector](./v4-language-vector.md)
2. [V4 Design Guardrails](./v4-design-guardrails.md)
3. [V4 Core And Store](./v4-core-and-store.md)
4. [V4 CursorValue And WhereBytes Plan](./v4-cursor-value-where-bytes-plan.md)
5. [V4 Config And CLI](./v4-config-and-cli.md)
6. [V4 ghcache Integration](./v4-ghcache-integration.md)
7. [V4 rev Relation](./v4-rev-relation.md)
8. [V4 System Architecture Audit](./v4-system-architecture-audit.md)
9. [V4 Rule Query Semantics](./v4-rule-query-semantics.md)
10. [V4 SQL Rule Query Plan](./v4-sql-rule-query-plan.md)
11. [V4 Terse Relational Query Sessions](./v4-terse-relational-query-sessions.md)
12. [V4 Durable Mounted Query Plan](./v4-durable-mounted-query-plan.md)
13. [V4 Effect Output Retraction Plan](./v4-effect-output-retraction-plan.md)
14. [V4 Runtime Batching](./v4-runtime-batching.md)
15. [V4 Primitive Examples Plan](./v4-primitive-examples-plan.md)
16. [V4 V3 Parity Gaps](./v4-v3-parity-gaps.md)
17. [V4 Next Slices](./v4-next-slices.md)

Doc posture:

- `human-goals.md` is the current human-authored intent artifact.
- Current `v4/src`, `v4/tests`, and `v4/examples` are executable truth.
- `/Users/chrishafley/projects/sprefa-archive-20260428/README.md` and older `.sprf` files are historical target vocabulary only.
- Use SQL terms when SQLite has a 1:1 concept. Keep implementation trait-backed.
- Avoid new glossary terms unless they map immediately to existing project words.
