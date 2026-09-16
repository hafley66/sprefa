# boop cards (accumulated, ride the next boop arc)

- [ ] EXIT TRIPWIRE (user 2026-08-11 "we should have a trip wire"): the
      on-exit epilogue knows rc, the worktree, and the base sha. When
      rc=0 AND (zero commits ahead of base OR dirty worktree), the epilogue
      appends an ALARM hail (kind=alarm, body names commits-ahead + dirty
      count), never a plain `done rc=0`. Measured incident: terra
      seeded-pre exited rc=0 with 12 modified files, 0 commits, red gates;
      the `done rc=0` hail looked like success.
- [ ] --brief validation at create time: refuse nonexistent or relative
      paths (in fix/boop-registry-kinds brief item 6, IN FLIGHT).
- [ ] registry row kinds lane/coordinator/native + agent register/done
      verbs (fix/boop-registry-kinds items 1-5, IN FLIGHT).
- [ ] lane wait --timeout default: session log 20260810.5 card.
