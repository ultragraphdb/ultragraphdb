# Stage 5.8, group 8: the cluster end to end, `ultragraphdb_tests`

Base commit `c80dc59f` (`1be22e5b` and a website update). This file records wave A, part 1: `ultragraphdb_tests` made
runnable under `make test`, and task 8.1's part in `ultragraphdb_tests`. The other part of task 8.1, `frontend_tests`,
`storage_tests` and `compute_tests`, is another agent's, beside this one in the same tree; so is a text change to
`PROTOCOL.md`. None of their files is part of this record, and task 8.1 stays open until their part is done.

Counts are those Bazel's test logs report: per target, the scenarios each of the five runs (compio over TCP, over Unix
sockets, over TLS; tokio over TCP, over TLS) reports, summed. Times are Bazel's for a run with `--nocache_test_results`.

## The split

`ultragraphdb_tests:bdd` ran every feature through the five runs in one binary: 257 s at task 7.4's gate, 268 s and 274 s
here under load with its failures fixed, against Bazel's 300 s, and more with each failing step's wait, so `make test`
could not pass. Its scenarios are
now eight targets, as `ultragraphdb_common_frontend` splits its own: `BDD_FEATURES` in `BUILD.bazel` names each target's
feature files, a target's runfiles hold those files alone, and `bdd` takes every feature file no other target names, so each
scenario runs in exactly one target. Every target keeps `exclusive` and `integration_test`, which is all `scripts/test.sh`'s
integration phase selects by; it orders nothing.

| Target | Feature files |
|---|---|
| `bdd_catalog` | `metadata-group/catalog.feature` |
| `bdd_client_endpoint` | `client-endpoint/*` |
| `bdd_kubernetes` | `kubernetes-deployment/pods.feature` |
| `bdd_quad_reads` | `quad-reads/*` |
| `bdd_sessions` | `transactions/lost_replies.feature`, `transactions/processes.feature` |
| `bdd_transactions` | the other files of `transactions/` |
| `bdd_wire_protocol` | `wire-protocol/*` |
| `bdd` | the rest: `commit-rounds`, `partitions`, `role-binaries`, `role_binaries`, `schema-changes`, `system-identity`, `term-dictionary` |

Each target starts its own shared clusters, a few seconds each. The split ran the same 328 scenario executions as the
whole target (208, 10, 36, 38 and 36 over the five runs). A target below 10 s is not this task's (deferred by the user).

## The scenarios: task 8.1 in `ultragraphdb_tests`

### Before

The split targets at the base, each run once, uncached (the whole target at task 7.5's gate: 198/208, 10/10, 35/36,
36/38, 35/36, 314 of 328, then a timeout):

| Target | Passed | Time |
|---|---|---|
| `bdd` | 24 of 25 | 21.5 s |
| `bdd_catalog` | 1 of 1 | 9.9 s |
| `bdd_client_endpoint` | 55 of 57 | 42.7 s |
| `bdd_kubernetes` | 6 of 6 | 7.4 s |
| `bdd_quad_reads` | 111 of 111 | 27.0 s |
| `bdd_transactions` | 63 of 76 | 168.0 s |
| `bdd_wire_protocol` | 48 of 52 | 45.4 s |
| **all** | **308 of 328** | |

(`bdd_transactions` then held the files `bdd_sessions` holds now.) The 20 failures, by cause:

1. **The system group's quiet time** (`PROTOCOL.md` §11.5, design D7): for twice `transaction_ms` (20 s; no setting moves it)
   after the group begins to listen it evicts no session, refusing a Register that needs an eviction with code 14, and
   answers a resend's check of kind 1 with 14. Scenarios that start a cluster and at once fill the registry or resend met it,
   each refused "the registry is full, and the group evicts no session for twice transaction_ms after it begins to listen"
   (the group's own message) or "the session has a request in flight" (code 14): "An eviction reaches every core at once",
   "An evicted session is told so", "A retry of a session evicted meanwhile is unknown", "A Register answered busy is sent
   again", "A resent empty Commit answers not committed", "A retried commit commits once" (compio and tokio), and, newly
   failing because the split runs them earlier in their shared cluster's life, "An unsent commit is lost, a sent one in
   doubt" (compio and tokio), "A session commits through two partitions", "A retry of a request never committed is told so" and "A Recall
   reaching no answer stays in doubt". Which scenarios meet it depends only on how soon after a cluster's start they run.
2. **The stopped storage process hosts the system group.** "A session with a commit in flight is not evicted" ("the
   request's connection was lost"), "A full registry of commits in flight refuses a Register as busy" and "A Register busy
   past its attempts returns code 14" (each `TimedOut`) SIGSTOP the process `storage` to hold a commit in flight at its
   partition; since the switch that process hosts the group too, so the Register the scenario sends next is answered by no
   one. A quiet-time refusal is a code 14 answered at once, not a timeout. Proven by a temporary edit, since reverted: with the
   quiet time waited out and two storage processes, stopping the hosting one (`storage_1`) failed "A full registry of commits
   in flight refuses a Register as busy" with `TimedOut` (31.9 s); stopping the other (`storage_2`) passed it (22.9 s).
3. **"A partition silent past the wait withdraws the tightening"**: its step SIGSTOPs the hosting process, so the
   declaring Commit's answer from the group never comes: `OutcomeUnknown`.
4. **"A token above the open epoch is refused"** (four runs): §4.8 holds a Begin naming a token ahead of the view until the
   close arrives, so the step's three requests end `TimedOut` ("the close before the token's epoch … did not arrive by the
   deadline"), as §4.8 says they should.

"A tenant's data drop keeps its catalog" and "A catalog version past its cap is refused at its Commit", items 5 and 2 of
task 7.4's list, pass at the base.

A fifth cause showed only in the all-in-one binary under load, twice in two runs: **"A write the tightening forbids is
refused once the check passed"** restarts the hosting storage process and connects a client at once, which the frontend
refused, "the connection to the system group is closed" (Unavailable): `Cluster::restart` waits on `/health`, which storage
answers from its start, before the group listens, and the frontend's link dials the group again only every 100 ms.

### What changed, in the harness alone

- `tests/cluster.rs`: each cluster started records, by its client address, when its group's quiet time ends at the latest
  (its start's end, after the hosting process was ready, plus twice `DEFAULT_TRANSACTION_MS`), and again when a killed host
  is started again and ready (`Cluster::revive`). `after_quiet_time(at)` waits until then.
- `tests/lossy_link.rs`: `LossyLink::to` waits out the quiet time of the cluster it links to. Every scenario with a lossy
  link has its client resend, a check of kind 1.
- `tests/own_steps.rs`: the eviction steps (`evicted_commit`) and the registry steps (`registry_all_in_flight`,
  `oldest_in_flight`, `register_retried`, `register_exhausted`) wait out the quiet time; the registry steps run on two
  storage processes and stop the one not hosting the group (`storage_2`), their commit at an IRI of its partition 2
  (`two_storages`); `direct_resent`, a resend after a kill, waits out the quiet time.
- `tests/tightening_steps.rs`: the silent scenario runs on two storage processes and stops `storage_2`; its Then compares
  `detail` and the message with the stopped partition, 2, in place of the literal 1, and its late report is partition 2's
  own line in the host's log (partition 1 reports in time, so any report line would have satisfied the old check). The
  restart scenario waits until the restarted host and the frontend are ready, as `kubernetes_steps` does after a restart.
- `tests/request_steps.rs`: the two steps of the retired scenario, and the imports they alone used, removed.

No assertion was weakened; the quiet-time waits cost 20 s at most once per cluster that needs one.

### Scenario text edited

| Scenario | Feature | Change, and why |
|---|---|---|
| A token above the open epoch is refused | `transactions/tokens.feature` | **retired**: it contradicts `PROTOCOL.md` §4.8 as this stage writes it, which holds a Begin naming a token ahead of the view until the close arrives and sends such a write to its partition. Group 5 retired the same scenario from `common_frontend` for the same reason; its replacement there is client-endpoint's "A token ahead of the core's view waits and is not refused" |

No other scenario's text changed. No step of this crate named the frontend as the books' host (D12); the steps that named
the books' host by stopping `storage` are listed above, their text unchanged.

### After

Each target once, `--nocache_test_results`, no load added, after the changes (scenarios per run: compio TCP / Unix / TLS,
tokio TCP / TLS):

| Target | Scenarios | Per run | Steps | Time |
|---|---|---|---|---|
| `bdd` | **25 of 25** | 24/1/0/0/0 | 51 | 11.8 s |
| `bdd_catalog` | **1 of 1** | 1/0/0/0/0 | 2 | 9.9 s |
| `bdd_client_endpoint` | **57 of 57** | 30/2/9/7/9 | 114 | 67.5 s |
| `bdd_kubernetes` | **6 of 6** | 6/0/0/0/0 | 12 | 6.9 s |
| `bdd_quad_reads` | **111 of 111** | 53/7/17/17/17 | 222 | 27.1 s |
| `bdd_sessions` | **14 of 14** | 12/0/0/2/0 | 28 | 154.3 s |
| `bdd_transactions` | **58 of 58** | 40/0/6/6/6 | 116 | 18.6 s |
| `bdd_wire_protocol` | **52 of 52** | 41/0/3/5/3 | 104 | 82.0 s |
| **all** | **324 of 324** | 207/10/35/37/35 | | |

324 = the 328 of before less the retired scenario's four runs (compio TCP, compio TLS, tokio TCP, tokio TLS: it is
`@core`). The same targets passed 3 runs of 3 each under 16 `yes` processes and two disk-writing loops (load average 25 to
45; `bdd_sessions` then within `bdd_transactions`, 166.7 s, the slowest), and once more under the `yes` processes alone.
`bdd_sessions` is now the slowest: its clusters of their own specs each wait out a quiet time once.

## The hang at a binary's end

Task 7.5's gate saw `ultragraphdb_tests:bdd` (whole) and once `frontend_tests:bdd` print every summary and then run until
Bazel's timeout, under a load average of 12 with another agent's builds.

**Not reproduced, so not root-caused.** Runs made to reproduce it, each with a watcher sampling (`sample`) any test binary
alive past its bound:

| Runs | Load | Ended normally |
|---|---|---|
| the seven split targets, once (base) | none added | 7 of 7 |
| `bdd`, `bdd_kubernetes`, `bdd_wire_protocol`, 4 each (base) | 16 `yes` processes, load average to 150 | 12 of 12 |
| the seven split targets, once (fixed) | 16 `yes` | 7 of 7 |
| the seven split targets, 3 each (fixed) | 16 `yes` and two `dd`/`sync` loops, load average 25 to 45 | 21 of 21 |
| every feature in one temporary target, as before the split, 2 runs, `--test_timeout=1200` for that diagnosis alone | the same | 2 of 2, 274 s and 268 s, each failing and so ending by `std::process::exit(1)` |

What was excluded by reading: Bazel's `test-setup.sh` kills the test's process group once the binary exits, so a role
process left behind cannot hold the target; the hang is the test binary not exiting. `service_utils`' drop sends SIGKILL to
each child still running and reaps it, which a stopped child does not resist; `wait_utils`' and the harness's HTTP probes
are bounded; a step's work is bounded by `STEP_WAIT`; compio's `Runtime` drop and its polling driver's drop block on
nothing; cucumber hands a World's `Arc` to the writers and holds none past the summary. Unbounded by any timer are the
`Runtime::new()?.block_on` calls of `members_at`, `placement_ranges` and the tightening steps' `latest_version`, and
`frontend_tests`' `client_refusal`, each of which runs inside a step, before a summary.

Two changes since the observation bear on it without proving anything: the compio polling driver lost every later event of
a batch once a write finished before a read on one socket (compio-rs/compio#1119, fixed here by `65b35fd4` and `1ecdbe36`,
after task 7.5's gate), which leaves a future waiting forever on an event that came; and a Bazel server for a copy of this
repository stood in the session's scratch directory (`ug-compio-trial`), whose integration tests, if run then, bind the same
declared ports outside this server's `exclusive`. Neither ties to a binary that has printed its last summary.

Since each target now ends in a fraction of the bound, a hang would show as a `TIMEOUT` of one target; the watcher
(`sample` of the binary and the process tree) is the evidence to take then.

## Clippy

The three unfulfilled expectations removed: `#[expect(clippy::disallowed_methods)]` on `LossyLink::to` and `carry`
(`tests/lossy_link.rs:49`, `:125`) and on `clock` (`tests/own_steps.rs:37`). The aspect reads rules_rust's `.clippy.toml`,
not the repository's `clippy.toml`, so `disallowed_methods` never fires on these test targets, which carry no `lint_config`;
`src/utils/harness.rs`'s expectation on `start_double` is in the library, which does, and is fulfilled. Nothing was put in
their place, and no `#[allow]` or `#[expect]` was added.

`bazel build --aspects=@rules_rust//rust:defs.bzl%rust_clippy_aspect --output_groups=clippy_checks
//ultragraphdb/ultragraphdb_tests:all`: clean over the library, `unit` and the eight scenario targets. Shown to bite: one
expectation put back on `clock` for the probe failed `:bdd` with "this lint expectation is unfulfilled"; reverted.
`make check`'s `cargo clippy --all-targets -D clippy::disallowed_methods` leaves the scenario target out (`test = false`).

## Found wanting

1. **The retired token scenario still stands in `openspec/specs/transactions/spec.md`** ("A token above the open epoch is
   refused"), and the change's delta spec (`specs/transactions/spec.md`) modifies only "Exactly-once commits through
   sessions", so the archive will keep a scenario §4.8 contradicts. The delta needs that requirement's scenario removed or
   replaced; spec text, the user's to approve.
2. **The frontend's `/ready` stays 200 while its connection to the system group is down.** After the host restarts, the
   frontend refuses clients "the connection to the system group is closed" though `/ready` answers 200: readiness is set once
   (`system_link_reads.rs`) and never withdrawn. Design D10 says the frontend is ready once "the group answers", and says
   nothing of afterwards. The harness waits on storage's `/ready`, which follows the group's listen, as `kubernetes_steps`
   does; a client arriving between that and the link's redial is still refused. The user's to decide.
3. **`scripts/bdd_counts.sh` compares `:bdd` alone** with `cargo test --test bdd`; for `ultragraphdb_tests` and
   `ultragraphdb_common_frontend`, both split, `:bdd` is the catch-all, so the script reports a mismatch. `ultragraphdb_tests`'
   bdd cannot run under Cargo anyway (its binaries are copied in by Bazel).
4. **`bdd_transactions` is the slowest target**, about 100 to 170 s, most of it the limit conversations' 11.5 s waits shared
   per run and the per-run clusters; each target is below the 300 s bound, none below 10 s (deferred).

## Notes for the commit

Four commits, each over the files named (one file's hunks across two where marked):

1. **The split**: `ultragraphdb/ultragraphdb_tests/BUILD.bazel`.
2. **The hang**: no change; this record's section says what was run.
3. **The lints**: the three `#[expect]` removals, `tests/lossy_link.rs` (the two attributes; the `to` hunk also holds
   commit 4's wait and doc line) and `tests/own_steps.rs` (the attribute on `clock`).
4. **The scenarios**: `features/transactions/tokens.feature`, `tests/cluster.rs`, `tests/lossy_link.rs` (the quiet-time
   wait), `tests/own_steps.rs` (the steps), `tests/request_steps.rs`, `tests/tightening_steps.rs`, and this file.

Files to delete: none.

# Wave A, part 2: `frontend_tests`, `storage_tests`, `compute_tests` (tasks 4.2, 5.1, 8.1 and 8.2 in these crates)

Base `1be22e5b`, beside part 1 in the same tree; none of part 1's files is part of this record. Counts are those Bazel's test
logs report; times are Bazel's for a run with `--nocache_test_results`. Every scenario below runs the built binaries as
processes, settings served by the config double, a storage process hosting the system group by its `[system]` table.

## Task 8.1 in the three crates

### Before

Each target once at the base, uncached (2026-11-17, 12:00):

| Target | Scenarios | Time |
|---|---|---|
| `frontend_tests:bdd` | 18 of 18 | 14.0 s |
| `storage_tests:bdd` | 98 of 98 | 95.4 s |
| `storage_tests:system_bdd` | 7 of 7 | 5.0 s |
| `storage_tests:system_partitions_bdd` | 4 of 4 | 11.1 s |
| `compute_tests:bdd` | 89 of 90 | 32.0 s |

The one failure, compute_tests' "A peer that stops answering without closing is a lost dependency": compute exited 69
naming the system group's address, "compute lost the system group at tcp://127.0.0.1:17081, which did not return within
800 ms", where the step asserts the storage partition's address.

### The edited scenario

| Scenario | Feature | The step that named the books' host, and the change |
|---|---|---|
| A peer that stops answering without closing is a lost dependency (and "A lost dependency exits with its code", which shares its When's setup) | `compute_tests/features/role_binaries/role_binaries.feature` | `lose_storage` (`tests/bdd.rs`) stopped or killed `storage`, which since the switch hosts the system group too, so compute lost both and named the group. **Text unchanged; the setup changed**: two storage processes, the first hosting the group and partition 1, the second (`127.0.0.1:17085`, observability `18085`) holding partition 2 alone; the signal goes to the second, and the Then asserts compute names `127.0.0.1:17085` |

What compute must name, by the spec: system-group, "A lost system group is waited for until the deadline", has a process
that dials the group exit 69 "once the group has been gone for `join_deadline_ms`, naming the address", and role-binaries'
two scenarios name no address. A hung or killed host is the group gone, so compute naming the group's address there is the
spec's (group 6's *Found wanting* 4); the scenario is about a hung peer found by a question with a limit, which a
partition apart from the host isolates. The product is unchanged.

Feature texts that still name the frontend where the system group is meant, green and **not edited** (each binding plays
or starts the group's host; the change's spec modifies the first two): `storage_tests/features/role_binaries/role_binaries.feature`
"A role with the wrong secret is refused" ("a storage process started with a different role secret connects to the
frontend"; this change's text is "a frontend started with a different role secret connects to the system group"), "A
subscription past the closes held exits with its code" ("the frontend ends a storage process's subscription …"; this
change's text names the system group), "Storage stopped before it joined is never ready" ("while the frontend holds its
Join's reply"), "A partition leases its blocks ahead of need" ("joins the frontend"); `compute_tests` "Compute is ready
once a partition has joined" ("compute has joined a frontend no partition has joined"). Their text is the user's to bring
to this change's.

### After

Each target once, `--nocache_test_results`, in one invocation after every change (no hang at a binary's end seen):

| Target | Before | After | Steps | Time |
|---|---|---|---|---|
| `frontend_tests:bdd` | 18 of 18 | **29 of 29** (11 added) | 58 | 46.8 s |
| `storage_tests:bdd` | 98 of 98 | **98 of 98** | 196 | 98.5 s |
| `storage_tests:drops_bdd` (new) | | **2 of 2** | 4 | 1.4 s |
| `storage_tests:system_bdd` | 7 of 7 | **8 of 8** (1 added) | 16 | 6.3 s |
| `storage_tests:system_books_bdd` (new) | | **4 of 4** | 8 | 3.5 s |
| `storage_tests:system_partitions_bdd` | 4 of 4 | **4 of 4** | 8 | 9.2 s |
| `compute_tests:bdd` | 89 of 90 | **90 of 90** | 180 | 27.5 s |

## Task 4.2: the unbound scenarios over the storage binary (`storage_tests`)

Bound by a delegated run, in `features/system/books.feature` (`@system @books`, a new target `system_books_bdd`) and
`features/system/pinned.feature` (`@system @hosting`, in `system_bdd`), steps in `tests/system_books_steps.rs`:

| Scenario | Bound as |
|---|---|
| The books recover in the storage process | the host at `sequencer.epoch_ms_max` 10,000, its closes driven one by one with CloseRequest to exactly 12 after a Declare of kind 1, two Leases; Catalog, a Challenge of the bootstrap `admin` and Placement read before a kill -9 and after the restart: equal values, the latest close 12, a third lease at or past the second's end |
| A partition's sync holds up no close | partition 2 in a non-hosting process stopped by SIGSTOP for ten times `epoch_ms_max` (50 ms): the closes go on in order meanwhile, and once it runs again a commit lands past the last close sent during the stop |
| A partition recovers from the group of its own process | three Declares of kind 1 as role `frontend`, a commit at partition 1 at or past version 3's effective epoch, kill -9, restart: the log's order "the system group listens", "a catalog version read … version=3", the partition's recovery with rounds above 0 |
| A failed log of the group stops the process | `metadata.snapshot_rounds = 1`, a directory made at `system.data_dir/meta/superblock.{a,b}.tmp` once ready, a Lease: the snapshot's trim fails creating the superblock's temporary file (EISDIR); exit 74, "a partition or the system group stopped; stopping the rest", "stopping role=storage", "failure=io: ordered log: storage create failed". No product hook |
| Every runtime of a pinned role holds a CPU of its own | five partitions, `pin_cores` and `pin_skip = 1`: on macOS each partition's runtime logs "unpinned: this platform cannot bind"; on Linux the expected CPUs are derived from `/proc/self/status`'s permitted set with `common_service::cpu_for`, refused naming the precondition below six CPUs |

**Weaker than the text, the user's to accept:** (1) "read at the same log position": the restarted partition appends its
catalog report to the metadata log before any read over a role connection can be made (before the restart the reads
answered at positions 23 to 25, after it 28 to 29), so the binding asserts equal values at a position at or past every one
before, not the same position; `common_system`'s binding over simulated storage keeps the equality. (2) the delayed sync is
a whole non-hosting process stopped, not one partition's sync inside the hosting process, which the binary has no means to
delay; the in-process isolation rests on `common_system`'s binding. (3) the Linux branch of the pinned scenario was not run
(this machine is macOS), and nothing restricts the process to six CPUs.
No product defect was found; none was fixed. **4.2 is not ticked** for (1) to (3).

| Target | Before | After | Time |
|---|---|---|---|
| `storage_tests:system_bdd` | 7 | 8 (the pinned scenario) | see *After* |
| `storage_tests:system_books_bdd` (new) | | 4 | see *After* |

## Task 5.1: the unbound scenarios over the binaries

Bound in `frontend_tests` (`tests/processes.rs` starts, kills and restarts the host, a second storage process at
`127.0.0.1:17085`, compute and the frontend; steps in `tests/system_steps.rs` and `tests/books_steps.rs`):

| Scenario | Feature | First run |
|---|---|---|
| The frontend listens at no role address | `client-endpoint/system_group.feature` | the step's own error (`lsof` not on the sandbox's `PATH`, now `/usr/sbin/lsof`); then passes: the frontend's TCP listeners, read back from the OS (macOS `lsof -a -p`, Linux `/proc/<pid>/net/tcp` by the process's socket inodes, the Linux branch not run here), are exactly its client and observability addresses |
| A restarted frontend serves the sessions it served before (also task 8.2's) | the same | passes: request 1 committed, the frontend killed and restarted, request 2 through the resumed session commits as request 2; the frontend's settings name no `data_dir`, and its restart's log says nothing of a recovery or a replay |
| A storage process restarted at its address is dialled again | the same | passes: partition 2's process killed and started again at its address, Members lists it there, and the frontend's next write to an IRI of partition 2 is answered |
| A check outlives the frontend that asked for it | `schema-changes/tightening.feature` | passes: `a` in partition 2, whose process is stopped (SIGSTOP); the declaring Commit sent and left, the version being checked held at the group, the frontend killed, partition 2 continued: its report `refused=true` in the host's log, the latest version two above the one before, and a Describe through a restarted frontend reports `nick` not single-valued. **Weaker than the text:** partition 1, in the hosting process, holds none of `nick`'s data and may report its pass before the frontend stops; "before any partition reports" holds for the partition whose data refuses it |
| A partition purges an entry only after the registry's eviction | `transactions/sessions.feature` | passes: `sessions_max` 1, closes at most 10 s apart and asked for by the test; the quiet time waited out (twice `transaction_ms`, 20 s, no setting moves it); after a close the second Register evicts the first session, a Recall at partition 1 answers its stored reply (a Commit) with no close on the test's subscription meanwhile, and once the next close is asked for and arrives, the Recall answers 18 |
| The frontend binary carries no book | `role_binaries/role_binaries.feature` | **failed**, then fixed (below) |
| The frontend is ready only once the books answer | the same | the premise check failed on the first binding (storage ready before the frontend started, the frontend's start having waited for its `/health`); with both started unwaited after a kill -9 over 200 rounds, passes: storage not ready as the frontend started, the frontend's `/health` answering before its `/ready`, and storage's `/ready` answering when the frontend's first does |
| Dropping a predicate removes it from the predicate lists; Dropping a graph keeps the other graphs | `storage_tests/features/schema-changes/drops.feature` (`@drops`, a new target `drops_bdd`, by a delegated run) | pass: each Drop sent by a client through the frontend; the partition's keys read back by a Scan of the tenant's range in every keyspace a Scan reads, decoded with `QuadKey::parse`, the subject-predicate lists as compute reads them; shown to bite by a temporary edit dropping `since` for `knows` and `<c>` for `<g1>` |

Not bound, each a specification question for the user:

- schema-changes "Dropping a vector predicate drops its vector index": the frontend refuses a vector-index declaration until
  stage 8 (`client_session_writes.rs:293`, bound as "A vector index is refused"), so no vector-index key can exist over the
  binaries.
- schema-changes "Dropping a tenant's everything survives a restart" (tenant 3) and "A tenant's drop leaves a tenant sharing
  its run" (tenants 7 and 8): this stage holds tenant 0 alone; metadata-group, "The metadata group holds the tenants", gives
  further tenants stage 7's `operate` command, and no client or role path makes one.
- role-binaries "A role with the wrong secret is refused", the frontend's text of this change: not bound in this run.

**5.1 is not ticked**: three scenarios cannot be bound in this stage.

### Product fixes, each failing first

1. **The frontend binary's graph named the books.** "The frontend binary carries no book" failed: "ultragraphdb_common_system
   is in the graph: [//ultragraphdb_common/ultragraphdb_common_system:src/lib.rs, …]", and `bazel query somepath` led from
   the frontend binary through `ultragraphdb_common_frontend` to `common_system`, `cluster_metadata`, `cluster_sequencer` and
   `transactions_log`. Root cause: rules_rs's `aliases()` names every dependency's label, the dev-dependencies' among them
   (the system group `common_frontend`'s tests run), and `aliases` is a label-keyed attribute, so Bazel takes each label as a
   dependency of the library and of every binary over it. Nothing of them was linked: `nm` of the frontend binary finds 0
   symbols of each, 6,487 of `common_frontend`. Fixed in `ultragraphdb_common_frontend/BUILD.bazel`: the library's aliases are
   those of its normal dependencies alone. After: no path from the frontend binary to any of the four, the scenario passes,
   and the non-integration targets stay cached (the rename was a no-op: each target is named for its crate).
   The same `aliases()` stands in `ultragraphdb_common_system` (its dev-dependencies `store_sim` and `tool_bdd` in the storage
   binary's graph) and `ultragraphdb_transactions_log` (`tool_bdd`); not changed here.
2. **A lost system group ended the frontend outside its ordinary path.** system-group, "A group gone past the deadline ends
   its dependants", the frontend's part, failed at "stopping role=frontend": the log held only "unavailable: the system group
   at tcp://127.0.0.1:17081 has been gone for join_deadline_ms, 800 ms". `frontend_role` returned its error at once, so no core
   stopped accepting and no open request was finished, against role-binaries' SIGTERM path and the function's own comment.
   Fixed in `ultragraphdb_frontend/src/service.rs`: the loss logs the failure and "stopping role=frontend", sets the stop every
   core watches, finishes the open requests as a SIGTERM does, and then returns the error (exit 69). After: passes.
   **The same gap in compute, not changed:** `compute_role` returns `hold_peers`' error through `?`, skipping `finish_open`
   and logging no stop; compute_tests' scenarios assert only the code and the address. Group 6's binary; the user's to direct.

## Task 8.2: system-group, "A lost system group is waited for until the deadline"

`frontend_tests/features/system-group/lost_group.feature`, the frontend's part (compute's is `compute_tests`'):

| Scenario | First run |
|---|---|
| A frontend started before storage waits | passes: the frontend's `/ready` 503 at its start and 500 ms on, the host started, storage's `/ready` answering when the frontend's first does |
| A group gone past the deadline ends its dependants (the frontend's part) | **failed**, fixed (*Product fixes* 2): exit 69 naming `127.0.0.1:17081`, "stopping role=frontend" |
| A commit during the outage is refused, not lost | passes: partition 2 in a non-hosting process, the host killed; the session's one-frame write answered code 17; a Recall at partition 2 answers 18 (its table holds nothing of it); the host started again, the client's resend of the same request commits it |
| An outage bans no source | passes: the host killed, 80 authentications each answered Error 17 on stream 0; the host started again, the endpoint cores answer 17 until their links have dialled it, and the first other answer authenticates (80 counted failures would have banned the address past 70) |
| A restarted frontend serves the sessions it served before (client-endpoint) | bound once, with task 5.1 |

Not bound: "A partition without closes commits until its epoch is full". An epoch holds `Version::COMMIT_SEQUENCE_MAX`,
4,194,302 commit sequences (`ultragraphdb_core_ids`), and no setting lowers it, so filling one over the binaries takes four
million commits. **8.2 is not ticked.**

## The gate

`make build` passes. `bazel test //... --test_tag_filters=-integration_test`: 179 of 179 pass (all cached: the
`common_frontend` BUILD change renames nothing a compile action reads). The seven integration targets of the three crates
pass uncached (*After*).

## Found wanting

1. **The frontend's `/ready` cannot tell a client its link is back** (part 1's *Found wanting* 2 seen again): "An outage bans
   no source" waits for the group by authenticating until a core relays, each refusal a 17 that counts nothing.
2. **`frontend_tests:bdd` and `storage_tests:bdd` past the bound** (deferred by the user): see *After*.
3. **Spec scenarios this stage cannot hold**: the vector-index drop (stage 8), tenants 3, 7 and 8 (stage 7), an epoch
   filled (four million commits): the change's delta needs them scoped, the user's to decide.

## Notes for the commit

Files to delete: none. Commits, by task:

1. compute_tests (8.1): `ultragraphdb/ultragraphdb_compute_tests/tests/bdd.rs`.
2. frontend, its library's graph and frontend_tests (5.1 and 8.2): `ultragraphdb_common/ultragraphdb_common_frontend/BUILD.bazel`,
   `ultragraphdb/ultragraphdb_frontend/src/service.rs`, `ultragraphdb/ultragraphdb_frontend_tests/` (`BUILD.bazel`,
   `features/role_binaries/role_binaries.feature`, the new `features/client-endpoint/system_group.feature`,
   `features/system-group/lost_group.feature`, `features/schema-changes/tightening.feature`,
   `features/transactions/sessions.feature`, `tests/bdd.rs`, `tests/endpoint_steps.rs`, the new `tests/processes.rs`,
   `tests/system_steps.rs`, `tests/books_steps.rs`).
3. storage_tests, task 4.2: `features/system/books.feature`, `features/system/pinned.feature`, `tests/system_books_steps.rs`,
   `tests/system_bdd.rs`, and `BUILD.bazel`'s `system_*` hunks.
4. storage_tests, the Drops (5.1): `features/schema-changes/drops.feature`, `tests/drop_steps.rs`, `tests/bdd.rs`, and
   `BUILD.bazel`'s `bdd`/`drops_bdd` hunk.

This section goes with whichever commit takes this file. `Cargo.lock` is unchanged.

# The gate of task 7.6

`make test` on 2026-11-17 at `4ea0abea`, the first full gate since group 0, on a quiet machine: every phase passed.

| Phase | Result |
|---|---|
| doc tests | 20 of 20 |
| unit tests | 92 of 92 |
| property tests | 10 of 10 |
| concurrency tests | 5 of 5 |
| BDD tests | 52 of 52 |
| integration tests | 18 of 18, 12 run and 6 cached |

No binary hung at its end. With it, tasks 4.2, 5.1, 7.6, 8.1 and 8.2 are complete: the scenarios that need a later stage's
work moved to `openspec/notes/backlog.md` (items 21 and 22); "A partition without closes commits until its epoch is full"
is bound at the partition; "A partition's sync holds up no close" is bound in-process in `common_system` beside the
process-level binding in `storage_tests`.

## 8.4 The development image booted once per architecture

Machine: this Mac (Apple M3 Max), Docker 29.8.2 in OrbStack 2.2.5 (`linux/arm64` daemon, kernel 7.2.12-orbstack, amd64 by
Rosetta), the runtime `image_bdd` uses. Build: `bazel build -c opt //ultragraphdb/ultragraphdb_supervisor:development_multiarch`
at `7316c828`, index `sha256:fcf7b9ef…` (amd64 `sha256:6eafd3cd…`, arm64 `sha256:87ac04d4…`). Each platform loaded with
`development_test_load -- --platform linux/<arch>` under a repository of the run's own, and started with `docker run
--platform linux/<arch>` on its default `dev` profile under the shipped seccomp profile, the client endpoint published. The
image holds no shell, so `/data` was listed through `docker cp <container>:/data - | tar -tv`.

| | arm64 (native) | amd64 (Rosetta) |
|---|---|---|
| Order in the supervisor's log | config `/ready`, storage started, storage `/ready`, compute started, compute `/ready`, frontend started, frontend `/ready`, every role is ready | the same |
| Container start to "every role is ready" | 532 ms | 1,134 ms |
| Supervisor's first line to it | 457 ms | 1,052 ms |
| Started to `/ready`: config, storage, compute, frontend | 21, 192, 20, 213 ms | 81, 598, 87, 264 ms |
| Driver of every runtime (11) | io_uring | polling |
| SIGTERM (`docker stop`) | frontend, compute, storage, config stopped, each 0; exit 0 | the same |

`/data` (65532, mode 0700) held exactly `config/`, `storage/` and `system/` on both: `system/meta/` and `system/seq/`, each a
segment and its superblock, and `storage/p1/r1/` with `store/` and `wal/`; no directory of the frontend's (design D11). The
storage process logged the system group listening at `unix:///run/ultragraph/system.sock` before its partition recovered.

Under the shipped profile, which lets the arm64 image start on io_uring, every amd64 runtime started on the polling driver:
the refusal is the emulation's, as stage 5.5 task 12.6 found, and the fallback served readiness with no warning or error.
The errno is not logged. The amd64 image on io_uring stays for physical Linux.

`start_three_hosts` (`supervisor_tests/tests/images.rs`, the role images of "The same frames cross TCP") started the
frontend's container before storage's (group 7, task 7.5, found wanting 3). It now starts storage, compute and the frontend
in D10's order, each ready before the next; `image_bdd` passes, 17.3 s uncached.

# Task 8.3: the failure runs

Five scenarios over the built binaries in `ultragraphdb_tests`, `features/transactions/failure_{host,frontend,link,order}.feature`,
steps in `tests/failure_steps.rs`, one target each (`bdd_failure_host`, `_frontend`, `_link`, `_order`). Each Commit writes
`<subject> <predicate> _:b`, its predicate its own and homed in the subject's partition: a blank node is minted anew at each
application, so a Commit applied twice holds two statements. After the fault each session resends what it holds in doubt,
again after 250 ms while it is answered 14 or 17 (§11.5), within twice `transaction_ms` and `START_SECONDS`; the subject is read
back in read-write transactions and each predicate counted: committed exactly once, not committed nowhere, nothing else, each
session's next Commit one above its committed requests.

| Run | Seen | Time ×3, uncached |
|---|---|---|
| Host (`storage_1`) killed, three Commits at the stopped partition 2, a fourth sent while the group is gone | the three acknowledged; the fourth 17, its resend after the restarted group's quiet time 18 | 22.4 / 22.4 / 22.4 s |
| Frontend killed while `nick` is checked against the stopped partition 2, three Commits there | all three in doubt; resends 14 for the twice `transaction_ms` the group keeps the killed connection's marks, then the stored reply; the tightening's version once | 22.4 / 22.2 / 22.1 s |
| The frontend's connection to the group cut (a `GroupLink` from `[::1]` to the group's port) under three Commits at the stopped partition 2, a fourth sent while cut | the three acknowledged; 16 → 32 connections dialled; the fourth 17, resolved 18 after the quiet time; each session's next Commit passes its check at once (the Endeds reached the group on the new connection) | 21.5 / 21.5 / 21.4 s |
| Frontend started alone, then the host, Commits at partitions 1 and 2 as `storage_2` starts; and a Drop through a core one close behind | `/ready` 503 before storage; every Commit acknowledged, none in doubt. The Drop: the link stalls every frame from the group, a version is declared at the group directly, probes show the partition at its effective epoch; the Drop naming Describe's earlier version is refused by the partition, code 2, kind 5, nothing removed; sent again with its number after the closes arrive, it commits, and the next request is held alone | 3.4 / 3.4 / 3.4 s |

The link stalls all frames rather than holding the closes: holding one frame of a connection breaks `sequence`
(§2.5, step 13).

## Defect found and fixed

`SystemLink`'s reading task dropped a frame `view()` refused and kept its connection: after one lost or reordered frame every
later frame breaks `sequence`, so the core's asks and checks waited for ever. §2.6: the side that dialled closes the
connection. Fixed in `system_link_tasks.rs`; test first, `test_a_frame_the_link_refuses_closes_its_connection` (failed
"the Session was left unanswered", passes). Body-level refusals inside `deliver` are still dropped silently; other role readers
not audited.

Body-level refusals, and the other readers (follow-up). §13: the side that dialled closes the connection for any frame it
refuses, body included, and answers what it carried with code 17; the side that accepted answers a frame-level rule on
stream 0 and closes. Fixed, test first:
- `SystemLink`: `deliver` refuses a frame on stream 0 or a stream not yet opened, a command its request is not answered
  by (§2.5 step 3), and a subscription's close or Error, a Check reply or Error, whose body breaks its rules; an ask's
  answer is read inside the ask, whose refusal closes the connection it came on (`refuse`, by connection number). Tests
  `test_a_subscription_frame_the_link_refuses_closes_its_connection` (failed "the check refused: none"),
  `test_a_check_reply_the_link_refuses_closes_its_connection` (failed code 1 for 17),
  `test_an_answer_the_link_refuses_closes_its_connection` (failed "refused: none": a Session's body was never read),
  `test_a_frame_on_a_stream_not_opened_closes_the_connection` (failed "the Session was left unanswered").
- compute `closed_epoch`: an Error whose body does not read closes the link, 17 (was code 1, link kept); `ClosedAnswer::BadError`.
- storage `StorageSession::answer`: a frame out of sequence closes the connection (`Answer::Close`), was a stream error.
- Clippy in the system link: a type for a close's parts; no conversion of a PathBuf to itself; the test splice runs on
  compio tasks rather than threads the workspace disallows.

Found wanting by the audit:
1. The frontend's client sessions read partition and compute replies without closing the `RoleLink` on a refused body or
   command: a malformed Bind binds at read version 0; other commands are ignored or relayed verbatim.
2. The client core checks a Result frame's chunks only when read: one dropped by a cancel, a dropped read or a
   transaction's end, or read from a kept read on the live connection, closes nothing.
3. The partition core refuses steps 9–12 as a stream error; it needs a `Push` that closes.
4. `serve.rs` does not tell the session of a stream the machine refused (`Inbound::Refused`).
5. Unreachable from a conforming peer, no fake yet: `RoleClient::request` takes a reply on any stream; compute's `Link`
   drops frames on unknown streams unchecked; compute's `closes` drops unasked frames unchecked; storage's
   `deliver_closes` reads frames of any stream and takes an Error 1 on stream 0 as the subscription's end.
6. `serve.rs` answers an Ended arriving while stopping with Error 17 (§13.25: nothing answers it).
7. **Fixed, test first.** `compute_core_catalog.rs` answers every frame it refuses on the catalog read with code 17, as
   §11.12 (*Errors*) has the side that dialled answer a request whose connection closed on a refused frame; §13.7 still
   classes the frame (`FieldValue` `"catalog"`). The six refusals (a frame that does not read, an Error body that does not
   read, a wrong stream, past credit, `CatalogFrames::push`, past `CATALOG_FRAMES_MAX`) go through one `refused`; the
   group's own Error passes through unchanged. Tests: `test_a_version_in_the_most_frames_is_read_and_one_more_refused`
   (failed `Protocol` for `Unavailable`) and the new `test_a_frame_breaking_the_first_ones_header_is_refused_with_code_17`
   (failed the same way; the played group's `answer_torn` names the next version in frames past the first).
8. **Fixed (harness).** `system_books_bdd` "A failed log of the group stops the process" made `meta/` read-only while the
   group ran; a superblock write past its create when the mode changed failed at `rename`. The step now leaves the
   directory writable and makes a directory at `superblock.a.tmp` and `superblock.b.tmp`, waiting (at most
   `START_SECONDS`) while a write's temporary file stands there. The next superblock write fails at its create
   (`storage create failed (is a directory)`), and only the superblock write fails, as root too. The exit-74 and
   stop-path assertions are unchanged. 20 of 20 uncached under 12 `yes` processes on 16 CPUs (3.4–7.3 s).

Items 1 to 6, fixed after the audit, test first (each new test failed for the reason given before its fix):
1. **The frontend's `RoleLink`** records each stream's requests: the one that opened it, and each later one `send` carries.
   In `deliver` it refuses a frame that answers none of its stream's requests (§2.5, step 3). It also refuses a Bind, Held,
   Commit, Drop, Reference or Error whose body breaks its rules, a Reference that is no reply included. A refusal closes
   the link (§2.6, §13): its streams are answered 17, and `RoleLinks` dials again. Result frames are not decoded there;
   the client checks them. `LinkEvent` carries the read reply (`Bound`, `Held`, `Registered`, `Refused`, `Frame` for a
   Result or a Commit's or Drop's reply relayed as it came), so the session handlers read no frame again: no Bind is taken
   as `read_version` 0, no other command is dropped or relayed. A partition's Error on a one-frame or Drop stream reaches
   the client rebuilt by `push_error`, fitted to its `max_frame`. Tests (`client_session_tests.rs`), each failing first:
   - `test_a_bind_answered_by_a_frame_refused_closes_the_link`, failed "no answer came";
   - `test_a_held_refused_closes_the_link`, failed "no answer came";
   - `test_a_read_answered_by_a_frame_refused_closes_the_link`, failed "no answer came";
   - `test_a_commit_reply_refused_closes_the_link`, failed with the Commit relayed;
   - `test_a_drop_reply_refused_closes_the_link`, failed with the Drop relayed;
   - `test_a_one_frame_write_answered_otherwise_closes_the_link`, failed with a Held relayed;
   - `test_a_reference_refused_closes_the_link`, failed with 17 answered and the link kept.

   The stricter link found test doubles breaking the protocol. `RoleDouble` answered a Drop's Bind with a
   `read_version`, and sent its replies through a session with no token; four BDD steps answered Commits the same way.
   They are fixed, and `committed` is exported for them.
2. **The client core** gives the live connection up when a broken Result frame is read from a read kept on it, its
   transaction refused (`test_a_broken_frame_of_a_kept_read_gives_the_live_connection_up`, failed "given up"). By design
   a Result frame's chunks are checked when it is read: one dropped unread by a cancel, a forgotten read or a
   transaction's end closes nothing.
3. **The partition core** answers a chunk or body-length failure (§2.5, steps 9–12) with `Push::Close`, whether checked
   as it takes a frame or, for a frame behind a parked request, in its turn. `serve.rs` sends that as Error 1 on stream 0
   and closes, as for `Answer::Close`.
   - `test_a_request_failing_its_checksum_closes_its_connection_now_or_in_its_turn` failed with `Refuse(.. "chunk 0, body
     bytes 0..24 ..")` for `Close(Chunk)`.
   - `test_a_pushed_close_ends_the_connection_with_error_1_on_stream_0` failed by timing out.
4. **`serve.rs`** tells the session `cancelled(stream)` when a refusal ends a stream it held open, such as a Credit with a
   bad body (`test_a_refusal_ending_an_open_stream_tells_the_session`, failed `[]` for `[1]`).
5. **Readers that took frames on any stream:**
   - **`RoleClient::request`** takes its reply on its own stream alone. It checks a frame on an ended stream whole and
     drops it, and refuses one on stream 0 or on a stream not opened, closing the connection.
     `test_a_reply_on_another_stream_is_refused_and_closes_the_connection` failed "a refusal: none";
     `test_a_late_frame_is_dropped_once_whole` failed "command 531 is not served".
   - **Compute's `Link`** refuses a stream at or above `next_stream`, and checks a late frame whole before dropping it.
     `test_a_frame_on_a_stream_not_opened_closes_the_link` and `test_a_late_frame_is_checked_whole_and_dropped` both
     failed "the lookup's refusal: none".
   - **Compute's `closes`** refuses a frame on stream 0 or on a stream not opened, and a late frame failing its check, then
     asks the group again (`test_a_frame_the_held_connection_refuses_closes_it`, failed "Ahead: the group asked again did
     not happen").
   - **Storage's `deliver_closes`** reads the subscription's stream alone and drops late frames. It takes an Error 1 on
     stream 0, or a refused frame, as the connection's end: it closes the connection and joins again, where it exited 69
     or kept the old client open through `retry_until`. The two new system-group scenarios
     (`storage_tests/features/system-group/lost_group.feature`) failed against the unfixed `service.rs`:
     - "A connection error ending a subscription's connection is joined again" failed "a second Join did not happen
       within 20 s";
     - "A frame on a stream storage has not opened closes the subscription's connection" failed with the first
       subscription's connection still open at the second Join.

   `serve_partition`'s listening moved to `accept_at` (clippy `too_many_lines`).
6. **`serve.rs`** answers no Ended, whether refused by the session or arriving while stopping, as it answers no
   CloseRequest (§13.25) (`test_a_refused_ended_is_answered_by_nothing`, failed `(16, 1)` for `(513, 2)`).

The last fix wave closed the four findings those fixes left:
1. **Storage `catalog_read`.** Catalog frames after the first were read without a check on their stream. A Catalog frame
   on stream 0, or on a stream not opened, was taken into the version, and a late frame failed the read. They now follow
   §2.5 step 3 as the subscription does:
   - a late frame is checked whole and dropped;
   - the group's Error on stream 0 or on the version's stream is its refusal;
   - a frame on any other stream is refused (`CommandNotServed`), and dropping the client closes the connection.

   An Error on the version's stream after the first frame now passes through with its own code, where it was refused as
   `WrongCommand`. Tests: three scenarios in `storage_tests` `lost_group.feature` over the played group, and in the
   system-group spec (failed: catalog connections 2 for 1, 1 for 2, 1 for 2).
2. **Frames behind a closing frame.** `serve.rs` already stops at the first Close. The window was in the partition core:
   the storage session answers `Open` at once, so it took frames its connection handed it before the close reached the
   session. The core now keeps the connections it closed (`refused`) until their `Closed`, and takes none of their later
   frames (§2.6). Test: `test_nothing_behind_a_frame_closing_its_connection_is_taken` (failed "the DirectWrite behind the
   closing frame was taken"). The checksum test's in-turn half moved to a second connection, since §2.6 no longer allows
   using a closed one.
3. **Compute's `Link`.** A frame on a stream whose holder dropped its receiver without `forget` (a Scan that could not be
   sent) was dropped unchecked. It is now late: checked whole, and the stream ends. Test:
   `test_a_frame_of_a_stream_whose_holder_is_gone_is_checked_whole` (failed "the lookup's refusal: none").
4. **`bdd_endpoint` "A role connection's dial past its bound closes it"** failed 15 of 60 uncached under 32 `yes`
   processes, every one `Unavailable` "no compute process answers". The step put the silent compute in the core's links
   alone. The start-up Members reads run every 100 ms while no partition answers (D9, D10), and they replaced it with the
   group's empty list. The step now joins it at the group and reads Members. 60 of 60 uncached under the same load.

Found wanting by the fixes, recorded in `openspec/notes/backlog.md`:
1. `StorageSession::answered` does not count `Push::Close` as ending a stream. This is harmless, since the connection
   closes.
2. Compute's `ScanStream::open` does not `forget` its stream when `link.send` fails; the entry stays until a frame
   arrives on that stream or the link closes.
3. Compute's `Link`: when `write_frames` ends on a write error, the link is not marked closed; later sends are answered 17
   and nothing dials again until the reader ends.
4. Storage's `held_versions`: an Error 7 on a version's stream after its first frame ends that tenant's versions quietly.
5. The late, stray and stream-0 rule is written three times: `RoleClient::arrived`, storage's `subscribed` and its
   `catalog_frame`.

## Bite evidence (temporary edits, reverted)

Counting assertions through `failure_order`: a predicate committed again fails "held once", a stray statement fails "no other
statement", a committed Commit counted not committed fails "held nowhere". The Drop run fails with the partition's
`drop_stale` disabled (all 5 statements removed). The link run fails with `SessionMarks::end` ending a mark only on its own
connection (the next Commit left in doubt). The host and frontend runs have no product-side bite.

## Found wanting

1. **Fixed (harness): `bdd_client_endpoint` "A read is never resent after a frame" failed intermittently** (facts `rows 12,
   rows 12, rows 1`). The kill was not late: a read's credit (4 frames of the fixture's about 7) keeps compute from finishing
   a read before the client grants more, which it does only after the kill. Both reads had gone to compute_1. Under D9 a
   frontend core learns compute processes from Members at its start, and again only when a role connection closes or no
   compute is left; the harness started the frontend before storage and compute, so a core whose start-up Members read fell
   before compute_2 joined listed one compute for its lifetime. "Reads alternate between two compute processes" failed the
   same way. Evidence: temporary logging of each dispatch's address and the core's compute count; the three two-compute
   scenarios ×10 under 12 `yes` processes failed 6 of 10, every failing run `computes=1`; after the fix 10 of 10 passed,
   `computes=2` on every dispatch. Fix: `tests/cluster.rs` `plan` starts storage, compute, then the frontend (D10's order).
   `bdd_client_endpoint` 20 of 20 uncached under load (72–90 s); the package's 13 targets pass. A compute process that
   joins while the frontend serves is found at the next of D9's events, as D9 states; stage 5.9 has no compute process to
   find.
2. The clippy aspect fails on `ultragraphdb_common_frontend` in lines task 8.3 did not write (shadowing in
   `system_link_tasks.rs`, a complex type in `system_link/mod.rs`, a useless conversion in the unit and property tests, a manual
   `div_ceil`, unfulfilled expectations).
3. The frontend-before-storage run sends no resend: the frontend holds a Commit for a partition not yet in Members until it
   joins.
