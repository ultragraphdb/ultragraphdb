# Group 0, task 0.6: the session registry's recorded counterexample

Stage 5.8's group 4 recorded that `ultragraphdb_common_system`'s property
`test_the_registry_agrees_with_its_model` drew "Session 2 of 1: left Some(Busy), right Some(Registered)" under random
seeds (`openspec/changes/archive/2026-11-17-stage-5-8-system-group/notes/group-4.md`, *Found wanting* 5 and the mutation
table). `notes/feasibility.md` carries it as blocker B3. Examined on 2026-11-18.

## Verdict

The registry is right, and the property's harness was wrong. The case was corrected in commit `758163d3`
(2026-11-16 19:01), fifteen minutes before group 4's note was committed (`4d689201`, 19:16), and its shrunk case is the
unit test `test_session_2_after_a_restart_is_quiet_from_the_runs_first_turn`
(`tests/types/system_group/system_group_sessions_tests.rs`). Group 4's mutation run counted against `common_system@443a7f54`
(17:30), a tree without that correction, and its note was never brought up to date. B3 is closed. This task changed no
source file.

The real-time waits that `443a7f54` removed had no part in it: that commit changed
`system_session_session_property_tests.rs` and `tests/property/utils/mod.rs`, and the registry's property reads neither a
wall clock nor a socket. The failure is a function of the drawn sequence alone.

## The sequence

`sessions_max` 1; Session 1 of principal 1; Restart; Pass of twice `transaction_ms` (100,000 µs); Session 2 of principal 1.

Hegel samples a state machine's invariants and does not run them after every rule. The harness of that time spawned the
restarted group's run and recorded the model's listening instant at once, while the run had not been polled. A Pass drawn
right after the Restart moved the clock before the run's first turn, so the group began to listen, and its quiet time
began, at the moved instant. Session 2 needed an eviction inside that quiet time and was answered code 14. The model
counted the quiet time from the spawn and registered.

`PROTOCOL.md` section 11.5 decides it for the registry: "For twice `transaction_ms` after the system group begins to
listen, when it holds no mark of the requests then on their way, it evicts no session, refusing a Register that would
need an eviction with code 14". The group begins to listen as `SystemGroup::run` begins (stage 5.8's `notes/group-3.md`,
*Found wanting* 2). `758163d3` has the harness wait on a `LatestClose` after the spawn, so the run takes its first turn
before the model records the instant.

## Evidence

- **The current tree does not draw it.** The `property` binary Bazel built, run with the environment of
  `scripts/property_random.sh` and libtest's filter `test_the_registry_agrees_with_its_model`: 400 of 400 runs passed, 40
  cases of 30 steps each, 16,000 cases. `#[hegel::test(test_cases = 40)]` takes precedence over `HEGEL_TEST_CASES` from
  hegeltest 0.52 on, so the case count cannot be raised from the environment; the runs were raised.
- **The tree without the correction draws exactly the record.** With the four lines `758163d3` added to `Registry::start`
  taken out by a temporary edit, the binary built and copied aside, and the edit undone before any run: 44 of
  100 random runs failed, every one with `sessions_max` 1 and the message "Session 2 of 1", left `Some(Busy)`, right
  `Some(Registered)`. No other message appeared. 38 shrank to the sequence above, 2 to it with the Pass split in
  two (1 µs, then 99,999 µs), and 4 to its sibling with the Restart first (Restart; Pass of 100,000 µs; Session 1;
  Session 2). Each is a Pass between a Restart and the run's first turn.
- **The shrunk case is a unit test and passes**: `test_session_2_after_a_restart_is_quiet_from_the_runs_first_turn` holds
  code 14 at twice `transaction_ms` after the spawn, code 14 one microsecond before the quiet time's end counted from the
  run's first turn, and the registration with session 1's eviction at its end.

## Sequences read for a separation

The model (`Model` in `system_group_sessions_property_tests.rs`) against `asked_session`, `SessionMarks` and
`MetadataGroup::register_session`, by sections 11.5 and 13.23. None separates the two on the current tree.

| Condition | Registry | Model | Unit test pinning it |
|---|---|---|---|
| A full registry whose every session is marked | `register_session` finds fewer sessions to evict than it needs, `Full`, answered code 14 | no unmarked session, Busy | `test_a_registry_full_of_marked_sessions_refuses_a_session`, `test_a_registry_of_one_session` |
| A mark's `writes` | `protects` reads it; a Check of kind 2 always marks with `writes` true, and the machine sends no in-process commit | every mark protects | the same |
| An eviction appended, carried by no close yet | `EVICTIONS_PENDING_MAX` is 65,536, and a case appends at most 30; `full` and the held identity are read from every command appended | the session is gone at once | `test_an_evicted_identity_registers_again` |
| The quiet time after a start | `full && is_quiet(now)`, the half-open interval from the run's first turn | `now < listened + 2T`, `listened` read after the run's first turn | `test_the_quiet_time_evicts_no_session`, `test_session_2_after_a_restart_is_quiet_from_the_runs_first_turn` |
| A closed connection's mark inside twice `transaction_ms` | `take` expires the marks at the turn's reading before the Session is decided; `close` keeps the instant of the first close | `expire` before the Session; the close's instant kept by `or_insert` | `test_a_closed_connections_marks_hold_the_registry_for_twice_the_limit`, `test_a_full_registry_evicts_a_closed_connections_session_at_twice_the_limit` |
| A restart | sessions replayed in their order; marks, closed connections and connection numbers begin again | sessions kept; marks, closed connections and open connections cleared | `test_a_restart_keeps_the_sessions_and_loses_the_marks` |
| The order of refusals | held identity, unknown principal, then full | the same | `test_a_sessions_refusals_follow_their_order` |

No test was added: the shrunk case and each condition above already hold a unit test, and a further one would restate them.

## Verification

- `bazel test //ultragraphdb_common/ultragraphdb_common_system/...`: 5 of 5 targets pass.
- The clippy aspect on the package: clean.
- `property` and `property_catalog`, each with its `BUILD.bazel` arguments and fresh seeds: 20 of 20 runs each.
- rustfmt: no source file differs from HEAD.

## Seen, not changed

- `tasks.md` 0.6 and `notes/feasibility.md` B3 still describe the case as open.
- The archived group 4 note still reads as an open finding; its archive is left as it was written.
