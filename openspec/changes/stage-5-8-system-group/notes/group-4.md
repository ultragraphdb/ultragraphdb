# Stage 5.8, group 4: storage hosts the group, `ultragraphdb_common_storage`, `ultragraphdb_storage`, `ultragraphdb_storage_tests`, `ultragraphdb_cluster_partition`

Base commit `443a7f54`. This file records task 4.1, the Ultra Style lift of the group's crates, and tasks 4.2 to 4.6, the
five phases: what the group adds, how the old and new settings live side by side until group 7, the mapping of every
corner case and law to its test, the mutation runs, and what was found wanting. Counts are those Bazel's test logs report.

Groups 5 and 6 and a performance change inside `ultragraphdb_common_system`'s catalog code ran beside this group, on
disjoint crates; the group touched `common_system` at its start surface alone (below).

## Task 4.1: the audit against `openspec/protocols/ULTRA_STYLE.md`

Scope: all of `src/` of `ultragraphdb_common_storage` (43 files), `ultragraphdb_cluster_partition` (24 files) and
`ultragraphdb_storage` (`main.rs`, `service.rs`), as at `443a7f54`. `ultragraphdb_storage_tests` holds tests alone. Group 3
lifted none of them; stage 5.5's review (`archive/2026-11-09-stage-5-5-three-role-refactor/notes/group-12.md`, 12.3) had
read them once.

How it was measured: every file read in full against the rules, with the checklist of 12.3's search patterns (`dyn`, `fn(`,
recursion, `loop`/`while`, unbounded channels, `assert!`, `let _`, `.ok()`, dropped sends, casts, cells and atomics,
`async fn` cancellation, `..=`, formatted log messages, division); rule 4 by a script counting each body's lines, comments
and blank lines left out; `#[must_use]` and `# Errors` by a script over every `pub fn`. Clippy under Bazel (the workspace's
table) builds the three crates clean before and after.

### `ultragraphdb_common_storage`

| Rule | Result | Evidence |
|---|---|---|
| 1 | conforms | no recursion, `dyn`, function pointer or stored callback |
| 2, loops | **gap, closed** | five loops did not say what ends them: `retention.rs:55`, `partition_core_reads.rs:198`, `:201`, `:448`, `partition_core_frames.rs:247`; each now does |
| 2, queues and tables | **gap, closed in part** | the core's maps (`partition_core/mod.rs:263-273`) and the detached registration tasks (`partition_core_remote.rs:42`) now say what bounds them; a capacity with a refusal is not added (left open) |
| 3, planes | **gap, closed** | `lib.rs` states the planes: requests, Scans and registrations data plane, allocating per request within the connection's held bytes, credit and turn budget; start, recovery, catalog installs, leases, reports and retention control plane |
| 4 | conforms | longest `read_operation` 68 lines (`utils/operation_of.rs:45`) |
| 5, 8, 10 | conforms | no run-time `assert!`; no macro or `cfg`; one `#[expect]` with its reason |
| 7 | **gap, closed in part** | the folds of `partition_core_one_frame.rs:215`, `:276` and `error_reply.rs:65` now say why they fall back; `partition_core_reads.rs:467`'s `next - 1` states the invariant it rests on. Left open: a reply that cannot be framed is only logged (`partition_core/mod.rs:727`, `partition_core_remote.rs:161`), which changes behaviour to answer |
| 9 | **gap, closed** | `OpenScan.held`'s `Arc` (`open_scan/mod.rs:51`) says why |
| Casts | **gap, closed** | `phase as usize` (`commit_phases/mod.rs:82`, `:88`) says why it is lossless |
| `#[must_use]` | **gap, closed** | `Retention::due`, `ScanBoard::take_turn`, `next_due`, `next_closed`, `end_transaction` |
| Division | **gap, closed** | `MESSAGES_PER_TURN_MAX = TURN_BYTES / HEADER_SIZE` is exact, checked by `const _: () = assert!(TURN_BYTES.is_multiple_of(HEADER_SIZE))` |
| Structure | **found** | `src/utils/partition_address.rs` is in no module tree (an uncompiled copy of `common_service_config`'s function since `24971eb8`). Not deleted: removing it is the user's call |

### `ultragraphdb_cluster_partition`

| Rule | Result | Evidence |
|---|---|---|
| 1, 4, 5, 7, 8, 10 | conforms | no recursion or stored callback; longest `change_references` 65 lines; no run-time assert, `unwrap`, `let _ =`, macro, `cfg` or `#[expect]` |
| 2 | **gap, closed in part** | `PartitionHost`'s tables (`partition_host/mod.rs:71-87`) and `Home`'s (`home/mod.rs:39-41`) now say what bounds them. Left open: `Fanout::reverse_counted`'s `counts` grow with a tenant's distinct objects; the mailbox's inbox is unbounded with its bound stated (open since 5.5) |
| 3 | **gap, closed** | `lib.rs` states the per-write, per-turn and per-read allocations and that transactions take the pipeline's buffers |
| 9 | left open | `ReverseQuery` derives `Copy` at an estimated 72 bytes (`reverse_query.rs:12`), unmeasured; dropping `Copy` changes a public type |
| `#[must_use]` | **gap, closed in part** | added to the pure getters: `PartitionHost::new`, `with_first_ticket`, `last_ticket`, `partition`, `mailbox`, `delivered`, `needs_sync`, `is_stopped`, `resends`, `parked`, `into_parts`, `bound_limit_ms`, `read_waits`, `bound_ticket`, `bound_read`, `discard_ts`, `closed`, `watermark`; `Partition::new`, `id`, `placement`, `applier`, `owns_run`; `Fanout::new`. Left open: the `take_*` and `sweep_bound` methods, whose callers may drop what they take |
| Swappable arguments | left open | same-typed tuples (`bind`'s, `commit_drop`'s, `BoundKey`) are public signatures |
| Division | **gap, closed** | `limit_us / 1_000` (`partition_host/mod.rs:453`) says it rounds down |
| Naming | left open | single-letter closure parameters (`|r|`, `|p|`, `|e|`) |

### `ultragraphdb_storage`

| Rule | Result | Evidence |
|---|---|---|
| 1, 4, 5, 8, 10 | conforms | the `&dyn Display` closure states its reason; longest `serve_partition` 68 lines; four `#[expect(clippy::disallowed_methods)]` with reasons |
| 7, a panic on a setting | **gap, closed (a defect)** | `send_report`'s `now() + peers.cluster().deadline()` panicked for a `join_deadline_ms` past what an `Instant` holds, the defect group 2 fixed in `retry_until`; it now adds with `checked_add`, a deadline the clock cannot name never passing. No test reaches it apart from the process: the line runs only when a report goes unanswered past the deadline |
| 2 | **gap, closed** | `lease_ahead`'s `while` states its end; `held_versions`' unbounded `loop` became a `for` to `VERSIONS_PER_TENANT_MAX` in task 4.4, refused past it |
| 2, connections | left open | the role addresses take no `ConnectionBound`: the bound is a decision |
| 3 | **gap, closed** | `main.rs` states the planes |
| 7, dropped sends | **gap, closed** | the two `drop(lost.send(…))` without a reason, and the detached thread, say why |
| 9 | **gap, closed** | the partition's `Arc` says who shares it |
| Division | **gap, closed** | `ms / 2` says it rounds down |
| Logging | left open | `"{cpu}"` (`service.rs:239`): the scenarios read its text |

### The lift and its evidence

Small and local: comments, attributes, one compile-time check, one arithmetic fix, no signature changed. So the group made
the lift and went on. Files, each lift line its own hunk: `common_storage/src/lib.rs` (the planes paragraph),
`types/retention.rs`, `types/partition_core/{mod.rs, partition_core_reads.rs, partition_core_frames.rs,
partition_core_remote.rs, partition_core_one_frame.rs}`, `types/open_scan/mod.rs`, `types/commit_phases/mod.rs`,
`types/scan_board/mod.rs`, `utils/error_reply.rs`; `cluster_partition/src/lib.rs`, `types/partition_host/{mod.rs,
partition_host_bound.rs, partition_host_read.rs}`, `types/partition/mod.rs`, `types/fanout/mod.rs`, `types/home/mod.rs`;
`ultragraphdb_storage/src/main.rs`, `src/service.rs` (`send_report`'s deadline, the comments).

| Target | Before, at `443a7f54` | After the lift |
|---|---|---|
| `common_storage:ultragraphdb_common_storage_tests` | 213 passed | 213 passed |
| `cluster_partition:ultragraphdb_cluster_partition_tests` | 100 passed | 100 passed |
| `cluster_partition:bdd` | 34 scenarios, 68 steps | 34 scenarios, 68 steps |
| `cluster_partition:workload_seeds`, `workload_faults` | 1, 2 passed | 1, 2 passed |
| `bazel build` of the three crates | | passes |

## Tasks 4.2 to 4.6: the five phases

### What was built

`ultragraphdb_storage` (`service.rs`):

- `storage_role` starts the system group when its settings hold `[system]`: `Hosted::start` reads `[metadata]`,
  `[sequencer]` and `cluster.partition_count` (`system_start`, `metadata_config`, as the frontend's `owner_start` reads
  them), refuses a `system.role_address` that does not parse naming the setting (exit 78), starts `start_system` over
  `system.data_dir` on core `partition_count` — the core after the partitions' (design D3) — and, once it has recovered,
  listens at `system.role_address`, serving each connection with a `SystemSession` (`Server` of `Endpoint::Frontend`, the
  group's role address in this stage) in a task the accepting task holds. Only then do the partitions start (design D10).
- Partitions dial the address `dialled` gives, the hosting instance's included (design D4).
- Readiness: `/ready` once every partition has recovered and joined, the group listening before any started (D10).
- The stop: every partition flushes, then `Hosted::stop` drops the accepting task, which closes every connection and lets go
  of every way to the group, and waits for the group's thread to end, at most `GROUP_STOP_LOOKS` (5 s).
- A failed log of the group: `Hosted::look`, in the role's loop and while it waits for readiness, takes the group's outcome;
  `RoleError::io` stops the partitions through the ordinary path (`stop_the_rest`), and the process exits 74. A lost peer
  stops nothing in the hosting instance.
- A non-hosting instance exits 69 once the group is gone for `join_deadline_ms`; every such failure is named with the
  group's address (`naming`): "the system group at tcp://…: …".
- Catalog versions are read across Catalog frames through `CatalogFrames` (design D13), credit granted per frame, at most
  `CATALOG_FRAMES_MAX` frames; a recovery reads at most `VERSIONS_PER_TENANT_MAX` versions of a tenant. A version read
  is logged (`a catalog version read`, with tenant, version and epoch).

`ultragraphdb_common_storage`: `Peers::system` (was `frontend`) and `SystemAsk` (was `FrontendAsk`), names of what a core
asks the system group; the Drop's `catalog` passed to the host.

`ultragraphdb_cluster_partition`: `PartitionHost::commit_drop` takes the Drop's `catalog`.

Outside the four crates, each forced by the work:

- `ultragraphdb_transactions_txn`: `Prepared::drop` takes `(tenant, catalog)` and writes `catalog` into the Drop's record.
- `ultragraphdb_transactions_commit`: the commit step's `validate` refuses a record whose first operation drops a tenant's
  data or everything in it (operations 3 and 4) when its `catalog` is not the version in effect at the open epoch, with
  `Conflict` kind `Catalog`: code 2, kind 5. That is where a round is decided, so the Drop's check is decided at the round
  as design D14 and `PROTOCOL.md` §6.5 have it; `cluster_partition` has no hook at the round of its own. The call sites of
  `Prepared::drop` in the two crates' tests take `(TENANT, 0)`.
- `ultragraphdb_common_system`, its start surface alone: `SystemStart::with_core` and `core` (default `SystemStart::CORE`,
  1, what `OWNER_CORE` was), read by `start_system`, so a storage process pins the group after its partitions; the frontend
  is unchanged. One test, `system_start_tests::test_a_start_holds_its_core`.

### How the old and new settings live side by side until group 7 (design D2)

Which address a storage instance's partitions dial (`service.rs`, `dialled`):

| The settings hold | The partitions dial |
|---|---|
| `[system]` | `cluster.system_address`, required: refused naming it, exit 78, when absent |
| `cluster.system_address`, no `[system]` | `cluster.system_address` |
| `cluster.frontend_address` and neither of the two | `cluster.frontend_address`, the frontend hosting the group there, as before this group |
| none of the three | refused naming `cluster.system_address`, exit 78 |

An instance hosts the group exactly when its settings hold `[system]`. Every profile and every test of today names
`cluster.frontend_address` and no `[system]`, so each runs as it did; the frontend still hosts the group at
`frontend.role_address` until group 5 changes it. Group 7's switch removes the third row with `cluster.frontend_address`.
Nothing checks today that a hosting instance's `cluster.system_address` names its own `system.role_address`: the config
service's System address check is group 7's.

### Phase 1 and 2: the surface and the failing run

The surface: `Prepared::drop`'s new argument, `PartitionHost::commit_drop`'s, the commit step's check as a bare
`unimplemented!()` reached only by a record dropping a tenant's data or everything in it, and `SystemStart::with_core` and
`core` as bare `unimplemented!()`; `bazel build` passed. The suite against it:

| Target | Phase 2 |
|---|---|
| `cluster_partition:ultragraphdb_cluster_partition_tests` | 105 run: 99 passed, 6 failed — 5 at the stub (`commit_step_round.rs:688`), the 6th (`test_a_drop_of_a_predicate_or_a_graph_is_not_checked`) refused `(2, 5)` by the existing transaction check, which a test setup error caused (below) |
| `cluster_partition:bdd` | 35 scenarios: 34 passed, 1 failed at the stub |
| `cluster_partition:property` | 1 test, failed at the stub on its first case |
| `common_system:ultragraphdb_common_system_tests` | 104 run: 103 passed, 1 failed at the stub (`system_start.rs:74`) |
| `common_storage:ultragraphdb_common_storage_tests`, `transactions_commit:bdd` | the existing Drop tests of kind 3 and 4 failed at the stub, as they exercise the new check |

The storage binary has no unit tests; its scenarios are processes in `storage_tests`. **Deviation:** `service.rs` was written
before its `@system` scenarios were run, so they were not observed failing against a phase-1 binary; their first run is
below.

### Phase 3: the audit of the inputs

The Drop tests draw or name versions 1 to 4 taking effect at epochs 3 to 6, so a version, its effective epoch and the round's
epoch never coincide except in C4.14, where the round's epoch is the effective one by design; `catalog` one below and one
above the version in effect; the round decided at an epoch past the last effective one (C4.11) and exactly at it (C4.14);
kinds 1 to 4. One setup was wrong and was corrected, no assertion weakened: the facts the Drops remove were written after
the versions took effect, so the write executed against version 0 under version 4 and was refused `(2, 5)` by the existing
transaction check; they are written first now, and every Drop test asserts the fact held before the Drop. Whether a Drop
removed anything is judged by reading the fact back through a transaction (`hosts::holds`), not by the store's bytes, which
a drop's marker and a round's bookkeeping change either way.

### Phase 4: the suite at its end

| Target | Before (`443a7f54`) | After | Uncached (`--nocache_test_results`, `--local_test_jobs=1`) |
|---|---|---|---|
| `cluster_partition:ultragraphdb_cluster_partition_tests` | 100 | 105 passed | 3.4 s |
| `cluster_partition:bdd` | 34 scenarios, 68 steps | 35 scenarios, 70 steps passed | 0.7 s |
| `cluster_partition:property` (new) | | 1 passed | 0.7 s |
| `transactions_txn:ultragraphdb_transactions_txn_tests` | 160 | 161 passed | 0.1 s |
| `transactions_commit:ultragraphdb_transactions_commit_tests` | 272 | 273 passed (`test_a_drop_of_a_tenants_data_is_decided_against_the_version_in_effect`, added by the mutation fix pass) | 3.0 s |
| `transactions_commit:bdd` | 99 scenarios | 99 scenarios passed | 1.0 s |
| `common_system:ultragraphdb_common_system_tests` | 103 | 104 passed | 1.5 s |
| `common_storage:ultragraphdb_common_storage_tests` | 213 | 213 passed | 37.8 s: over the ten seconds before this group; its edits here are renames in three test files |
| `storage_tests:system_bdd` (new, integration) | | 6 scenarios: 5 passed, 1 failed (below) | 5.4 s |
| `storage_tests:system_partitions_bdd` (new, integration) | | 4 scenarios, 4 passed | 6.6 s |

The property was shown to bite: with the commit step's Drop check made never to hold, by a temporary edit since reverted, the
target failed on its first counterexample, "kind 4, catalog 1, history [(1, 6), (2, 7), (3, 10), (4, 11)], round 10", a
Drop naming version 1 under version 3 that committed.

The one failing `@system` scenario, "Storage is ready before any frontend exists", fails because the config service exits
with code 78 at its start on a target of one hosting storage instance and nothing else: its checks are the old ones until
group 7's switch (design D2). It is the one `@system` scenario that runs the config service; every other is served by the
config double. Its first pass is group 7's gate.

The existing `storage_tests:bdd` target leaves the `@system` features out and keeps its scenarios; one expectation follows a
message this group renamed: "the system group delivered close …" for "the frontend delivered close …". It was not run here:
it starts the frontend binary, which group 5 changes.

### Scenarios bound

`ultragraphdb_storage_tests/features/system/` (tag `@system`; `hosting.feature` and `role_binaries.feature` tagged `@hosting`,
run by `system_bdd`; `partitions.feature` tagged `@partitions`, run by `system_partitions_bdd`; `bdd` now leaves `@system`
out):

- system-group, "One storage instance hosts the system group": "The hosting instance serves the books", "A system table
  without its directory stops the start". "The frontend holds no book" is group 5's (the frontend binary's dependencies).
- system-group, "The system group runs on a core of its own over logs of its own": "The group's logs lie beside the
  partitions'", over a data root of the test's own standing for the container's `/data` (group 7 runs the container).
- system-group, "The hosting process starts the group first and stops it last": "Storage is ready before any frontend
  exists", through the config service itself.
- partitions, "A storage process hosts partitions over bucket ranges": all four.
- role-binaries, "Readiness, health and shutdown", storage's parts: "Storage is not ready before recovery", "Storage flushes
  on shutdown".

`ultragraphdb_cluster_partition/features/schema-changes/drop_catalog.feature`: schema-changes, "A Drop aimed at an earlier
schema is refused by the partition", over hosts in one process (non-integration, passes in this gate).

Not bound here, each named under *Not done*: "The books recover in the storage process", "A partition's sync holds up no
close", "A partition recovers from the group of its own process", "A failed log of the group stops the process"; the
role-binaries scenarios of "Each role runs one runtime per core" and "Configuration is explicit and validated at start"
for storage keep their existing bindings in `bdd` (`features/role_binaries/role_binaries.feature`), whose text the
change's spec repeats, except "Every runtime of a pinned role holds a CPU of its own", bound nowhere for storage.

### Corner cases C4.1 to C4.16

| Case | Test |
|---|---|
| C4.1 | `system_bdd`: "Storage is ready before any frontend exists", "The hosting instance serves the books" (the hosting log's `the system group listens` precedes its partitions') |
| C4.2 | "The hosting instance serves the books": the non-hosting instance logs no listener; its partitions join at `cluster.system_address` |
| C4.3 | "Two storage processes share the partitions": Members at the group's address names the hosting instance's partitions, which joined there |
| C4.4 | not covered (*Not done*) |
| C4.5 | not covered over the process (*Not done*); the path is `Hosted::look` taking the group's `RoleError::io` |
| C4.6 | every `@system` scenario runs no frontend and no compute, and the hosting instance stays ready |
| C4.7 | "A claim on a held partition is refused": exit 69 past `join_deadline_ms`, naming the group's address; the deadline less one tick is not tested |
| C4.8 | not covered: group 8's failure runs |
| C4.9 | "Storage flushes on shutdown": a restart replays nothing; the group's stop after the partitions is not observed apart |
| C4.10 | `bdd`: "A subscription past the closes held exits with its code", its message now "the system group ended …" |
| C4.11 | `partition_host_bound_tests::test_a_drop_naming_the_version_in_effect_commits` |
| C4.12, C4.13 | `test_a_drop_naming_another_version_is_refused_and_removes_nothing` |
| C4.14 | `test_a_drop_at_the_epoch_a_version_takes_effect_names_that_version` |
| C4.15 | `test_a_drop_of_a_predicate_or_a_graph_is_not_checked` |
| C4.16 | `partition_host_bound_tests::test_a_drop_of_a_predicate_or_a_graph_finding_nothing_is_not_checked`; `transactions_commit`'s `commit_step_store_tests::test_a_drop_of_a_predicate_or_a_graph_finding_nothing_is_not_checked`, with an empty transaction still checked beside it; `transactions_txn`'s `test_a_drop_is_a_drop_command_even_when_it_finds_nothing_to_remove` |
| beside them | `test_a_tenant_holding_no_version_drops_under_version_zero`; `transactions_txn`'s `test_a_drop_names_its_catalog_version` |

### Laws

| Law | Test |
|---|---|
| L4.1 | `cluster_partition/tests/property/types/partition_host/partition_host_bound_property_tests.rs`, `test_a_drop_commits_exactly_under_the_version_in_effect_at_its_round`: 0 to 4 versions with strictly rising effective epochs from 3 to 12, the round's epoch 3 to 13, kind 1 to 4, `catalog` 0 to one above the highest; the model (`in_effect`) is the highest version whose effective epoch is at or below the round's. Target `property`, tagged `property-test` |

`ultragraphdb_common_storage` and `ultragraphdb_storage` hold no law (`notes/laws.md`, group 4) and gain no `property` target.

### Phase 5: fuzzing and mutation testing

The group adds no decoder of bytes: the Catalog frames are read by `ultragraphdb_protocol_wire`'s `CatalogFrames`, whose
fuzzing is group 1's. No fuzz target.

`scripts/mutants.sh <crate> --diff 443a7f54`, one run at a time, outputs kept under the scratchpad and `mutants.out/`:

| Crate | Runs | Outcome |
|---|---|---|
| `ultragraphdb_cluster_partition` | 1 | 3 mutants, 3 caught (`commit_drop` to `Ok(0)` and `Ok(1)`; `resend` to `()`) |
| `ultragraphdb_transactions_txn` | 1 | 1 mutant (`Prepared::drop` to `Ok(Default::default())`), unviable: `Prepared` has no `Default` |
| `ultragraphdb_transactions_commit` | 2 | run 1: 10 mutants, 9 caught, 1 missed, `drop_stale` to `false`, since the crate's own suite held no Drop of a tenant's data naming another version (the partition's tests did). Killed by `commit_step_store_tests::test_a_drop_of_a_tenants_data_is_decided_against_the_version_in_effect`; run 2 (`--re drop_stale`): 3 mutants, 3 caught |
| `ultragraphdb_storage` | 2 | run 1: Cargo could not build the binary alone (`futures::channel` is behind `futures`' `std` feature, which Bazel's unified build has and `cargo build -p` not); the manifest takes `features = ["std"]`, as `common_compute`'s does. Run 2: 59 mutants, 42 missed, 17 unviable, 0 caught. The true state: the binary has no tests Cargo runs; its code is exercised by `storage_tests`' process targets over the Bazel-built binary, which cargo-mutants does not run. No survivor is equivalent; each is a line those targets cover or leave uncovered, for the stage's final verification |
| `ultragraphdb_common_storage` | 3, the bound | no mutant tested: run 1 and 2 stopped in the unmutated build (`futures::channel` as above, then the tests' `futures::executor`), run 3 in the tests' `futures::poll`, behind `async-await`. The manifest now takes `std` and, among the dev-dependencies, `async-await`, `executor` and `std`, so a run builds; the ledger allows no fourth run for base `443a7f54`, so the crate's run belongs to the stage's final verification. Group 3's run of this crate stopped in the same baseline build |
| `ultragraphdb_common_system` | 1, `--re 'system_start\.rs|system_thread\.rs'` | no mutant tested: the unmutated baseline failed its `property` target under Cargo, which runs Hegel without `HEGEL_DERANDOMIZE`: `system_group_sessions_property_tests::test_the_registry_agrees_with_its_model` drew a counterexample, "Session 2 of 1: left Some(Busy), right Some(Registered)" (`tests/property/types/system_group/system_group_sessions_property_tests.rs:243`). Not this group's code: reported, not touched. `with_core` and `core` are pinned by `system_start_tests::test_a_start_holds_its_core`, which reads both a default and a set value. This run counts against the ledger the concurrent change in `common_system` shares (`common_system@443a7f54`) |

No entry was added to `.cargo/mutants.toml`.

## Found wanting

1. **A Drop of a predicate or a graph that finds nothing to remove is refused under a nonzero catalog version.**
   `Prepared::drop` records such a Drop with no operation (`ChangesetApplier::prepare_drop`: "dropped as nothing"), so its
   record reads as an empty transaction, and the commit step's existing transaction check refuses it with code 2, kind 5
   whenever the tenant has a version other than 0 in effect: its record's `catalog` is the Drop's, 0. `PROTOCOL.md` §6.5 has
   kinds 1 and 2 checked by no version, and C4.15 commits them. Found by C4.15's first run; not fixed, since it is the
   existing transaction check's behaviour and outside D14: the tests of C4.15 and L4.1 drop a predicate and a graph that
   hold data. A fix would keep the transaction check off a record changing and reading nothing.
   **Fixed 2026-11-16.** The proposal, not its record, now says it is a Drop: `Prepared::drop` marks its proposal a Drop
   command (`Prepared::is_drop_command`, `Proposal::is_drop_command`, false by default), and `CommitStep::validate`
   checks a record naming no operation against the version in effect only when its proposal is no Drop command. A
   transaction's record that changed and read nothing is the same bytes but for its session and still conflicts naming
   the catalog (§11.13, *Transactions against a version*), so the fix keys on the command, not on an empty record as
   proposed above. Kinds 3 and 4 always name their operation and keep D14's check against the version they name. Tests:
   C4.16.
2. **"A write to the metadata log fails" has no means over the process.** system-group, "The hosting process starts the
   group first and stops it last", and C4.5 ask a failed write of the group's log; the binary has no fault injection, and a
   file of the log cannot be made to fail a write once open. The scenario is not bound; the code path is
   `Hosted::look`, and the group's own stop on a failed log is `common_system`'s, tested over the simulated storage.
3. **10,000 rounds against the ten seconds a target may take.** role-binaries, "Storage is not ready before recovery",
   names 10,000 rounds to replay; at one round per commit that takes the target past ten seconds. Bound with
   `ROUNDS` = 200: the law, `/ready` only after the replay, is the same.
4. **The books' recovery at "twelve closes" over a process.** system-group, "The books recover in the storage process":
   the sequencer closes on its timer, so a process cannot be stopped at exactly twelve closes; group 3 bound the scenario
   over the simulated storage. Not bound here.
5. **A property of the system group fails on random draws.** Under Cargo, which runs Hegel without `HEGEL_DERANDOMIZE`,
   `common_system`'s `test_the_registry_agrees_with_its_model` found "Session 2 of 1: left Some(Busy), right
   Some(Registered)": the registry answered 14 where the model (`PROTOCOL.md` §11.5, §13.23) registers. Bazel's
   derandomized run does not draw it. Group 3's code or model; recorded for its owner, the shrunk case to become a unit
   test before a fix (`tdd-protocol.md`).

## Not done or not verified

- tasks.md 4.2 is not ticked: four scenarios are not bound — system-group's "The books recover in the storage process"
  (*Found wanting* 4), "A partition's sync holds up no close" (no means to delay one partition's sync in a process), "A
  partition recovers from the group of its own process" (it needs a principal and three Declares through the group; group
  8's failure runs take the hosting process down with versions in its log), and "A failed log of the group stops the
  process" (*Found wanting* 2); and role-binaries' "Every runtime of a pinned role holds a CPU of its own" for storage
  (Linux's `/proc` read-back; this machine is macOS).
- tasks.md 4.5 is not ticked: `common_storage`'s run tested no mutant, the binary's mutants are not killed by any suite
  Cargo runs, and `common_system`'s baseline failed (above).
- C4.4, C4.5 and C4.8 have no test; C4.7's deadline less one tick and C4.9's order of the group's stop are not observed.
- `storage_tests:bdd` was not run: it starts the frontend binary, which group 5 was changing during this run.
- `cargo fmt -p ultragraphdb_common_system` ran once early in this group, over the crate the concurrent performance change
  edits; any reformatting of its files is rustfmt's alone.

## Notes for the commit

Two commits, the lift first (task 4.1's files above, each lift line its own hunk), then the rest. `Cargo.lock` takes
`hegeltest` in `cluster_partition`'s dependencies and `ultragraphdb_common_system` in `ultragraphdb_storage`'s, no
version; the lock's other new lines in this working tree (`futures` in `ultragraphdb_frontend`, `hegeltest` in
`ultragraphdb_common_frontend`) are group 5's.
