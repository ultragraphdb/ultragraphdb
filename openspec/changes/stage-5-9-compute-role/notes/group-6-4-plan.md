# Task 6.4: rebinding `ultragraphdb_compute_tests`' reads to the client endpoint, the plan (2026-11-19, base `2127e67b`)

The lead builds a foundation, four workers rebind in parallel, and the lead finishes.

Since the switch, 80 of `ultragraphdb_compute_tests`' 113 scenarios send ComputeRead to a `ComputeSession` that
`played_system.rs`' `serve_compute` serves in the test process. The other 33 already run against the binaries:

- the client endpoint (`endpoint_steps.rs`, `endpoint_read_steps.rs`);
- processes and the system group (`system_steps.rs`, `books_steps.rs`, `bdd.rs`).

The 80 split as follows: 34 REBIND, 45 COVERED, 1 RELAY.

How they were found:

- Each scenario's steps were matched to the step function that binds them.
- A scenario counts as reaching the session when one of its step functions dials `compute_client`, builds a `Scene`, or
  uses a shared cluster (`four()`, `solo()`, `brief()`, `played()`). Five more reach it through a helper:
  `reverse_read` and `key_of_knows`.
- COVERED means a scenario of the same name is bound in `ultragraphdb_tests` or `common_compute`. Its steps were
  compared text for text: 43 are identical and 2 differ, both explained below.
- `common_compute`'s `quad-reads/executor.feature` drives `ReadExecutor` directly, with a client's Read: the endpoint's
  path, not `ComputeSession`'s.

## (a) The map

### REBIND: 34

| Worker | Feature now | Scenario | Steps now |
|---|---|---|---|
| W1 | `quad-reads/compute_reads.feature` | A partition waiting its turn in a merge fails no read | `robust_steps::stalls_behind, robust_steps::went_on` |
| W1 | `quad-reads/compute_reads.feature` | A Scan that stalled before giving a key refuses the read | `robust_steps::stalls_at_once, robust_steps::stalled_read` |
| W1 | `quad-reads/compute_reads.feature` | A corrupt stored record fails its own read alone | `robust_steps::corrupt_record, robust_steps::own_read_alone` |
| W1 | `quad-reads/compute_reads.feature` | A limit asks only its own rows' names | `robust_steps::limit_names, robust_steps::own_names` |
| W1 | `quad-reads/compute_reads.feature` | A client holding no credit holds a frame's worth of names | `robust_steps::no_more_credit, robust_steps::few_frames` |
| W1 | `quad-reads/compute_reads.feature` | Concurrent first reads of a core share one connection per partition | `robust_steps::together, robust_steps::one_connection` |
| W1 | `quad-reads/compute_reads.feature` | A pin refused holds up no other | `robust_steps::pin_refused, robust_steps::pins_go_on` |
| W1 | `wire-protocol/past_credit.feature` | An Entries frame past its credit closes compute's role connection | `robust_steps::past_credit, robust_steps::past_credit_closes` |
| W2 | `quad-reads/compute_reads.feature` | A value-index lookup asks every partition | `lookup_steps::value_index, lookup_steps::three_subjects` |
| W2 | `quad-reads/compute_reads.feature` | An unlimited read that fails restarts | `lookup_steps::restarts, lookup_steps::second_run` |
| W2 | `quad-reads/compute_reads.feature` | A partition's refusal is relayed with its detail | `lookup_steps::relayed_detail, lookup_steps::relays` |
| W2 | `quad-reads/compute_reads.feature` | A frame compute refuses closes the connection | `lookup_steps::refused_frame, lookup_steps::closes` |
| W2 | `quad-reads/compute_reads.feature` | A partition that stops answering fails the read in time | `lookup_steps::hung_partition, lookup_steps::hung_refused` |
| W2 | `quad-reads/compute_reads.feature` | A read past the retention keeps its version | `resume_steps::outlasts_retention, resume_steps::kept_version` |
| W2 | `quad-reads/compute_reads.feature` | A late read keeps its version between Scans | `resume_steps::late_list_scan, resume_steps::late_read_answered` |
| W2 | `quad-reads/compute_reads.feature` | A read at latest level holds no version at first | `resume_steps::short_reads, resume_steps::pins_by_level` |
| W3 | `quad-reads/compute_reads.feature` | A term absent where its list lies answers no row | `read_steps::term_absent, read_steps::absent_no_row` |
| W3 | `quad-reads/compute_reads.feature` | A term and its list take two Scans | `read_steps::two_scans, read_steps::terms_first` |
| W3 | `quad-reads/compute_reads.feature` | A read reads one version of a partition | `read_steps::one_version, read_steps::second_at_version` |
| W3 | `quad-reads/compute_reads.feature` | A list Scan waits for every partition's close | `read_steps::waits_for_close, read_steps::no_list_before_close` |
| W3 | `quad-reads/compute_reads.feature` | A partition the referrer set does not name is not asked | `access_steps::set_names_two, access_steps::not_named` |
| W3 | `quad-reads/compute_reads.feature` | A lagging referrer delays the read, never truncates it | `access_steps::lagging, access_steps::timed_out` |
| W3 | `quad-reads/compute_reads.feature` | A monotonic-fast read does not wait for a lagging partition | `level_steps::monotonic_over_both, level_steps::at_lower` |
| W3 | `quad-reads/transaction_reads.feature` | A scan through compute holds no list in the transaction | `transaction_steps::ten_thousand, transaction_steps::one_range` |
| W3 | `quad-reads/transaction_reads.feature` | A read past the read set's room ends its transaction | `lookup_steps::no_room, lookup_steps::past_room` |
| W3 | `quad-reads/transaction_reads.feature` | A span recorded after a Cancel only widens the commit's check | `transaction_steps::span_after_cancel, transaction_steps::aborts_range_unseen` |
| W4 | `index-maintenance/reverse_count.feature` | A reverse count sums the referrers' partitions | `lookup_steps::reverse_count, lookup_steps::counted` |
| W4 | `index-maintenance/reverse_count.feature` | A reverse count names only what it returns | `robust_steps::counted_named, robust_steps::counted_alone` |
| W4 | `quad-reads/compute_reads.feature` | A later latest-state reverse read of another home is refused | `robust_steps::later_reverse, robust_steps::level_refused` |
| W4 | `quad-reads/compute_reads.feature` | A reverse read at a named version finds an edge whose member stands above it | `robust_steps::named_reverse, robust_steps::returns_edge` |
| W4 | `quad-reads/compute_reads.feature` | A resumed statement read passes a property removed since | `robust_steps::removed_property, robust_steps::third_alone` |
| W4 | `term-dictionary/dangling.feature` | A dangling ID read through compute answers code 17 | `lookup_steps::dangling, lookup_steps::names_list` |
| W4 | `term-dictionary/dangling.feature` | A lookup's dangling ID names its key | `robust_steps::lookup_dangling, robust_steps::names_key` |
| W4 | `term-dictionary/dangling.feature` | A lookup's corrupt dictionary value is no absence | `robust_steps::lookup_corrupt, robust_steps::names_entry` |

### COVERED: 45

| Feature now | Scenario | Bound through the endpoint in |
|---|---|---|
| `quad-reads/compute_reads.feature` | A bound subject asks one partition | `ultragraphdb_tests` quad-reads/spread.feature |
| `quad-reads/compute_reads.feature` | A bound literal object fans out | `ultragraphdb_tests` quad-reads/spread.feature |
| `quad-reads/compute_reads.feature` | An unbound subject answers partition by partition | `common_compute` quad-reads/executor.feature |
| `quad-reads/compute_reads.feature` | A vector object reads the forward lists | `ultragraphdb_tests` quad-reads/spread.feature |
| `quad-reads/compute_reads.feature` | A reifier's skolem IRI reads its statement's properties | `ultragraphdb_tests` quad-reads/access_paths.feature |
| `quad-reads/compute_reads.feature` | A fan-out stays on its read's core | `common_compute` quad-reads/executor.feature |
| `quad-reads/compute_reads.feature` | A reverse read gathers every referrer | `ultragraphdb_tests` quad-reads/spread.feature |
| `quad-reads/compute_reads.feature` | A bound object reads the reverse keyspace | `ultragraphdb_tests` quad-reads/access_paths.feature |
| `quad-reads/compute_reads.feature` | An unbound graph reads every graph | `ultragraphdb_tests` quad-reads/access_paths.feature |
| `quad-reads/compute_reads.feature` | A statement without a statement ID is named by its quad | `ultragraphdb_tests` quad-reads/access_paths.feature |
| `quad-reads/compute_reads.feature` | Parallel edges are told apart by their reifiers | `ultragraphdb_tests` quad-reads/access_paths.feature |
| `quad-reads/compute_reads.feature` | A reverse read names each parallel edge's reifier | `ultragraphdb_tests` quad-reads/access_paths.feature |
| `quad-reads/compute_reads.feature` | An unbound graph is walked by Scans of limit 1 | `ultragraphdb_tests` quad-reads/access_paths.feature |
| `quad-reads/compute_reads.feature` | A read resumes where it stopped | `ultragraphdb_tests` quad-reads/resume_keys.feature |
| `quad-reads/compute_reads.feature` | A resumed read may find nothing | `ultragraphdb_tests` quad-reads/resume_keys.feature |
| `quad-reads/compute_reads.feature` | A resume key with another pattern is refused | `ultragraphdb_tests` quad-reads/resume_keys.feature |
| `quad-reads/compute_reads.feature` | A resume key holds terms, not keys | `ultragraphdb_tests` quad-reads/access_paths.feature |
| `quad-reads/compute_reads.feature` | The empty result | `ultragraphdb_tests` quad-reads/resume_keys.feature |
| `quad-reads/compute_reads.feature` | A resume key of another transaction is refused | `ultragraphdb_tests` quad-reads/resume_keys.feature |
| `quad-reads/compute_reads.feature` | A reader at a named version resumes after a reconnect | `ultragraphdb_tests` quad-reads/resume_keys.feature |
| `quad-reads/compute_reads.feature` | A resumed version no longer retained is refused | `ultragraphdb_tests` quad-reads/retention.feature |
| `quad-reads/compute_reads.feature` | A limit stopping inside one object's statements resumes there | `ultragraphdb_tests` quad-reads/resume_keys.feature |
| `quad-reads/compute_reads.feature` | A limit stopping after an object with no statement ID skips it whole | `ultragraphdb_tests` quad-reads/resume_keys.feature |
| `quad-reads/compute_reads.feature` | A limit stopping inside a statement's record resumes there | `ultragraphdb_tests` quad-reads/resume_keys.feature |
| `quad-reads/compute_reads.feature` | Only the last batch holds a key | `ultragraphdb_tests` quad-reads/resume_keys.feature |
| `quad-reads/compute_reads.feature` | A read after a commit sees it through its token | `ultragraphdb_tests` quad-reads/levels.feature |
| `quad-reads/compute_reads.feature` | Two reads of one transaction share its snapshot | `ultragraphdb_tests` quad-reads/levels.feature |
| `quad-reads/compute_reads.feature` | Monotonic-fast reads never go back | `ultragraphdb_tests` quad-reads/levels.feature |
| `quad-reads/compute_reads.feature` | A monotonic-fast read never answers below the token | `ultragraphdb_tests` quad-reads/levels.feature |
| `quad-reads/compute_reads.feature` | A latest-state read across partitions is refused | `ultragraphdb_tests` quad-reads/spread.feature |
| `quad-reads/compute_reads.feature` | A batch stops at the client's frame | `common_compute` quad-reads/executor.feature |
| `quad-reads/compute_reads.feature` | A row too large for the client's frame is refused | `common_compute` quad-reads/executor.feature |
| `quad-reads/compute_reads.feature` | A resume key that does not fit goes alone | `common_compute` quad-reads/executor.feature |
| `quad-reads/transaction_reads.feature` | A read sees its transaction's own write | `ultragraphdb_tests` quad-reads/transaction_reads.feature |
| `quad-reads/transaction_reads.feature` | A read sees its transaction's new term and its emptied list | `ultragraphdb_tests` quad-reads/client_order.feature |
| `quad-reads/transaction_reads.feature` | A term nobody holds conflicts with its later statement through compute | `ultragraphdb_tests` quad-reads/transaction_reads.feature |
| `quad-reads/transaction_reads.feature` | A scanned range conflicts with an insert into it through compute | `ultragraphdb_tests` quad-reads/transaction_reads.feature |
| `quad-reads/transaction_reads.feature` | A reverse range conflicts with an edge whose reverse entry lands in it | `ultragraphdb_tests` quad-reads/transaction_reads.feature |
| `quad-reads/transaction_reads.feature` | A range stopped at its limit conflicts only with what it returned | `ultragraphdb_tests` quad-reads/transaction_reads.feature |
| `quad-reads/transaction_reads.feature` | A limit stopping before a graph leaves that graph unread | `ultragraphdb_tests` quad-reads/client_order.feature |
| `quad-reads/transaction_reads.feature` | Resuming records the rest | `ultragraphdb_tests` quad-reads/transaction_reads.feature |
| `quad-reads/transaction_reads.feature` | A statement's record read conflicts with a property written | `ultragraphdb_tests` quad-reads/transaction_reads.feature |
| `quad-reads/transaction_reads.feature` | A graph appearing in a gap conflicts | `ultragraphdb_tests` quad-reads/client_order.feature |
| `system-identity/resume_keys.feature` | A resume key carries terms, not keys | `ultragraphdb_tests` system-identity/results.feature |
| `system-identity/resume_keys.feature` | A tampered resume key is refused | `ultragraphdb_tests` system-identity/results.feature |

### RELAY: 1

| Feature now | Scenario | Steps now |
|---|---|---|
| `quad-reads/compute_reads.feature` | A read's stream starts with the frontend's credit | `batch_steps::starting_credit, batch_steps::four_then_one` |

Notes on the map:

- **RELAY.** The delta's quad-reads requirement "A read's batches fit the client's frame" replaces "A read's stream
  starts with the frontend's credit" with "A read's stream starts with the client's credit". That scenario is bound in
  `common_compute`'s `quad-reads/executor.feature`, so this one goes.
- **"A bound literal object fans out".**
  - The delta's text ends "returns `<a>`, partition 1's row, before `<c>`, partition 2's". `ultragraphdb_compute_tests`
    holds that text.
  - `ultragraphdb_tests`' `quad-reads/spread.feature` still holds the base text, "merged in key order".
  - It counts as COVERED. The lead restates `spread.feature`'s scenario and its Then step to the delta's text in step
    (d), so the binding that stays holds the spec's text.
- **"A fan-out stays on its read's core".** `common_compute` holds the delta's text ("the core its client's connection
  arrived on"); `ultragraphdb_compute_tests` holds the base text. It counts as COVERED and needs no restatement.
- **No requirement.** 17 REBIND scenarios have no requirement of their own: they are corner cases carried from earlier
  stages. They exist only here, so they are rebound, not dropped.

## (b) The foundation, built by the lead before the workers start: about 2.5 hours

The workers need one cluster shape, one client layer and fixed files. All of it lives in files no worker edits.

### F1. Each shared cluster runs the compute binary (about 45 minutes)

- `cluster.rs`' `start_cluster` and `bdd.rs`' `start_played_cluster` start the compute binary through `processes.rs`.
  Before `2127e67b` they did, with a role address. Now they give it:
  - `cluster.system_address` = the played group at `base + 1`, whose Members lists the tapped partitions, so `tap.rs`
    stays between compute and the partitions as before;
  - an `[endpoint]` table: clear text, at a port of the block;
  - the cluster's extra settings: `scan_stall_ms`, `read_retain_ms`.
- Its log goes to `scratch("compute-{base}.log")`, the file the steps read before the switch.
- `played_system.rs`' `PlayedConnection` hands every command it does not play itself to a `SystemPlay` from
  `common_compute`'s `test-utils`. `SystemPlay` answers Challenge, Verify, Session, Check, Ended, Declare and Defaults
  for an endpoint, and holds the bootstrap principal.
  - Members, Placement, Subscribe, CloseRequest, Catalog and Lease stay as `PlayedConnection` plays them.
- Removed:
  - `ServedCompute`, `serve_compute` and the `compute` field of `Played`;
  - the in-process log (`IN_PROCESS_LOG` and `main`'s subscriber) and the `tracing-subscriber` dev-dependency, with
    `Cargo.lock`.
  - These were listed under (d); they go here because nothing after F1 uses them.

### F2. A client layer in `scene.rs` (about 60 minutes)

- An `EndpointClient` over the compute binary's client endpoint, at the frame level, as the existing step helpers are.
  It does Hello, logs in as the bootstrap administrator over SCRAM, and selects the tenant.
- It keeps the shape the steps already call:
  - `answer_on(client, read)`, `next_of`, `next_batch`, `send_only`, `send_on` (Credit, Cancel) and `last_stream`;
  - `ReadAnswer`, `Batch` and `Refused` keep their fields.
- `Scene`'s builders `snapshot`, `snapshot_at`, `read_of` and `answer` build a client Read inside a read-only
  transaction Begun at the level asked:
  - `snapshot_at(epoch)` begins once the played group's close has reached `epoch`;
  - a token read Begins with the token.
- Read-write reads Begin a read-write transaction at compute. `Scene::begin(partition, identity)`, which opened one at a
  partition directly, gives way to a client transaction whose writes and Commit go through compute. Check and Ended are
  answered by F1's `SystemPlay`.
- `compute_client` is removed. Each worker turns its steps from `RoleClient` to `EndpointClient`.

### F3. The files the workers own (about 30 minutes)

- The 45 COVERED and the 1 RELAY scenario are removed from their feature files now, not after the workers. Under F1
  their steps fail, and leaving them would put dead steps in the workers' files. Their step functions go with them, and
  so do helpers nothing else calls.
  - `batch_steps.rs` binds nothing then.
  - `system-identity/resume_keys.feature` holds nothing then: both its scenarios are COVERED.
  - Both are removed by `git rm`, under the approved removal of "the features of `ultragraphdb_compute_tests` that
    drove reads by ComputeRead".
- `quad-reads/compute_reads.feature`'s 25 REBIND scenarios are split by worker. Each keeps its text and its `Rule:`
  heading.
  - W3's stay in `compute_reads.feature`.
  - W1's go to `quad-reads/read_faults.feature`.
  - W2's go to `quad-reads/lookups.feature`.
  - W4's go to `quad-reads/reverse_versions.feature`.
  - This way no two workers run one target, and so no two share a port block.
- Step functions move so each feature file's steps sit in one worker's files:
  - from `robust_steps.rs` to a new `reverse_dictionary_steps.rs`: `counted_named`/`counted_alone`,
    `lookup_dangling`/`names_key`, `lookup_corrupt`/`names_entry`, `later_reverse`/`level_refused`,
    `named_reverse`/`returns_edge`, `removed_property`/`third_alone`;
  - from `lookup_steps.rs` to `reverse_dictionary_steps.rs`: `reverse_count`/`counted` and `dangling`/`names_list`;
  - from `lookup_steps.rs` to `transaction_steps.rs`: `no_room`/`past_room`.
- Helpers that more than one worker's files call move to `scene.rs`. `scene.rs`, `bdd.rs`, `cluster.rs`,
  `played_system.rs`, `processes.rs`, `tap.rs`, `played_partition.rs` and `BUILD.bazel` are frozen during the worker
  phase. A worker needing a change there asks the lead.
- After F3, `features/` holds 21 files. The targets take their port blocks in `glob` order, and no file is added or
  removed until the workers return, so every block below is fixed.

### F4. The foundation's gate (about 15 minutes)

- `bazel build //ultragraphdb/ultragraphdb_compute_tests/...` and the Clippy aspect pass.
- The 13 targets of the 33 scenarios already on the binaries pass.
- The 34 REBIND scenarios are expected red until their worker returns.
- The foundation is snapshotted as a commit message for the user before the workers branch from it.

## (c) The workers

Each worker runs in a git worktree branched from the foundation, owns its files outright, and runs only its own
targets. Running in worktrees means:

- no worker's half-done edit breaks another's compile, since every target compiles all of `tests/`;
- each worktree has its own Bazel server, and each worker's targets hold blocks no other worker's do.

The cost is a cold build per worktree, about 10 to 15 minutes each.

Every worker follows these rules:

- Scenario texts stay as they are.
- A scenario that cannot be driven through the client endpoint with its text unchanged is reported, not rewritten. An
  example is a read that must name an epoch a client cannot name.
- Tests return `TestResult`: no unwrap, expect or panic, no `#[allow]`/`#[expect]`, no sleeps as fixes, no retries.
- No commit, stash, reset or checkout.
- Gate: `bazel build` of the package, the Clippy aspect over it, and the worker's targets passing.
- Report the files changed and each scenario green.

Ports: each worker runs only its targets. A target's block is 200 ports from `12000 + (46 + index) × 200`, from
`bazel/test_ports.bzl`; the package holds blocks 46 to 67.

### W1: reads under faults (8 scenarios)

- **Files:** `tests/robust_steps.rs` (after F3), `features/quad-reads/read_faults.feature`,
  `features/wire-protocol/past_credit.feature`.
- **Scenarios:**
  - "A partition waiting its turn in a merge fails no read"
  - "A Scan that stalled before giving a key refuses the read"
  - "A corrupt stored record fails its own read alone"
  - "A limit asks only its own rows' names"
  - "A client holding no credit holds a frame's worth of names"
  - "Concurrent first reads of a core share one connection per partition"
  - "A pin refused holds up no other"
  - "An Entries frame past its credit closes compute's role connection": reads its refusal from `compute-{base}.log`,
    the compute process's log, again.
- **Targets and ports:**
  - `bdd_quad_reads_read_faults`: index 8, ports 22800 to 22999;
  - `bdd_wire_protocol_past_credit`: index 19, ports 25000 to 25199.

### W2: lookups and retention (8 scenarios)

- **Files:** `tests/lookup_steps.rs`, `tests/resume_steps.rs` (after F3), `features/quad-reads/lookups.feature`.
- **Scenarios:**
  - "A value-index lookup asks every partition"
  - "An unlimited read that fails restarts"
  - "A partition's refusal is relayed with its detail"
  - "A frame compute refuses closes the connection"
  - "A partition that stops answering fails the read in time"
  - "A read past the retention keeps its version"
  - "A late read keeps its version between Scans"
  - "A read at latest level holds no version at first"
- **Target and ports:** `bdd_quad_reads_lookups`: index 6, ports 22400 to 22599.

### W3: access paths, levels and transaction reads (10 scenarios)

- **Files:** `tests/read_steps.rs`, `tests/access_steps.rs`, `tests/level_steps.rs`, `tests/transaction_steps.rs`
  (after F3), `features/quad-reads/compute_reads.feature`, `features/quad-reads/transaction_reads.feature`.
- **Scenarios:**
  - "A term absent where its list lies answers no row"
  - "A term and its list take two Scans"
  - "A read reads one version of a partition"
  - "A list Scan waits for every partition's close"
  - "A partition the referrer set does not name is not asked"
  - "A lagging referrer delays the read, never truncates it"
  - "A monotonic-fast read does not wait for a lagging partition"
  - "A scan through compute holds no list in the transaction"
  - "A read past the read set's room ends its transaction"
  - "A span recorded after a Cancel only widens the commit's check"
- **Targets and ports:**
  - `bdd_quad_reads_compute_reads`: index 5, ports 22200 to 22399;
  - `bdd_quad_reads_transaction_reads`: index 10, ports 23200 to 23399.

### W4: reverse reads and the dictionary (8 scenarios)

- **Files:** `tests/reverse_dictionary_steps.rs` (made in F3), `features/index-maintenance/reverse_count.feature`,
  `features/quad-reads/reverse_versions.feature`, `features/term-dictionary/dangling.feature`.
- **Scenarios:**
  - "A reverse count sums the referrers' partitions"
  - "A reverse count names only what it returns"
  - "A later latest-state reverse read of another home is refused"
  - "A reverse read at a named version finds an edge whose member stands above it"
  - "A resumed statement read passes a property removed since"
  - "A dangling ID read through compute answers code 17"
  - "A lookup's dangling ID names its key"
  - "A lookup's corrupt dictionary value is no absence"
- **Targets and ports:**
  - `bdd_index_maintenance_reverse_count`: index 3, ports 21800 to 21999;
  - `bdd_quad_reads_reverse_versions`: index 9, ports 23000 to 23199;
  - `bdd_term_dictionary_dangling`: index 17, ports 24600 to 24799.

## (d) After the workers return

1. Merge the four worktrees. Their files are disjoint, so each merge is a copy, and the merged package builds and passes
   its Clippy aspect.
2. Restate `ultragraphdb_tests`' "A bound literal object fans out" and its Then step to the delta's text.
3. Remove `ComputeSession`: its role session and its ComputeRead handling, `ReadRequest::of` over a `ComputeReadView`
   included. This step depends on question 1 below: `common_compute`'s own suites hold it.
4. Update the tasks.md 6.4 line with the three accepted decisions:
   - the in-process `ComputeSession` was the interim until this rebinding;
   - `joining.rs`' Members and liveness asks stay, without the Join;
   - `system_group.feature` was removed and `played_frontend.rs` moved to `played_system.rs`. `CoreListener` stays.
5. Gate: `make build`, the Clippy aspect over the changed packages, `rustfmt --check`, then the integration groups in
   `make test`'s order: config, storage, compute, supervisor, cluster. Counts before and after go into
   `notes/group-6.md`.

## Questions for the user

1. **`ComputeSession` in `common_compute`'s own suites.**
   - About 30 test files of `common_compute` drive reads through `ComputeSession`: up to 168 unit and property tests
     over the played clusters `tests/played`, `tests/played_reads`, `tests/utils/played_cluster.rs`, `ApartCluster::compute`
     and `read_executor_property_tests`. `compute_session_tests.rs` among them tests the session itself.
   - The compute binary no longer uses `ComputeSession`; only tests do.
   - Removing it, as approved, means these suites move to `ReadExecutor` driven directly, as `executor_steps.rs` already
     does. That is a wave of its own, about the size of this one.
   - The other choice is to keep `ComputeSession` as a test double in `common_compute`'s `utils_test`, behind
     `test-utils`, off the library's surface, and remove it when those suites move.
   - Which?
2. **Worktrees.** The plan runs the workers in git worktrees, each with a cold Bazel build of about 10 to 15 minutes.
   The other choice is the shared tree, where the Bazel server serializes the workers' runs and one worker's broken
   compile stops the other three. Worktrees are recommended. Agreed?
3. **Removals in the foundation.** F3 removes `batch_steps.rs` and `system-identity/resume_keys.feature` by `git rm`,
   once their scenarios are COVERED or RELAY. Both are under the approved removal of the ComputeRead features; this
   confirms it reaches whole files.

## As built at F4 (2026-11-19)

This section supersedes (b) and (c) where they differ.

### What the foundation found

- **7 REBIND scenarios reach no `ComputeSession`.** Their steps drive `ComputeCore`'s lookups in the test process, over
  the shared clusters' partitions. No client surface carries them, so they stay as they are and pass unchanged:
  - "A value-index lookup asks every partition"
  - "A reverse count sums the referrers' partitions"
  - "A reverse count names only what it returns"
  - "A lookup's dangling ID names its key"
  - "A lookup's corrupt dictionary value is no absence"
  - "Concurrent first reads of a core share one connection per partition"
  - "A pin refused holds up no other"
- That leaves 27 to rebind. The foundation rebound "A term absent where its list lies answers no row" itself, as the
  worked example of the client layer, so the workers rebind 26.

### F1: the cluster

- `cluster.rs`' `start_compute` starts the compute binary for every shared cluster, `played()`'s included:
  - client endpoint at the cluster's base + 20, in clear text;
  - observability at base + 21;
  - `cluster.system_address` the tapped played group at base + 1;
  - log at `scratch("compute-{base}.log")`, which `ClusterInfo::compute_log` names.
- `played_system.rs` answers Challenge, Verify, Session, Check (each passed) and Ended, as the group serves compute. The
  administrator, `endpoint_steps::ADMIN` with its `PASSWORD`, has principal `scene::PRINCIPAL`; an unknown name gets an
  unknown name's verifier.
- `SystemPlay` was not used: its frames are built by a crate-private constructor. **Nothing in `common_compute`
  changed.**
- Removed: `serve_compute`, `ServedCompute`, `Played::compute`, the in-process log and the `tracing-subscriber`
  dev-dependency (with `Cargo.lock`).

### F2: the client layer, `tests/client.rs`

- `ClientRead` has public fields: `begin`, `limit`, `terms`, `pattern`, `resume`, `max_frame`.
- Constructors: `read_at(scene, pattern, read_only, level)`, `snapshot_at(scene, pattern, epoch)` (a read-only Begin
  naming the version bounding `epoch`) and `snapshot(scene, pattern)`.
- `answer(scene, read)` answers on a connection of its own.
- `EndpointClient` connects with a Hello naming `max_frame` and logs in as the administrator. Its methods:
  - `stream`, `send_on`, `send_only` (returns the stream);
  - `next_frame(what)`;
  - `begin(&Begin)` (returns the transaction's stream, or compute's refusal);
  - `send_read(transaction, read)`.
- `answer_on(client, read)` grants a Credit of 1 per batch. `next_of(client)` returns a batch or a refusal.
- The old ComputeRead layer in `scene.rs` (`compute_client`, `answer_on`, `next_of`, `next_batch`, the `Scene` read
  builders, `resumed`) stays only so unconverted step files compile. The lead removes it in (d).

### F3: files

- **Removed:** the 45 COVERED and the 1 RELAY scenario, with 79 step functions only they used and the helpers, imports,
  fields and variants that died with them. By `git rm`: `tests/batch_steps.rs` and
  `features/system-identity/resume_keys.feature`.
- **Split:** `compute_reads.feature` into `compute_reads.feature` (W3), `lookups.feature` (W2), `read_faults.feature`
  (W1) and `referrers_and_versions.feature` (W4), each with its scenarios' `Rule:` headings and texts unchanged.
- **Step moves:**
  - `robust_steps.rs` to new `reverse_version_steps.rs`: `later_reverse`/`level_refused`, `named_reverse`/`returns_edge`,
    `removed_property`/`third_alone`, `property_removal`.
  - `resume_steps.rs` to `reverse_version_steps.rs`: `resumed`.
  - `lookup_steps.rs` to `transaction_steps.rs`: `no_room`/`past_room`.
- **Shared helpers to `scene.rs`:** `edge_with`, `reads`, `until`, `large_list`, `padded`.

### The foundation's gate

- `make build` passes, and the Clippy aspect over `//ultragraphdb/ultragraphdb_compute_tests/...` is clean.
- The compute group by tag: 14 of 21 targets pass. The 7 that fail fail on exactly the 26 scenarios the workers rebind;
  every other scenario passes, the worked example and the 7 in-process lookups among them.

### Port blocks after F3

| Index | Port base | Feature file | Worker |
|---|---|---|---|
| 3 | 21800 | `index-maintenance/reverse_count.feature` | none: in-process lookups |
| 5 | 22200 | `quad-reads/compute_reads.feature` | W3 |
| 6 | 22400 | `quad-reads/lookups.feature` | W2 |
| 8 | 22800 | `quad-reads/read_faults.feature` | W1 |
| 9 | 23000 | `quad-reads/referrers_and_versions.feature` | W4 |
| 10 | 23200 | `quad-reads/transaction_reads.feature` | W3 |
| 17 | 24600 | `term-dictionary/dangling.feature` | W2 |
| 19 | 25000 | `wire-protocol/past_credit.feature` | W1 |
