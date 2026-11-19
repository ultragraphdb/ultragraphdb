# Group 5: the endpoint executes reads

## Task 5.1: the lifts

`group-5-lift-compute.md` and `group-5-lift-frontend.md`. The two lifts were gated apart with unchanged counts and
landed together in commit `429acbfe`. The property test's race the frontend's lift found is fixed in `e714ddaa`.

## Task 5.2: the merge (2026-11-18, base `e714ddaa`)

`ultragraphdb_common_frontend`'s code is in `ultragraphdb_common_compute` (design D1).

- **Moved by `git mv`:** 123 files, the crate's sources, tests, property tests, features and test doubles, each to the
  same path under `ultragraphdb_common_compute`.
- **Merged by hand:** `src/lib.rs`, `src/types/mod.rs`, `src/utils/mod.rs`, `tests/mod.rs`, `tests/types/mod.rs` and
  `tests/utils/mod.rs`, the only paths both crates held. No module name and no exported name was held by both.
- **Manifest and build file:** the dependencies joined, without `ultragraphdb_traits_catalog`, which nothing used; the
  `test-utils` feature, the `property` target and the eleven BDD targets brought over under the crate's name.
  `Cargo.lock` changed its dependency lists alone.
- **Dependants:** `ultragraphdb_frontend` and `ultragraphdb_frontend_tests` name `ultragraphdb_common_compute`.
- **`ultragraphdb_common_frontend`** is an empty crate until task 6.5: a manifest, a build file and a `lib.rs` holding
  its documentation line. Its five module files that the merge left behind were removed with the user's approval.
- **`AGENTS.md`:** both crates' rows and the dependency rows that named the frontend's crate.

| Target | `common_compute` before | `common_frontend` before | Merged |
| --- | --- | --- | --- |
| Unit tests | 179 | 114 | 293 |
| Property tests | none | 12 | 12 |
| BDD, eleven targets | none | 146 scenarios, 292 steps | 146 scenarios, 292 steps |

The gate: `cargo fmt --check` clean, `make build`, the Clippy aspect over `//...` and `make test` all pass; the
integration phase ran 21 of its 22 targets fresh.

Mutation runs from here on set `MUTANTS_UNCHANGED_BY` to this task's commit.

## Tasks 5.3 to 5.7: the endpoint executes reads (2026-11-18, base `18e41fa9`)

The design is `notes/group-5-design.md`; the decisions on its ten questions, the user's and the coordinator's, are taken
as written there: rows partition by partition, the resume key of §11.8's 32-byte layout, the removals of task 5.7 in the
same commit as phase 4, the step bound stated over the scheduler, `with_compute` a builder, two scenarios moved to tasks 6.2
and 3.2, code 6 at 1,048,576 for an Execute's Read, the page fix tested through `test-utils`, and the wire's changes in this
group.

### The specifications first

- quad-reads: a MODIFIED delta of "Compute executes quad-pattern reads over storage", the archived text with the order alone
  changed to §9.1's, and its scenarios "A bound literal object fans out" and "An unbound subject answers partition by
  partition" (renamed from "… merges every partition") worded as stage 6.5's delta words them; the value-index lookup keeps
  its key order, which §9.1 does not govern. "A read's batches fit the client's frame" holds an Execute's bound: read as a
  Read, refused with code 6 and `detail` 1,048,576 past a frame, with the scenario "An Execute whose bound pattern passes a
  frame is refused" (`notes/group-0-deltas.md` says so beside the scenario it replaces).
- `PROTOCOL.md` §4.1: a resume key's fixed part is 32 bytes, as §11.8 states.
- `tasks.md`: "Compute listens at no role address" moved to task 6.2, "Members names no compute node" to task 3.2.
- `notes/laws.md` states how L5.7 and L5.8 read "in the turn", and L5.9 the scheduler's; `notes/corner-cases.md` C5.32 the
  same.
- `openspec validate stage-5-9-compute-role --strict` passes.

### Phase 1: the surface

`ReadExecutor` (`new`, `with_open`, `start`, `ready`, `cancel`, `next`); `ReadPattern::of_read`; `ReadRequest` public with
`new` and eight setters; `ComputeCore::steps`; `EndpointCore::with_compute`; `ReadView::pattern_hash`; `ResumeKey::new`
taking the partition, `ResumeKey::with_ordering`, `ResumeKeyView::partition` and `ordering`; behind `test-utils`,
`page_held_bytes`. `make build` passed and the crate's counts stood at 293, 12 and 146.

### Phase 2: the suite and its failing run

Written against the unimplemented surface, the run of 2026-11-18:

| Target | Tests | Failed | Why |
|---|---|---|---|
| `common_compute` unit | 308 | 16 | `not implemented`: `ReadExecutor::new` (10), `ReadPattern::of_read` (3), `page_held_bytes` (2), `ResumeKeyView::partition` (1) |
| `common_compute` property | 18 | 6 | `not implemented` (5); the step law's assertion, its reads' tasks having panicked on `ReadExecutor::new` (1) |
| `common_compute` `bdd_executor` | 11 scenarios | 11 | each `When` panicked, `not implemented` |
| `protocol_wire` unit | 993 | 9 | `not implemented`: `ReadView::pattern_hash` (2), `ResumeKeyView::partition`, `ResumeKey::with_ordering`; the 24-byte layout `encode` still wrote (5) |

What the suite holds, by law and case:

- Unit, `tests/types/read_executor_tests.rs`: C5.1, C5.2, C5.3 (partition 2's keys below partition 1's, so key order would
  differ), C5.30 and C5.31 (no batch before its room), C5.38 (a cancelled read hands out nothing and lets its Scans go), a
  refusal as the stream's last frame, a read started again on its stream, `with_open`'s count, C5.45 and L5.10 (a probe task
  sees at most one step between its runs).
- Unit, `tests/types/read_pattern_tests.rs`: §4.4's slot refusals with their index; `tests/utils_test/page_bytes_tests.rs`:
  a vector of 65,535 four-byte elements under 64 statement IDs held once (262,140 bytes), and two records each once.
- Wire: `ReadView::pattern_hash` against bytes laid out by hand from §4.4, §4.5 and §11.8, and equal to
  `ComputeReadView::pattern_hash`; the resume key's partition at 24, padding, partition 0, ordering value, and C5.26's order.
- Property, `tests/property/types/read_executor_property_tests.rs`: L5.1 and L5.2 together, L5.4 with L5.6 (each page
  through another executor), L5.5, L5.7, L5.9 with L5.10, L5.14.
- Scenarios, `features/quad-reads/executor.feature` (`bdd_executor`), over the executor and played partitions, the
  scenario playing the client's connection as its outbound queue gives room: "A commit is relayed between a read's frames",
  "A read's step stays within one frame", "A fan-out stays on its read's core", "An unbound subject answers partition by
  partition", the four of "A read's batches fit the client's frame" and its Cancel, and both of "Compute times a partition
  that stops answering". "A row too large for the client's frame is refused" stands the literal of 600,000 bytes in by an
  IRI of that length, which the played partition holds as a term's text.

The played cluster under `tests/apart/` is shared by the unit, property and BDD targets, included by path as `pub mod apart`
(AGENTS.md: a harness included by path); the played partition gained `frames_for`, a Scan answered from its table for a
role double, and `Script::Paced`, and `apart_reads` gained `hold_subject`, a subject held in any tenant.

Laws not stated as their own property test in this group: L5.3 (held by C5.7 and C5.8's scenario and the result sink's
tests), L5.8 (held by the cancel scenario and unit test), L5.11 to L5.13, which this group does not change: the order of a
transaction's reads and writes and the routing stay `ClientSession`'s and `ReadRoute`'s, held by `order.feature`,
`routing.feature` and `run_read_tests`. They are recorded open for task 9.1, the change's one mutation run.

### Phase 3: the audit

The inputs were read against the corner-case enumeration for coincidence: the partition-order tests draw or fix each
partition's IDs so that key order and partition order differ; the resume pages are read through executors of their own
cores, so no state of one executor coincides with the key; the step laws use read-write reads, which hold no pin, and the
counter leaves a pin's frames out, so the probe sees a read's steps alone.

### Phase 4: what was built

- The executor (`ReadExecutor`): numbers each read it starts, stamps each frame with the session's stream, drops what a read
  no longer held sends late. `ComputeSession` is an adapter over it.
- The step: `ScanStream::next` ends a step after each frame it consumes (`Link::step`, counted unless the stream is a pin's),
  `ResultSink::send_batch` after each batch but the last, at its room's wait; `merge`'s `try_join_all` is gone.
- Rows partition by partition: every partition's first Scan of rows asked at once (`PartitionRows::open_first`), the
  partitions drained in ascending order; a resume passes the partitions before the key's, reads the key's after its row and
  the later ones from their start. `ResumeKey` writes §11.8's 32-byte layout with the partition and the ordering value.
- `Page::add_object` holds an encoding once; `add_row` names the last object.
- The session: `ReadPattern` taken as a read arrives, `ReadRequest` built at its start from the transaction's state, the
  executor's frames taken beside the session's events, `ready` passed on, Cancel, Commit, Abort, code 23 and the connection's
  end stopping the read; an Execute read as the Read of its statement's pattern.
- The frontend binary: each endpoint core's `ComputeCore`, dialling as `frontend`, the times from `[storage]`.

The coordinator's ruling that only the two key-order Then-steps change in `ultragraphdb_compute_tests` was met in four
places, all the same change: `read_steps.rs`' three expectations built by `in_key_order` (the bound literal object, the
unbound subject and the vector object) and `access_steps.rs`' reverse read now expect `<a>`, partition 1's, before `<c>`;
the two scenarios' Then texts follow the delta.

One test changed its statement with the key's layout: `result_sink_tests`' exact fit of a resume key in its last batch. A key
now ends with its ordering value at a multiple of 8, so its length steps by 8 and a frame of exactly `max_frame` bytes is
reached only where the rest of the frame leaves the right remainder; the test now holds the key within those 8 bytes.

### Task 5.7: removed

Scenarios, each checked for a read law no other scenario holds:

| Scenario | Where | The law, held now by |
|---|---|---|
| Reads alternate between two compute processes | `common_compute` `compute.feature`; `ultragraphdb_tests` `processes.feature` | none: no read is spread |
| A closed compute process is skipped | both | none: no compute process is tried |
| No compute process can be reached | both | a read's partition unreachable answers 17: `errors.feature` |
| A read is never resent after a frame | both | "A silent partition ends its reads with code 17" (no Scan sent again) |
| A Result frame past its credit closes the frontend's role connection | `compute.feature` | an Entries frame past credit: `compute_tests` `past_credit.feature` until task 6.4, then task 4.2's tap |
| A hung compute process answers code 17 | both | "A silent partition ends its reads with code 17" |
| A compute process waiting within the deadline is not hung | both | "A partition answering within the bound is not hung" |
| A frame restarts a hung compute's timer | `compute.feature` | the same |
| A stream without credit holds off a hung compute's timer | both | `read_executor_tests`' room test; `compute_tests` "A client holding no credit holds a frame's worth of names" |
| A compute process that joins later is found before a read is refused | `system_link.feature` | a partition's address read again: `frontend_tests` "A storage process restarted at its address is dialled again" |

`order.feature`'s "An Execute too large for compute's frame is refused" was reworded: "An Execute whose bound
pattern passes a frame is refused". The `bdd_compute` target went with `compute.feature`.

Code: `ClientRead`'s relay fields (`body`, `pattern`, `route`, `attempt`, `tried`, `frames`, `last_frame`,
`members_read`); `ClientSession::compute_read`, the relay's `dispatch_read`, `members_again`, `members_read`, `read_event`,
`read_closed`, `read_cancelled_answering` and the hung-compute timer (`hung_at`); `SessionEvent::MembersRead`;
`RouteKey::Read`; `RoleLink::credit`; `RoleLinks::next_compute`, `compute_count` and the compute addresses `joined` and
`learned` kept; `Frontier` (`src/types/frontier.rs`) and its five tests (`tests/types/frontier_tests.rs`), the heap of the
key-order merge, whose order law the partition-order tests now hold.

Doubles and harness: `SystemPlay::compute` and its Members listing; `Played::compute`; `Stage::compute_with`, `compute`,
`computes_in_turn`, `answer_empty`, `empty_answer`; `answer_frame`, `answer_with`, `stalling`; `ultragraphdb_tests`'
second compute process (`Spec::second_compute`) and `two_computes`, `stalling`; `role_links_tests`' two compute tests
(one replaced by a test of the partitions listed).

Bound again over the in-process path: every endpoint scenario that played compute plays its partition instead
(`holding_reads`, `refusing_reads`, `rows_held` over a played partition's table, `release`, `refuse_scan`), and the steps
naming the relay take the deltas' wording.

### The gate of phase 4

Two bindings of `ultragraphdb_tests`' `process_faults.feature` named the compute process, and the first `make test` failed
both:

- "Code 17 runs a read again and leaves a commit in doubt" killed the compute process to have a read refused with code 17.
  The endpoint executes the read itself now, so the step kills `storage_2`, the partition the read reaches, before the read:
  the read finds no connection, and the commit after it no partition. The scenario's text is unchanged.
- "A long deadline survives the client's timeout" ran on a cluster setting the frontend's `storage.scan_stall_ms` to 100,
  which timed the relay's hung compute process. The frontend's reads take that setting now, and their wait for the
  placement map, twice it, ended at 200 ms while the step held the system group's storage process stopped. The setting and
  its constant `STALL_MS` were removed: the frontend reads by the default, as the compute process it replaced did.

A third failure was intermittent and in a crate this group did not change, rebuilt only because the wire crate changed:
`common_role`'s `test_a_refused_close_request_is_answered_by_nothing` failed 6 of 30 runs of its target and none of 20 alone.
With one dispatcher alive, tracing-core registers a callsite first reached on another thread by that thread's default, here
none, and caches `never`; `test_a_refused_ended_is_answered_by_nothing` reaches the same refusal's warning on its own thread
without a subscriber, so the warning this test captures went unseen. The test now holds a second dispatcher for its run, so
every registration asks both: 130 of 130 runs pass.

The gate on 2026-11-18: `make build` passes; the Clippy aspect over `//...` is clean; `make test` passes, every unit,
property, scenario and integration target green (20, 91, 10, 5, 52 and 22 targets in its six runs); `make property-random`
passes 20 of 20 runs on each of the ten property targets. A first `make property-random` failed three runs on Hegel's
`TooSlow` health check alone, each over 100 s of wall time spanning the machine's idle sleep at 13:48; the run again under
`caffeinate -i` is the one recorded.

## Task 5.8: the integration targets in parallel (2026-11-18)

### The cause

Every process the harness starts listened at a port fixed in `ultragraphdb_tests`' `one_host.rs` and `harness.rs` (17080 to
17083, 17090, 17091, 18081 to 18083), and each sister crate added its own (17084, 17085, 17095, 17200 to 17204, 17300,
17301, 17400, 18085, 18090, 18101, 18103, the config battery's 17090 and 17091). Two targets would have collided on them,
so every process-starting integration target was tagged `exclusive`, and Bazel ran the phase's targets one at a time.

### The port scheme

`ultragraphdb_tool_bdd::port(offset)` is the base in `ULTRAGRAPHDB_TEST_PORT_BASE` plus `offset`, and `loopback(offset)` its
`127.0.0.1` address; an unset variable fails the first call, naming it and `bazel/test_ports.bzl`, and an offset past the
block of `PORT_BLOCK` (200) ports is refused. No port is picked at runtime. The harness's addresses are functions over it,
`client_address()` and its eight siblings, each a `TestResult<String>`, and `frontend_on_one_host()`,
`compute_on_one_host()`, `storage_on_one_host()`, `role(..)` and `role_of_service(..)` return a `TestResult` with them.

The harness's offsets: 0 the client endpoint, 1 the system group, 2 compute's role address, 3 storage's (its partitions 3
to 6), 10 the config service, 11 the double, 12 to 14 the frontend's, compute's and storage's observability. Each crate's
own: `ultragraphdb_tests`' clusters shift by 20 ports a cluster (the shared one 20, the spread one 40, the specs 60, 80, ... to 180,
the eighth refused by name past the block), their second processes 5 above the first; `frontend_tests` 5 and 16 (the
second storage process); `compute_tests` 4 (its own system group), 5 and 16 (the lost storage process), and its four
shared clusters at 20, 54, 88 and 122, 34 ports each; `storage_tests` 15 (the played frontend), 20 to 24 and 30, 31 (the
two storage processes' partitions), 40 (`system_bdd`'s group), 41 and 43 (observability), 45 (admin); `supervisor_tests`
45 (admin), the shipped `dev` profile's 7080, 8081 to 8083 and 8090 moved to the harness's; `config_tests` 0, 1 and 2.

`bazel/test_ports.bzl`'s `port_env(index)` gives the calling package's target its block, the bases 200 apart from 12000;
a package's targets take their indices from one `enumerate`, and a package or index outside the table, two packages
sharing a block, or a block reaching 32768 (Linux's ephemeral range; macOS's starts at 49152) fails the load:

| Package                         | Blocks | Bases          | Targets |
|---------------------------------|--------|----------------|---------|
| `ultragraphdb_tests`            | 0–45   | 12000 to 21000 | `unit` 0, `bdd` 1, `BDD_FEATURES` from 2 (41 used) |
| `ultragraphdb_frontend_tests`   | 46–59  | 21200 to 23800 | 12, one per feature file |
| `ultragraphdb_compute_tests`    | 60–67  | 24000 to 25400 | 8, one per feature file |
| `ultragraphdb_storage_tests`    | 68–87  | 25600 to 29400 | 15 |
| `ultragraphdb_config_tests`     | 88     | 29600          | `bdd` |
| `ultragraphdb_supervisor_tests` | 89–90  | 29800, 30000   | `bdd` 0, `image_bdd` 1 |

### The targets

`exclusive` is dropped from every target but `ultragraphdb_supervisor_tests:image_bdd`, which loads the role images into the
one Docker daemon under fixed tags and runs containers on the host's cores; its container names are per process
(`ugimg-<pid>-<n>-<role>`) and its host ports the daemon's. Every other target starts processes alone, its data under its
own `TEST_TMPDIR` and its Unix sockets named by its pid.

The splits, by feature file: `ultragraphdb_tests`' `bdd_client_endpoint` (7 files), `bdd_quad_reads` (8),
`bdd_sessions` and `bdd_transactions` (7), `bdd_wire_protocol` (8) one target per file, listed in `BDD_ALONE` and named
for their path; `bdd`'s remainder gave `bdd_partitions`, `bdd_schema_changes` and `bdd_system_identity` to `BDD_GROUPS`.
`ultragraphdb_frontend_tests:bdd`, `ultragraphdb_compute_tests:bdd` and `ultragraphdb_storage_tests:bdd` became one target
per feature file, generated from their globs; the storage `@system` and `@drops` targets stay as they were.
78 integration targets in all, 22 before.

### Scenario counts, before and after

Every target's scenarios and steps, summed over its runs (carriers and shells): per package, before from the last logs on
disk, after from the run below.

| Package                         | Before        | After         |
|---------------------------------|---------------|---------------|
| `ultragraphdb_tests`            | 211 / 423 and `bdd_quad_reads` | 211 / 423 and `bdd_quad_reads_*` 111 / 222 |
| `ultragraphdb_frontend_tests`   | 30 / 60       | 30 / 60       |
| `ultragraphdb_compute_tests`    | 90 / 180      | 90 / 180      |
| `ultragraphdb_storage_tests`    | 121 / 242     | 121 / 242     |
| `ultragraphdb_config_tests`     | 44 / 88       | 44 / 88       |
| `ultragraphdb_supervisor_tests` | 35 / 70       | 35 / 70       |

`bdd_quad_reads` left no complete log on disk (its last run was cut off), so its 111 scenarios after have no figure
before; every other target's totals match.

### Times

No run was measured before the change (the user's call): before is the last logs on disk, each target alone, `exclusive`.
Their sum is 671 s without `bdd_quad_reads`: `bdd_sessions` 158, `storage_tests:bdd` 98, `bdd_wire_protocol` 83,
`bdd_client_endpoint` 64, `frontend_tests:bdd` 50, `compute_tests:bdd` 27, `supervisor_tests:bdd` 25, `bdd_failure_frontend`
23, `bdd_failure_host` 23, `bdd_failure_link` 22, `bdd_transactions` 20, `image_bdd` 18, `bdd` 14, `bdd_catalog` 10,
`bdd_kubernetes` 8, `system_bdd` 7, `system_partitions_bdd` 6, `bdd_failure_order` 5, `system_books_bdd` 4, `config_tests:bdd`
4, `drops_bdd` 2.

After, one run of `bazel test //... --test_tag_filters=integration_test --test_env=ENV=LOCAL` on the 16-core machine: 239 s
of wall time, the test binaries already built; 77 of 78 targets pass. Run side by side, every target takes longer than
alone (`bdd_catalog` 10 s alone, 32 s here). The slowest:

| Target | Scenarios | Time |
|---|---|---|
| `ultragraphdb_tests:bdd_transactions_processes` | 7 | 101 s |
| `storage_tests:bdd_quad_reads_read_kernel` | 47 | 77 s |
| `ultragraphdb_tests:bdd_transactions_lost_replies` | 7 | 74 s |
| `ultragraphdb_tests:bdd_client_endpoint_processes` | 7 | 59 s |
| `ultragraphdb_tests:bdd_wire_protocol_lost_replies` | 4 | 50 s |
| `ultragraphdb_tests:bdd_wire_protocol_process_faults` | 9 | 48 s |
| `supervisor_tests:bdd` | 28 | 47 s |
| `ultragraphdb_tests:bdd_wire_protocol_damaged_replies` | 2 | 45 s |
| `storage_tests:bdd_wire_protocol_parked_requests` | 10 | 39 s |

Each is one feature file: a split by file cannot shorten it, so the 30 s mark is passed by those above it.

### The one failure

`bdd_wire_protocol_process_faults` failed "Code 17 runs a read again and leaves a commit in doubt": the read was refused
`Refused { request: Other, code: Unavailable, .. "the role connection closed under the request, or could not be opened" }`,
where the scenario expects a refusal the client marks `RunsAgain::Read`. Run alone, with nothing beside it
(`--runs_per_test=3 --local_test_jobs=1`), it failed 1 of 3 the same way. It is the step task 5.6's gate rewrote to kill
`storage_2` before the read, and it is intermittent with or without the parallel phase: group 5's, left open here.

### Found, not changed

- `ultragraphdb_tests`' `transactions/processes.feature` (7 scenarios, 101 s) and `transactions/lost_replies.feature`
  (7, 74 s) are the phase's floor: 10 to 15 s a scenario, beyond a cluster's start of about 2 s.
- `compute_tests`' `main` starts all four shared clusters before any scenario, in each of its 8 targets, where a file may
  use none of them (`role_binaries`, `system_group`, `wire_protocol/past_credit`); starting them on first use would cut each
  target's start.
- `storage_tests:bdd_quad_reads_read_kernel` (47 scenarios) and `compute_tests:bdd_quad_reads_compute_reads` (59) are one
  file each.
- `config_tests`' `restore_down` took a port by binding 127.0.0.1:0 and letting it go, for a URL where nothing answers: that
  bind-then-release could race another process for the port; it names offset 2 of its block now, where nothing listens.
- Every target keeps `max_concurrent_scenarios(1)`: its scenarios share its block.
- The fixed sleeps in the scenarios' paths (`frontend_before_storage` 500 ms, `ENDED` 300 ms, `service_late` 600 ms, the
  double's 300 ms split) are what those scenarios test.

## Two intermittent failures after task 5.8 (2026-11-18)

A full `make test` failed once in each of `storage_tests:bdd_config_service_bootstrap` and
`ultragraphdb_tests:bdd_transactions_lost_replies`, both green on the next run; neither run's log survived the rerun.

### `bdd_config_service_bootstrap`: a race in the shared harness, fixed

`ultragraphdb_tests::key_file()` truncated and rewrote `read.key` on every call. In "A role reads its settings over TLS" the
config service starts without waiting, and the next `bootstrap()` rewrites the key at once, so under the storage group's
side-by-side load the service could read the file between the truncation and the write. It then exits with "holds 0
bytes", and the storage process's `/health` wait times out. Reproduced in 6 of 40 runs with the 14 other storage targets
looping beside it; every failing run's config log held "holds 0 bytes", and no passing run's did. `key_file()` and
`holding()` now write through `replace`, a staged file renamed into place, so a reader finds the old file or the new one
whole. Two harness unit tests pin this by inode, each red against the old code. The `WRITTEN_AGAIN` mutex that had fenced
the harness's own tests from the same race is removed. After the fix: 0 failures in 40 runs under the same load.

### `bdd_transactions_lost_replies`: not reproduced, open

117 valid runs, 0 failures: 112 direct runs of the binary from its runfiles, 4 to 16 at a time beside other cluster
targets and CPU load (load average 40 to 70), and 5 rounds of all 40 cluster targets, 16 side by side. Four further
failures came after the Mac's idle sleep at 15:38 and are discarded. Unconfirmed candidate: a resend's kind-1 Check
reaching the system group before the original's Ended, which travels on another endpoint core's connection; the group's
answer, code 14, is legal (`PROTOCOL.md` §11.5), and the scenario's one-attempt client reports it as a final `InDoubt`. Open
until a captured log of the next failure shows the cause.

### Found by the two investigations, open

- **"Code 17 runs a read again and leaves a commit in doubt"** (`wire-protocol/process_faults.feature`): 12 failures in 25
  runs. The read is refused with `ClientSession::unavailable()`'s message as request kind Other, which fits `bind_event`'s
  `LinkEvent::Closed` ending the read-write transaction bound to the killed `storage_2`: the reader's read takes the refusal
  of another transaction's stream. Group 5's code.
- **"A Register answered busy is sent again"** (`process_faults.feature`): 20 failures in 25 runs under load, through the
  step's wall-clock gaps (`own_steps.rs` `register_retried`: a 20 ms pause, `storage_2` continued after 120 ms).
- **`storage_tests:system_books_bdd`**, "A partition recovers from the group of its own process": 12 failures in 21 runs
  under load, "Transaction has been aborted", before and after the harness fix; twice "3 closes in 500 ms".
- Under load heavier than `make test` makes: catalog-check timeouts of 8,000 ms in `bdd_transactions_operations`,
  `bdd_transactions_requests` and `bdd_schema_changes`, and `bdd_quad_reads_levels`' "answers at epoch 12 without waiting".
- The config service a test starts outlives a test killed by its pid: `config_process` starts it with `clear_env` and no
  `ULTRAGRAPHDB_PARENT_PID`. Under Bazel the process group's kill covers it.
- RustRover holds 127.0.0.1:30001 and 30002 on this Mac, inside the supervisor's block 90 (30000–30199).
