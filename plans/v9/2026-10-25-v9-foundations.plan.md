# v9 foundations: rework order

Chris, 2026-10-25: "how can we fix our foundations bc yea this is VERY rough start".
Inputs: `2026-10-21-v9-principles.md`, `2026-10-21-v9-core-types.md`, `SLOP.md`, `2026-10-25-v9-dict-candidates.md`, the whole-branch review.

## 1. Where the build left the principles

| principle / plan | built | gap |
| --- | --- | --- |
| 5. rows are ids; dense `Sid` inside, hash `Id` at boundaries; bytes in one arena | `Row = Vec<u64>` of blake3 ids; `Dict` = `HashMap<u64, Box<[u8]>>` | heap per row, per join key, per text; no `Sid` |
| 5. text once | ryi path copies each text ~4 times; wire allocates a `String` per text cell per reply | copies at both edges |
| 4. arrangements are the only shared state | dict and `Ryi::answered` are Rust-side maps | two Rust maps beside DD |
| 2. time = (event, round, wave) | (event, round) | no wave (no macrotime yet; fine for now) |
| bounded fixpoint protects one event's work | bound counts distinct inner rounds; still climbs with chain growth (SLOP P4 reopened) | wrong measure |
| 13. sync and async drivers | services run inside `commit` on the Core thread; ryi blocks every client up to 60 s | no async driver |
| 7. storage = update log + paged arrangements | one SQLite table per relation (Base, Derived, Request, Answer), whole `dict` loaded at open | storage is a relational mirror; no paging, no capture log |
| an engine error leaves state as before | `StepLimit` bricks the engine; a failed step-back rebuilds and may drift | recovery path is ad hoc |

## 2. Rework order (each step lands with its probe; numbers before and after, 3 runs)

| step | change | type sig sketch | gate |
| --- | --- | --- | --- |
| 1 ids | `Id(u64)` blake3 at store/wire/service edges; `Sid(u32)` dense, minted by one interner (arena-backed; candidate pick from the dict doc) | `Dict::intern(&[u8]) -> (Id, Sid)`; `Dict::bytes(Sid) -> &[u8]`; `Dict::sid(Id) -> Option<Sid>` | collision test; hafley_scm rows equal |
| 2 rows | engine rows hold `Sid`; arity-specialized or inline storage (`SmallVec<[u32; 4]>` first, DD columnar later); one input per Base/Answer rel instead of one filtered stream | `type Row = SmallVec<[Sid; 4]>`; `inputs: Vec<InputSession<Event, Row, Diff>>` | engine oracle tests; hafley_scm step ms and RSS vs today (58-60 ms, ~62 MB warm) |
| 3 text edges | ryi: stream stdout lines, borrow-deserialize, intern from `&str` (one copy); wire: decode text cells from the dict at serialization | `fn ryi_lines(child) -> impl Iterator<Item = Result<RyiEdge<'_>>>` | peak RSS on hafley_scm without the 22.8 MB buffer |
| 4 dict home | SQLite `dict` is the source; in-memory = bounded cache keyed by `Id` (`quick_cache` or `moka`) plus the live `Sid` arena for ids in the engine | `Dict { arena, by_id, cache, fresh }` | open time and RSS with a 1M-text store |
| 5 bound | replace the round count with a per-epoch work budget: updates fed back through each loop per outer epoch; over budget = `FixpointLimit`, rollback as now | `Bounds { max_loop_updates: u64 }` | the chain-growth test in `tests/0_reach.rs` passes; the runaway counter still trips |
| 6 services | services on their own threads (or tokio tasks); Core sends `call` batches and drains answers on `poll`; a commit returns after the engine settles what has arrived; ryi no longer blocks other clients | `IService::call` sends; answers arrive on a channel; `Core::tick` drains | two clients: one asks ryi on a big root, the other commits and reads within 100 ms |
| 7 recovery | the store is the source of truth (commits are atomic): any engine error past rollback rebuilds the engine by replay; `StepLimit` included | `Core::rebuild(&mut self) -> Result<(), Diag>` | fault-injection test per error kind |
| 8 storage shape | decide: keep SQLite tables for Base/Answer (inputs) and Derived (query surface), or store only inputs + DD capture log and derive on open | see question 5 | open time on hafley_scm and on the 200k probe |

Steps 1-3 are one arc (ids, rows, text). Step 5 and step 7 are independent of it. Step 6 changes the commit contract. Step 8 waits for a decision.

## 3. Decisions needed

1. Dict content: bytes (non-UTF-8 allowed) or UTF-8 text only. Text only opens `lasso` / `string-interner`.
2. Dense `Sid` inside the engine: yes (plan) or blake3 `u64` end to end.
3. Dict residency: all text in memory, or a bounded cache over SQLite.
4. Service answer batches: Arrow (principle 10 tier 1) or plain rows for now.
5. Storage: relational mirror of every relation (today), inputs only + replay, or inputs + DD capture log.
6. Commit contract with async services: a commit returns when the engine settles what has arrived (answers land in later events), or it waits for the services it woke (today's behaviour, bounded by a timeout).
