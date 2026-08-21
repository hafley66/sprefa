---
created: 2026-08-21
updated: 2026-08-21
type: task
assignee: chris
status: open
priority: high
epic: comptime-type-model
labels:
- area:dl6
- intent:decision
related: ['@semantic-type-identity', '@type-relation-ir', '@compiler-type-relations']
---

# Review canonical type identity

## Description

Review parameter, application, and derived type identities. Produce a decision only.

Review whether parameters, nested applications, and materialized declarations
share one canonical identity graph. Resolve the current split between
structural `application/2`, concrete names in `derived_from/2`, and template
parameters represented as named relation identities.

## Review Alternatives

- Use structural semantic terms for parameters and applications, with concrete
  generated names retained only as physical declarations.
- Use generated concrete declaration names as semantic identity.
- Keep separate structural and materialized identities with an explicit,
  lossless relation between them.

## Acceptance Criteria

- [ ] Review concrete row examples from the stress fixture.
- [ ] Choose the canonical identity of a parameter, application, and materialized declaration.
- [ ] Decide whether concrete generated names participate in semantic identity or only physical storage.
- [ ] Record the user-confirmed ruling or explicit deferral.

## Tests Run

Review card. No implementation CI.

## Implementation Notes

Do not modify canonical row production from this card.
