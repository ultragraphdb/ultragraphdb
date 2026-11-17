# Stage 5.8, group 6: compute dials the group, `ultragraphdb_common_compute`, `ultragraphdb_compute`

Base commit `443a7f54`, the tree clean at the start. Tasks 6.1 to 6.4. Groups 4 and 5 and a performance change in
`ultragraphdb_common_system`, `ultragraphdb_core_catalog` and `ultragraphdb_core_mapping` ran beside this group in the same
tree; none of their files is part of this group's change.

Counts are those Bazel's test logs report: a unit target's `test result`, a BDD target's scenarios and steps.

## Task 6.1: the audit against `openspec/protocols/ULTRA_STYLE.md`

Scope: all of `src/` of `ultragraphdb_common_compute` at the base, 54 files, 5,097 lines, 204 functions. Every `file:line`
below is as at `443a7f54`.

How it was measured, as groups 1 to 3 measured: Clippy under Bazel through the aspect, which takes the workspace's lint
table, with the lints Ultra Style's table states that the workspace table does not hold added on the command line
(`-Dclippy::indexing_slicing`, `cast_possible_truncation`, `must_use_candidate`, `missing_errors_doc`,
`missing_panics_doc`). The library is clean. The unit test target reports three `clippy::indexing_slicing` in
`tests/types/partition_rows/partition_rows_frames_tests.rs:160-162`: the aspect's limit groups 1 to 3 recorded, since it
reads rules_rust's `.clippy.toml` and not the repository's, which allows indexing in tests. Rule 4 was measured by a script
counting each function's body lines, comments and blank lines left out.

| Rule | Result | Evidence |
|---|---|---|
| 1, acyclic control flow | conforms | no recursion and no stored callback; static dispatch throughout |
| 2, bounds | **gap, closed** | five loops did not say what ends them: `ReadPins::keep` (`types/read_pins.rs:99`), `ComputeSession::next_push` (`types/compute_session.rs:99`), `read_frames` (`types/link.rs:222`), `ComputeCore::link` (`types/compute_core/mod.rs:190`), `PartitionRows::fill` (`types/partition_rows/partition_rows_fill.rs:29`). Each now says so. The loops of `frontier.rs`, `compute_core_lookups.rs`, `partition_rows/mod.rs`, `partition_rows_terms.rs`, `partition_rows_fill.rs:125` and `run_read.rs:478` said so already; `prefix_end` and the `pop_back` drain iterate a collection |
| 2, queues | conforms | the unbounded channels state their bounds: `ComputeSession::new` (`types/compute_session.rs:38-40`), `Link`'s `out` (`types/link.rs:38-40`) and `OpenStream`'s frames, by its credit (`types/link.rs:55-57`) |
| 3, planes | **gap, closed** | no module stated a plane. `src/lib.rs` now states that a read's Scans, the Entries frames its links hand on and the Result batches its session pushes are the data plane, and that dialling a partition and reading where it listens and the placement map are control plane |
| 4, 70 lines | conforms | longest bodies 65 (`utils/run_read.rs:271`, `reverse_parts`), 59 (`types/access.rs:50`, `range`), 56 (`utils/run_read.rs:452`, `merge`); 0 of 204 above 70 |
| 5, invariants | conforms | no run-time `assert!` under `src/`; one `debug_assert_eq!` (`types/partition_rows/partition_rows_plan.rs:165`) |
| 6, smallest scope | conforms | `clippy::shadow_unrelated` clean |
| 7, results and panics | conforms | lint-clean; each `.ok()` folds a refusal into an `Option` its caller reads as absent (`types/row_terms.rs:31`, `:114`, `:120`, `:188`, `:206`; `compute_core_lookups.rs:300`; `partition_rows_frames.rs:145`; `partition_rows_fill.rs:138`) |
| 8, macros, `cfg` | conforms | no `macro_rules!`, no `cfg` |
| 9, ownership | **gap, closed** | `ComputeCore`'s four `RefCell`s (`types/compute_core/mod.rs:40-43`), `Link`'s cells (`types/link.rs:43-49`), `ReadShared::remaining` (`types/read_shared.rs:16`), `ReadPins::keeping` (`types/read_pins.rs:30`) and `ComputeSession`'s `Rc<ComputeCore>` (`types/compute_session.rs:27`) did not say why; each now does. `ReadPins::taken` said so already |
| 10, zero warnings | conforms | aspect clean over the library; the `#[expect(clippy::disallowed_methods)]` of `utils/clock.rs:9` carries its reason and is fulfilled under the repository's `clippy.toml` |
| Types: casts | conforms | every `as` states why it is lossless (`types/page.rs:68`, `types/result_sink.rs:40`, `:68`, `types/compute_core/mod.rs:176`, `types/partition_rows/partition_rows_terms.rs:117`) |
| Types: exhaustive matches, `#[must_use]`, `# Errors` | conforms | `clippy::wildcard_enum_match_arm`, `must_use_candidate`, `missing_errors_doc` clean |

### The lift and its evidence

Small and local: 21 lines of comments added and 2 changed, in 7 files, no code. So the step made the lift and went on.

Files, under `ultragraphdb_common/ultragraphdb_common_compute/src/`: `lib.rs` (the planes paragraph), `types/compute_core/mod.rs`
(the struct's rule-9 paragraph and `link`'s loop), `types/compute_session.rs`, `types/link.rs`,
`types/partition_rows/partition_rows_fill.rs`, `types/read_pins.rs`, `types/read_shared.rs`. In `lib.rs` and
`types/compute_core/mod.rs` the lift's lines are hunks of their own beside task 6.3's.

| Target | Before, at `443a7f54` | After the lift |
|---|---|---|
| `common_compute:ultragraphdb_common_compute_tests` | 155 passed, 3.2 s uncached | 155 passed |
| Clippy aspect, the workspace table and the five lints, the library | clean | clean |

## Tasks 6.2 and 6.3: compute dials the system group

### What was built

`ultragraphdb_common_compute`:

- `group_address(start)` (`utils/joining.rs`): the address compute dials for the system group, `cluster.system_address` where
  the settings name it, else `cluster.frontend_address` (below, *Settings until the switch*).
- `join_group(group, join, cluster, stopping)`: the join moved out of the binary's `service.rs` (`attempt`), unchanged in what it
  sends: Join, then Members until a storage member is listed, then a dial of the first partition, retried by
  `common_role::retry_until` until the cluster's deadline; past it, an `unavailable` error naming the group's address.
- `hold_peers(group, join, partitions, cluster, stopping, clock)`: the holding moved out of `service.rs` (`hold_peers`, `hold`,
  `answers`), with three changes. The time is read from `clock`, the time since an origin of the caller's, so a test decides
  by its clock whether a peer is gone past the deadline. The group's ask is a dial and a Members request, and a group whose
  Members no longer lists compute's address, as a group restarted holds no Join from before (the system group's
  `Membership` lives in memory in this stage), is sent compute's Join again (corner case C6.3). A partition lost while the
  group's ask is failing too waits on the group's verdict, so when the process hosting both is killed compute names the
  group's address, as the scenario has it. A peer lost names itself: "compute lost the system group at …" or, as before,
  "compute lost the role at …".
- `ComputeCore::catalog(tenant, version)` (`types/compute_core/compute_core_catalog.rs`): a catalog version read at the group
  through the wire's `CatalogFrames` (design D13): the request on a stream of its own, each frame checked by `CatalogFrames`,
  a frame's credit granted as the stream's runs out while more are due, a frame past the stream's credit refused as
  `PastCredit`, a frame on another stream refused, the group's Error returned as it came (code, message, detail), the whole
  bounded by the core's answer wait (code 17), and at most `CATALOG_FRAMES_MAX` frames (65; *Found wanting* 2).
- `ComputeCore` reads Members and Placement at the group's address it is given: its field and `new`'s parameter are named
  `group`, and its messages name the system group where they named the frontend.
- `Cargo.toml`: `futures` with its `std` feature. The workspace takes `futures` without default features since `86c16d7b`;
  under Cargo nothing else in this crate's graph turns `std` on, so `futures::channel` was missing and `cargo test -p
  ultragraphdb_common_compute`, which `scripts/mutants.sh` runs, did not build at the base. Bazel unifies the features over
  the workspace and was unaffected; neither lock file changed.

`ultragraphdb_compute`: `service.rs` reads `group_address`, joins through `join_group` and holds through `hold_peers` on the
process's clock; `attempt`, `hold`, `answers`, its own `hold_peers`, `POLL` and `ASKS_PER_DEADLINE` moved out with them, and
`join_as` takes no role kind, which only named the attempts.

`ultragraphdb_compute_tests`: `features/system-group/system_group.feature`, compute's part of "A group gone past the deadline
ends its dependants", bound in `tests/system_steps.rs` over a storage process hosting the group by its `[system]` table
(`system.role_address` 127.0.0.1:17084, `system.data_dir`, `[sequencer]`, `cluster.partition_count` 1) and compute dialling
`cluster.system_address`, neither naming `cluster.frontend_address`. `features/role_binaries/role_binaries.feature`'s header
names this change's spec beside stage 5.5's.

### Settings until the switch

Group 7 switches the settings in one commit (design D2). Until then compute takes its group's address by `group_address`:

| Settings name | Compute dials |
|---|---|
| `cluster.system_address` alone | it |
| both | `cluster.system_address`; `cluster.frontend_address` is not read |
| `cluster.frontend_address` alone, as every profile, harness and chart does today | it, sending what it sent there before: Join, Members, Placement, and now Catalog when asked. At the base the frontend's role address is where the system group listens (`common_system` runs in the frontend process), so the same commands reach the same group |
| neither | refused at start, exit 78, naming `cluster.system_address` |
| a `cluster.system_address` that is no address | refused at start, exit 78, naming `cluster.system_address`, whatever `cluster.frontend_address` holds |

What changes on the old path too: compute's periodic ask of the group is now a dial and a Members request in place of a bare dial,
and a group whose Members no longer list compute is sent its Join again; at the frontend, which lists compute once it joined, no
second Join is sent. Group 7 replaces `group_address`'s fallback by `common_role::system_address` when it removes
`cluster.frontend_address`.

### Phase 1 and 2: the surface and the failing run

The surface compiled with bare `unimplemented!()` bodies: `group_address`, `join_group`, `hold_peers` and
`ComputeCore::catalog`; `bazel build` of the crate passed. The suite written against it, observed failing:

| Target | Phase 2 |
|---|---|
| `common_compute:ultragraphdb_common_compute_tests` | 171 tests, 155 passed, 16 failed, each at an `unimplemented!()` of `utils/joining.rs` or `compute_core_catalog.rs`; those holding the peers through a task, 11.4 s in all as their waits ran out after the task's panic |

`compute_core_tests::test_the_placement_and_the_members_are_read_at_the_group` was added in phase 4, over code the group renamed
and did not change; it passed at once. `compute_core_catalog_tests::test_a_version_in_the_most_frames_is_read_and_one_more_refused`
was added by the mutation fix pass (below).

### Phase 3: the audit of the inputs

Read against corner cases C6.1 to C6.5 for coincidence: the tenant (4), the versions (1, 2, 3, 9) and their epochs (2, 5, 7, 9) are
distinct; a version past one frame is 2.5 frames at compute's `max_frame`, its last frame part full, its bytes a pattern of period
251 that no frame boundary lines up with; the deadline (200 ms) is far from the clock's tick (1 µs), and the clock moves one tick
before the deadline and onto it; the group, the partition and compute have distinct addresses. No case was found where two
quantities coincide.

### Phase 4: the implementation and the suite

| Target | Before (`443a7f54`) | After | Uncached, alone |
|---|---|---|---|
| `common_compute:ultragraphdb_common_compute_tests` | 155 passed | 173 passed | 3.7 s (3.2 s in three runs in a row at 172 tests, before the fix pass's test) |

`ultragraphdb_compute` has no test target of its own: its `service.rs` is covered by `ultragraphdb_compute_tests`
(*Service binaries* in `AGENTS.md`), which carries `integration_test`.

### Laws

None: group 6 holds no law of its own (`notes/laws.md`, task 0.6), and its crates gain no `property` target.

### Corner cases C6.1 to C6.5

Tests under `ultragraphdb_common/ultragraphdb_common_compute/tests/`:

| Case | Test |
|---|---|
| C6.1 | `utils/joining_tests::test_compute_dials_until_the_group_answers_and_a_partition_joined`; over the binaries, "Compute is ready once a partition has joined" |
| C6.2 | `utils/joining_tests::test_a_group_back_before_the_deadline_is_joined_again` (the group's connections turned away, then dialled again) |
| C6.3 | the same test, the group back one tick before the deadline and joined again; `test_a_group_gone_past_the_deadline_ends_compute_naming_its_address`, waiting one tick before the deadline and ending on it; `test_a_partition_lost_with_the_group_is_reported_as_the_group`; over the binaries, "A group gone past the deadline ends its dependants" |
| C6.4 | `utils/joining_tests::test_the_group_is_the_system_address_where_it_is_named`, `test_the_frontend_address_alone_is_dialled_as_before`, `test_no_group_address_is_refused_naming_the_system_address`; `types/compute_core/compute_core_tests::test_the_placement_and_the_members_are_read_at_the_group`; `types/compute_core/compute_core_catalog_tests` (six tests: the most frames a version may take and one more, a version past one frame, one of one frame and an empty one, `version` 0, a version never held refused with code 7 and its `detail`, a group past the answer bound or turning the connection away refused with code 17) |
| C6.5 | `utils/joining_tests::test_a_group_refusing_the_secret_is_asked_until_the_deadline` (joining: asked more than once, then code 69 naming the address and `AuthenticationFailed`, code 19); `test_a_group_refusing_the_secret_ends_compute_at_the_deadline` (holding) |

Beside them: `test_a_stop_ends_a_join_with_nothing`, `test_a_partition_gone_past_the_deadline_ends_compute_naming_it`.

Not tested: `read_catalog`'s refusal of a frame past the stream's credit and of a frame on another stream, which no peer serving
through `common_role::serve_connection` can send, since its connection holds a stream's frames for its credit; as `Link`'s
`PastCredit` check has none either.

### Scenarios bound

| Scenario | Bound in | Runs |
|---|---|---|
| system-group, "A group gone past the deadline ends its dependants", compute's part | `ultragraphdb_compute_tests`, `features/system-group/system_group.feature` | over the built binaries, `integration_test`: first at group 7's gate, since it needs group 4's hosting storage process |
| role-binaries, "A lost dependency exits with its code" | `features/role_binaries/role_binaries.feature`, already, the text unchanged by this change | as before, on `cluster.frontend_address` until group 7 |
| role-binaries, "Readiness, health and shutdown", compute's part | no scenario of the change's spec names compute; its rule, ready once the group and a partition answer, is "Compute is ready once a partition has joined" and C6.1's test | |

The frontend's part of "A group gone past the deadline ends its dependants" is group 5's.

### Phase 5: fuzzing

None: compute decodes no bytes of its own. Every frame it reads is read by a view of `ultragraphdb_protocol_wire`
(`MembersView`, `PlacementView`, `CatalogView` through `CatalogFrames`, `ErrorReplyView`), whose fuzz targets cover them.

### Phase 5: mutation testing

`scripts/mutants.sh <crate> --diff 443a7f54`, each run to its end, one at a time under the script's lock.

| Crate | Run | Outcome |
|---|---|---|
| `ultragraphdb_common_compute` | 1 | no mutant tested: the unmutated baseline did not build under Cargo, `futures::channel` missing (the manifest fix above) |
| | 2, over the diff | 43 mutants: 25 caught, 11 unviable, 1 timeout (`link.rs`: `read_frames` replaced by `()`, which reads nothing, a hang), 6 missed |
| | 3, `--re` over the five the fix pass addressed | 6 mutants (the regex took the unviable `/` with `%` of the same line too): 5 caught, 1 unviable |
| `ultragraphdb_compute` | 1 | 3 mutants, 3 missed: the binary has no Cargo test. Each is process-covered by `ultragraphdb_compute_tests`, as stage 5.5 classed the binary's mutants: `compute_role` replaced by `Ok(())` by every scenario starting compute, "A compute process without its address stops the start" among them; `join_as` by "Compute is ready once a partition has joined" and "A lost dependency exits with its code"; `accept_on_core` replaced by `()` by "Compute serves reads alone" and every read |
| `ultragraphdb_compute_tests` | none | no `src/`: the script reports nothing to mutate |

The fix pass, run 2's six survivors:

- `ask_group`'s two `==` with `!=` (the member's kind, its address against compute's): killed. The test of a group listing compute
  asserted no Join after the first Members, before a Join that ask sends could arrive; it now asserts after the second ask
  begins, which is after the first has ended.
- `CATALOG_FRAMES_MAX`'s `+` with `-` and `*`, `/` with `*`: killed by
  `test_a_version_in_the_most_frames_is_read_and_one_more_refused`, a version answered in 65 frames, one byte each, read, and one
  in 66 refused. The played group's `answer_in` splits a version so.
- **Open**: `hold`'s `deadline / ASKS_PER_DEADLINE` with `*`, asking every two deadlines in place of every half deadline. The
  pace is real time, and the tests decide by the clock they move whether a peer is gone, so no test observes how often compute
  asks; the mutant changes when compute notices a loss (up to three deadlines after it in place of one and a half), not
  whether. Left for the stage's final verification; `.cargo/mutants.toml` gained no entry, since the mutant is not
  equivalent.

### The gate

Design D12's gate for groups 0 to 6, run on 2026-11-16 with groups 4 and 5 still at work in the same tree:

- `bazel build //...` (`make build`): failed only in other groups' crates, at each run a different one as they worked:
  `ultragraphdb_storage` (group 4), then `ultragraphdb_common_frontend`'s test target (group 5). Every target of this group's
  crates built, and the Clippy aspect, with the five lints added, is clean over `common_compute`'s library and unit target,
  the compute binary and `compute_tests:bdd`.
- `bazel test //... --test_tag_filters=-integration_test --keep_going`: 147 of 154 pass;
  `common_frontend:ultragraphdb_common_frontend_tests` failed to build, and `client_compio`, `client_tokio`, `cluster_partition`
  (`bdd`, `property`, the unit target) and `common_frontend:property` failed: groups 4's and 5's crates, not touched here.
  `common_compute:ultragraphdb_common_compute_tests` passes.

The whole gate is the wave's, once groups 4 and 5 return.

### `ultragraphdb_compute_tests`, run once

`bazel test //ultragraphdb/ultragraphdb_compute_tests:bdd`, its first attempt stopped by `common_frontend` not building, then
run once: 8 features, 89 scenarios, 178 steps, all passed, in 27.3 s. "A group gone past the deadline ends its dependants"
passed against group 4's storage binary as it stood in the tree then, hosting the group by its `[system]` table; the 88 others
ran on `cluster.frontend_address` as before. The target held 88 scenarios in 24 s before this change: it is past the 10 s bound
already, by the clusters of processes its read scenarios share, and this group's scenario adds about 3 s.

### Found wanting

1. **What compute does with a catalog version in this stage.** `PROTOCOL.md` §11.12's endpoint table has compute request
   Catalog, and design D13 has compute read a version through `CatalogFrames`; no text says what compute reads one for. The
   rows of §11.12's routing table that name the catalog (an IRI a keyed type mints, an edge-encoded instance's handle IRI) are
   not implemented in compute's routing. Implemented: `ComputeCore::catalog`, which nothing in compute's read path calls yet.
2. **How many Catalog frames a version may take.** §13.7 bounds a version's bytes (`catalog_bytes_max`) and has the frames
   continue while the bytes "do not fit one frame within the requester's `max_frame`", but bounds no count: a group sending frames
   of no bytes, none flagged `LAST`, never ends. Implemented: at most `CATALOG_FRAMES_MAX`, 16,777,216 bytes over half the
   `max_frame` floor (262,144) plus one, 65 frames, read as each frame but the last filled within the requester's `max_frame`, as
   the wire's `Catalog::frames` fills them; past it Error 1, `FieldValue` `"catalog"`.
3. **When a lost group's deadline starts.** system-group: compute "SHALL exit with code 69 once the group has been gone for
   `join_deadline_ms`". Compute asks every half deadline and counts the deadline from the start of the ask that finds the peer
   gone, as it did for every peer before this change; it exits between one and one and a half deadlines after the loss.
4. **Which address names a loss of both.** The scenario has compute name the group's address when the hosting process is killed,
   which takes its partitions with it; no text says which address compute names when the group and a partition are both gone.
   Implemented: the group's, a partition's loss waiting on the group's verdict while the group's ask is failing.
5. **"Dial again when its connection closes" for compute.** Compute holds no connection to the group between its asks: each ask
   dials. Read as: compute dials the group at every ask, and Joins again when the group no longer lists it, since in this stage
   a Join is held in memory alone (`common_system`'s `Membership`) and a restarted group has forgotten it; §13.1's stage 5.9 text
   has a Join answered once durable.
6. **The join's refusal names the frontend.** `common_role::retry_until`, shared with storage, says "the frontend did not take
   this compute within …". `join_group` names the group's address before it; the inner text is `common_role`'s, which this group
   left alone since storage needs the same change (group 4's or group 7's).

### Not done or not verified

- The repository's gate did not pass as a whole: groups 4's and 5's crates failed while they worked (above). This group's
  targets pass.
- The scenario over the binaries passed against group 4's work in progress; it is verified for the stage at group 7's gate.
- One mutant open: `hold`'s pace (above).
- `compute_tests:bdd` runs 27.3 s, past the 10 s bound, as it ran 24 s before this change; splitting it is not this group's.
- `read_catalog`'s refusals of a frame past credit and of a frame on another stream have no test (above).
- `ComputeCore::catalog` has no caller in compute's read path (*Found wanting* 1).

### Commit messages

Two commits: the lift alone first, then the rest. The lift's lines in `src/lib.rs` (the planes paragraph) and
`src/types/compute_core/mod.rs` (the struct's rule-9 paragraph and `link`'s loop comment) are hunks of their own beside task
6.3's, so `git add -p` takes them apart.

```
common_compute: Ultra Style lift (stage 5.8, task 6.1)

The audit of ultragraphdb_common_compute against ULTRA_STYLE.md found three gaps, closed by comments alone: five loops say
what ends them (rule 2), lib.rs states the crate's planes (rule 3), and the cells and Rc shared by ComputeCore, Link,
ReadShared, ReadPins and ComputeSession say why (rule 9). No code changed; the unit target passes 155 before and after.
The audit is in notes/group-6.md.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
```

```
common_compute, compute: compute dials the system group (stage 5.8, tasks 6.2 to 6.4)

Compute reads its group's address with group_address: cluster.system_address where the settings name it, else
cluster.frontend_address, which every profile names until group 7's switch (design D2). Joining and holding the peers move
from the binary's service.rs into common_compute as join_group and hold_peers, which read the time from a clock they are
given, so tests decide by their clock whether a peer is gone past the deadline. The group is asked its Members at each ask,
and sent compute's Join again once it no longer lists compute, as a restarted group does; a partition lost while the group
is failing too is reported as the group lost, naming its address. ComputeCore reads Members and Placement at the group and
gains catalog(), a version read across Catalog frames through CatalogFrames under the stream's credit (design D13).

common_compute's manifest takes futures' std feature: since 86c16d7b the workspace takes futures without default
features, and under Cargo nothing else in this crate's graph turned std on, so scripts/mutants.sh could not build it.

compute_tests binds compute's part of "A group gone past the deadline ends its dependants" over a storage process hosting
the group. Corner cases C6.1 to C6.5 are unit tests; group 6 holds no law. common_compute's unit target: 173 pass, 3.7 s.
Mutation: common_compute 25 caught, 5 more killed by the fix pass, 1 open (the pace of the asks); the binary's 3 are
process-covered. Notes in notes/group-6.md.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
```
