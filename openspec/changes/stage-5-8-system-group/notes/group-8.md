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
