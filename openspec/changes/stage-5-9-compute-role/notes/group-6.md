# Group 6: the switch to two roles

## Task 6.1: the frontend's scenarios moved into `ultragraphdb_compute_tests` (2026-11-19, base `ed6987b8`)

The scenarios of `ultragraphdb_frontend_tests`, their steps and their harness live in `ultragraphdb_compute_tests`, under
that crate's names, and run against the frontend binary as before. Scenario text is unchanged: what names the frontend
binary is restated for compute in the switch's commit.

### Files

Moved by `git mv`, each with its history:

| Before (`ultragraphdb_frontend_tests/`) | After (`ultragraphdb_compute_tests/`) |
|---|---|
| `features/client-endpoint/system_group.feature` | `features/client-endpoint/system_group.feature` |
| `features/client-endpoint/tls.feature` | `features/client-endpoint/tls.feature` |
| `features/config-service/parent.feature` | `features/config-service/parent.feature` |
| `features/metadata-group/bootstrap.feature` | `features/metadata-group/bootstrap.feature` |
| `features/quad-reads/order.feature` | `features/quad-reads/order.feature` |
| `features/role_binaries/role_binaries.feature` | `features/role-binaries/endpoint_process.feature` (its name was compute's) |
| `features/role_binaries/unready.feature` | `features/role-binaries/unready.feature` |
| `features/schema-changes/tightening.feature` | `features/schema-changes/tightening.feature` |
| `features/system-group/lost_group.feature` | `features/system-group/lost_group.feature` |
| `features/system-identity/result_terms.feature` | `features/system-identity/result_terms.feature` |
| `features/transactions/sessions.feature` | `features/transactions/sessions.feature` |
| `features/wire-protocol/result_terms.feature` | `features/wire-protocol/result_terms.feature` |
| `tests/books_steps.rs` | `tests/books_steps.rs` |
| `tests/endpoint_steps.rs` | `tests/endpoint_steps.rs` |
| `tests/processes.rs` | `tests/processes.rs` |
| `tests/role_read_steps.rs` | `tests/endpoint_read_steps.rs` |

Compute's own `features/role_binaries/role_binaries.feature` moved to `features/role-binaries/role_binaries.feature`, the
capability's name; its target's name is unchanged.

Folded into compute's files, then removed by `git rm`:

- `tests/bdd.rs` into compute's `tests/bdd.rs`: one `World`, one `main`, one `services()`; the frontend's role-binaries and
  config-service steps beside compute's; the before-hook that waits for the target's own ports now runs before every
  scenario.
- `tests/system_steps.rs` into compute's `tests/system_steps.rs`.
- `Cargo.toml` and `BUILD.bazel`: the dependencies joined into compute's manifest, `ultragraphdb_common_compute` with
  `test-utils`; the genquery `frontend_deps` added to compute's build file. The crate's directories, empty after the moves,
  went with them, since the workspace takes every folder under `ultragraphdb/` as a member.

What the two harnesses both did, now done once:

- `signal`: `processes::signal`, which returns an error where compute's asserted.
- The second storage process, partition 2 hosting no group, at offsets 5 and 16: compute's `not_hosting_partition_2` and
  its two offsets are `processes::second_of`, `second_role_address` and `second_observability`.
- "it names neither `ultragraphdb_store_kv` nor fjall": one step.
- "the hosting storage process is killed and does not return within `join_deadline_ms`" was a step of each crate, compute's
  starting compute alone, the frontend's the frontend alone. It now starts the storage process hosting the group, the
  frontend and compute, each with a deadline of 800 ms, kills the host and holds how each ended
  (`Steps::dependants`); each scenario's Then judges its own role, by the harness's system address. Compute's scenario
  dialled a group at offset 4 of its own, and now dials the harness's at offset 1.
- `Roles` gained `start_compute`.

The clusters compute's read scenarios share now start on their first use (`shared`), where `main` started all four before
any scenario: in the merged crate every target would otherwise start four clusters, the twelve moved ones included, none of
which reads them. The getters return a `TestResult` in place of aborting the process; their 111 call sites take a `?`.

Renamed without a change of behaviour: the administrator client's name `ultragraphdb-frontend-tests` to
`ultragraphdb-compute-tests`; in `endpoint_steps.rs` a variable named `frontend` that held the system group's address, now
`system`; comments naming the endpoint.

### Targets and ports

| Before | After |
|---|---|
| `ultragraphdb_frontend_tests:bdd_client_endpoint_system_group` | `ultragraphdb_compute_tests:bdd_client_endpoint_system_group` |
| `ultragraphdb_frontend_tests:bdd_client_endpoint_tls` | `ultragraphdb_compute_tests:bdd_client_endpoint_tls` |
| `ultragraphdb_frontend_tests:bdd_config_service_parent` | `ultragraphdb_compute_tests:bdd_config_service_parent` |
| `ultragraphdb_frontend_tests:bdd_metadata_group_bootstrap` | `ultragraphdb_compute_tests:bdd_metadata_group_bootstrap` |
| `ultragraphdb_frontend_tests:bdd_quad_reads_order` | `ultragraphdb_compute_tests:bdd_quad_reads_order` |
| `ultragraphdb_frontend_tests:bdd_role_binaries_role_binaries` | `ultragraphdb_compute_tests:bdd_role_binaries_endpoint_process` |
| `ultragraphdb_frontend_tests:bdd_role_binaries_unready` | `ultragraphdb_compute_tests:bdd_role_binaries_unready` |
| `ultragraphdb_frontend_tests:bdd_schema_changes_tightening` | `ultragraphdb_compute_tests:bdd_schema_changes_tightening` |
| `ultragraphdb_frontend_tests:bdd_system_group_lost_group` | `ultragraphdb_compute_tests:bdd_system_group_lost_group` |
| `ultragraphdb_frontend_tests:bdd_system_identity_result_terms` | `ultragraphdb_compute_tests:bdd_system_identity_result_terms` |
| `ultragraphdb_frontend_tests:bdd_transactions_sessions` | `ultragraphdb_compute_tests:bdd_transactions_sessions` |
| `ultragraphdb_frontend_tests:bdd_wire_protocol_result_terms` | `ultragraphdb_compute_tests:bdd_wire_protocol_result_terms` |

Every target carries `integration_test` and `compute-integration-test`. `bazel/test_ports.bzl` gives
`ultragraphdb_compute_tests` blocks 46 to 67, the frontend's 46 to 59 and compute's 60 to 67 joined; 20 are taken.
`scripts/test.sh` runs no `frontend` group: no target carries its tag, and Bazel fails a `--test_tag_filters` that selects
no test.

### Counts

| | Before | After |
|---|---|---|
| `ultragraphdb_frontend_tests`, 12 targets | 30 scenarios, 60 steps | none |
| `ultragraphdb_compute_tests`, 8 targets | 90 scenarios, 180 steps | |
| `ultragraphdb_compute_tests`, 20 targets | | 120 scenarios, 240 steps |

Each moved target runs the scenarios and steps it ran before. Times, before (cached) and after one run of the compute
group, 20 targets side by side: compute's targets shorter by their clusters' start (`role_binaries` 19.6 s to 13.4 s,
`system_group` 15.7 s to 5.8 s, `compute_reads` 21.4 s to 16.4 s); `lost_group` 10.7 s to 16.7 s, its folded scenario
starting compute beside the frontend.

### Elsewhere

- `AGENTS.md`: the `ultragraphdb_frontend_tests` row removed, `ultragraphdb_compute_tests`' row joined, `make test`'s
  groups without `frontend-`.
- `ultragraphdb_storage_tests`' `features/system-group/lost_group.feature`: its description names where the other roles'
  parts are bound.

### The gate

`make build` passes; the Clippy aspect over `//ultragraphdb/ultragraphdb_compute_tests/...` is clean; `rustfmt --check` is
clean; `bazel test //... --test_tag_filters=compute-integration-test` passes 20 of 20; the five storage targets reading the
edited feature pass.

### Found, not changed

- `ultragraphdb_compute_tests` still names the frontend binary where it starts it until the switch: the copy target
  `copy_frontend`, the genquery `frontend_deps`, `FRONTEND_BINARY`, `Roles::start_frontend` and the role name `frontend`
  its role connections authenticate as.
- `played_frontend.rs` plays the system group for the shared clusters; its name is the relay's.
- `endpoint_steps.rs`' TLS step plays a compute process that joins the group by its role address, which no read reaches
  since group 5: task 6.4's.
- `AGENTS.md`'s *Service binaries* says each binary has a sister `_tests` crate; the frontend binary has none now, until
  task 6.5 removes it.

## Checkpoint B: tasks 6.2, 6.3, 6.6 and 6.7, and task 6.4 in part (2026-11-19, base `998b4149`)

The switch: the compute binary runs the client endpoint and the executor, listens at no role address, authenticates to
the system group as `compute`, and sends no Join (C6.28). The role names are `compute` and `storage` alone.

### Phase 2: the suite failing for the stated reason

Each target was run once against the base, its new and restated tests in place. Each failure was read: a retired name
accepted, a role table or Roles check of three roles, or the server's clock seam not yet there
(`unimplemented!("a server on another clock")`). No test failed for another reason. The logs are kept in the session's
scratchpad, `phase2/`.

| Target | Failed of run |
|---|---|
| `common_config_check:bdd` | 8 of 27 scenarios |
| `common_config_check:property` | 5 of 8 |
| `common_config_check` unit | 21 of 66 |
| `common_role:property` | 1 of 3 (L6.5) |
| `common_role` unit | 4 of 71, 2 of them at the clock seam (C6.20, task 6.6) |
| `common_service` unit | 1 of 74 (compute's readiness binding) |
| `common_service_config` unit | 1 of 31 (`[frontend]` read as a table) |
| `common_system:bdd` | 10 of 52 scenarios |
| `common_system:property` | 3 of 9 (L6.4 among them) |
| `common_system` unit | 10 of 105 |

After phase 4 every one of these targets passes with the same counts.

### Phase 4: what the switch changed

- `common_service_config`: `TABLES` has no `frontend`, so a `[frontend]` table is an unknown table. `ComputeTable` reads
  no setting: `compute.role_address` is an unread key.
- `common_config_check`:
  - The roles are `compute` and `storage`.
  - The Roles check wants at least one instance of each.
  - The new Tables check refuses `[compute]` or `[endpoint]` in a storage file and `[system]` in a compute file.
    `[storage]` in a compute file is accepted, since compute reads `storage.scan_stall_ms` and `storage.read_retain_ms`;
    L6.3 and C6.9 were corrected to say so.
  - Compute's listener is `endpoint.address`, and only storage has a role address.
- `protocol_wire`: a Subscribe with `latest` 1 is accepted from compute alone.
- `common_system`:
  - The endpoint table by role (`PROTOCOL.md` §11.12a) gives compute every command the frontend sent.
  - `role_named` knows `compute` and `storage` alone.
  - A CloseRequest of partition 0 is compute's.
- `common_role`:
  - `ROLE_NAMES` is `compute` and `storage`.
  - The `ServeClock` trait and `WallClock` are new.
  - `Server<K = WallClock>` reads and waits on its clock, set by `with_clock` (task 6.6).
- `common_service`: compute's readiness binds as the frontend's did.
- `common_compute`:
  - `joining.rs` sends no Join. `find_partitions` and `hold_peers` keep the Members and liveness asks behind "A lost
    dependency exits with its code".
  - `SystemLink` dials the group as `compute`. The suite found this one: the link still authenticated as `frontend`,
    which the group now refuses.
- `ultragraphdb_compute`:
  - `compute_role` is the frontend's service under compute's names.
  - It stops after the endpoint's `transaction_ms`, not a fixed 50 s.
  - It watches its peers for a lost dependency.
- `ultragraphdb_supervisor`: the start order is storage, then compute; the stop order is compute, storage, then the
  config service. The development image holds two role binaries.
- Profiles: `dev` and `dev-net` give compute an `[endpoint]` table.
- Chart:
  - `computeInstances` gives one StatefulSet per instance, each of one replica with no volume.
  - The headless Service is `compute`, and the Service `compute-client` spans every compute pod (C6.36).
  - `scripts/cluster_local.sh` and `scripts/bench_cluster.sh` follow.
- Harnesses: `ultragraphdb_tests`, `ultragraphdb_compute_tests`, `ultragraphdb_storage_tests`,
  `ultragraphdb_supervisor_tests`, `ultragraphdb_config_tests` and `ultragraphdb_chart_tests` start, name and configure
  two roles. No target copies the frontend binary.

### Scenario texts restated

- 90 scenarios across 12 crates take their delta's text. The step strings that bind them changed with them.
- 2 more take the delta's text from another capability's file: "Compute serves clients on every core" and "Storage is
  ready before any compute node exists".
- 25 keyword lines with no delta of their own name compute where they named the frontend: 11 `Feature:` titles, 4
  beyond-spec scenarios of `ultragraphdb_tests`' failure features and `common_compute`'s tightening, and 10 of their
  steps.
- "A crash before the close leaves the version pending" read "the frontend is killed". The spec says "the system
  group's host is killed", and the scenario now does too; this drift predates the change.
- "A stall past the client's read timeout is refused" takes the delta's name, "A stall past what the client's read
  timeout covers is refused".

### Removed and moved

By `git rm`, approved for task 6.5 and done in the switch, since the switch makes them false:

- `ultragraphdb/ultragraphdb_config/profiles/dev/frontend.toml`
- `ultragraphdb/ultragraphdb_config/profiles/dev-net/frontend.toml`
- `deploy/helm/ultragraphdb/templates/frontend.yaml`
- `deploy/helm/ultragraphdb/profile/kube/frontend.toml`

The development image's frontend layer, the chart's frontend image value and every `copy_frontend` target are dropped
by edit.

Also by `git rm`: `ultragraphdb/ultragraphdb_compute_tests/features/system-group/system_group.feature`.

- Its "A group gone past the deadline ends its dependants" lives on in `lost_group.feature`, under the delta's text.
- Its "A compute process joins a restarted system group at once" is a Join, which task 3.7 brings back.

From `ultragraphdb_compute_tests`' `role_binaries.feature`, 4 scenarios are removed:

- "The compute binary carries no storage engine" is merged into `endpoint_process.feature`'s "The compute binary carries
  no storage engine and no book".
- "A compute process without its address stops the start": the role address is retired, and "Compute without the
  system group's address stops the start" stands in its place.
- "Compute refuses a role with another secret": compute has no role listener.
- "Compute serves reads alone": a ComputeRead, which task 6.4 removes.

By `git mv`: `ultragraphdb/ultragraphdb_compute_tests/tests/played_frontend.rs` to `tests/played_system.rs`. It plays the
system group and, beside it, compute's role session in process.

### Task 6.4, in part

The compute binary serves no ComputeRead, so `ultragraphdb_compute_tests`' read features run against a `ComputeSession`
served in process beside the played system group (`played_system.rs`, `serve_compute`). The binary is no longer under
test there. What those sessions log goes to one file per test process, `compute-in-process.log`, through a
`tracing-subscriber` dev-dependency. "An Entries frame past its credit closes compute's role connection" reads its
refusal there, where it read the compute process's log before.

Removing `ComputeSession` needs each of those read features bound to the client endpoint first, with its scenarios
unchanged. Of them, 37 scenarios exist only in `ultragraphdb_compute_tests`; the rest exist already through the endpoint
in `ultragraphdb_tests` or `common_compute`. That rebinding is a commit of its own and is not done.

`CoreListener` stays: the client endpoint listens through it. The approved removal does not apply to it.

`joining.rs`' asks stay, without the Join, because the unchanged role-binaries requirement "A lost dependency exits with
its code" depends on them. That is against the approval's wording ("compute's Join by its role address with
`joining.rs`' asks"), and is the user's to decide.

### Laws

- L6.1, L6.2 and L6.3 are in `common_config_check`'s property suite.
- L6.4 is in `common_system`'s, over the commands the test builds. A command number outside `Command` is the wire's
  refusal, not the table's, so L6.4 does not draw from any u16.
- L6.5 is in `common_role`'s.
- No binary or chart holds a law.

### The gate

- `make build` passes.
- The Clippy aspect over the 25 changed packages is clean. Its four findings were fixed: a single-arm `match`, a
  function of 71 lines (`compute_role`, its peer watch moved to `watch_peers`) and a loop over one element.
- `rustfmt --check` over the changed Rust files is clean.
- Every non-integration test of the changed packages passes: 58 targets.
- The integration groups ran in `make test`'s order: config 1 of 1, storage 15 of 15, compute 19 of 19, supervisor 2 of
  2 and cluster 40 of 40.
  - The first compute run failed 1 target: past-credit's log, above.
  - The first supervisor run failed 4 scenarios: an assertion of "started no role" matched the config service's start
    line. It now names storage and compute.
  - Neither was intermittent. Each was fixed and its group run again whole.
- `supervisor_tests:image_bdd` ran in Docker on this host's arm64, not skipped: 7 of 7. The development image came up
  with two roles over Unix sockets, and the two role images ran over `tcp://`.
- `ultragraphdb_compute_tests` holds 113 scenarios, down from 120:
  - 2 in the removed `system_group.feature`;
  - 4 removed from `role_binaries.feature`;
  - 1 from merging "carries no storage engine" with "carries no book".

### Found, not changed

- `frontend` still appears where task 6.8's search will find it. Each is left for checkpoint C:
  - feature descriptions and code comments;
  - the files `system_session_frontend.rs`, `storage_tests`' `played_frontend.rs`, `common_compute`'s two
    `played_frontend.rs`, `sessions_frontend.feature`, `failure_frontend.feature` and the target `bdd_failure_frontend`;
  - `Endpoint::Frontend`, the wire's `RoleKind::Frontend`, `common_service`'s `RoleKind::Frontend` and
    `CloseRequest::frontend`;
  - the Welcome name `"frontend"` of test servers in `storage_tests`, `ultragraphdb_tests` and `common_system`.
- Two scenarios of `ultragraphdb_compute_tests`' `compute_reads.feature`, "A read's stream starts with the frontend's
  credit" and the batch relay's texts, describe the relay, so task 6.4 decides them.
- Delta scenarios no feature binds yet:
  - group 2's Compute rule: "A compute instance without tags is refused", "An advertised address a Join would refuse is
    refused", "Two compute instances advertising one address are refused" and "A target of several compute instances is
    accepted";
  - group 3: "A silent node's connection is closed" and "Members names no compute node";
  - task 6.8: "Nothing is named frontend";
  - "An upgrade keeps the Secrets" and "A bench Job reports". Neither was bound before this change, which copies their
    requirement unchanged.
- C6.26, a stop bounded by `transaction_ms` with a read-write transaction left open, has no scenario. The bound is
  implemented: the drain waits at most the endpoint's `transaction_ms`.
- `ultragraphdb_tests`' "An empty namespace comes up" holds out its node-list clause until group 7, as `pods.feature`
  notes.
- `specification/conformance/storage/__pycache__/` is untracked. This work did not make it.

## Task 6.4: compute_tests' reads on the client endpoint, and ComputeSession removed (2026-11-19, base `dabad993`)

The plan, the foundation and the worker briefs are in `notes/group-6-4-plan.md`. `common_compute`'s move onto
`ReadExecutor` is in `notes/group-6-4-common-compute.md`. Nothing in the workspace serves or sends a ComputeRead any
more. The ComputeRead wire builder, view, golden frame and fuzz target stay; they are task 1.5's.

### The accepted decisions

- Until this task, `ultragraphdb_compute_tests`' reads ran against an in-process `ComputeSession`.
- `joining.rs`' Members and liveness asks stay, without the Join.
- `system-group/system_group.feature` was removed, and `played_frontend.rs` became `played_system.rs`.
- `CoreListener` stays: the client endpoint listens through it.

### Scenario accounting, `ultragraphdb_compute_tests`

Before the foundation, the crate held 113 scenarios. 80 of them sent ComputeRead to the in-process session:

| Class | Count | What happened |
|---|---|---|
| COVERED | 45 | Removed. 40 are bound through the binaries in `ultragraphdb_tests`, 5 through `ReadExecutor` in `common_compute`'s `executor.feature` |
| RELAY | 1 | Removed: "A read's stream starts with the frontend's credit". The delta replaced it with "…the client's credit", bound in `common_compute` |
| In-process lookups | 7 | Kept as they are: they drive `ComputeCore`'s lookups in the test process and never reached the session |
| REBIND | 27 | Rebound to the compute binary's client endpoint, texts unchanged: 1 in the foundation as its worked example, 6 by W1, 8 by W2, 6 by W3, 6 by W4 |

After: 67 scenarios over 21 targets, all passing. Each changed target, scenarios passed of run:

| Target | Scenarios |
|---|---|
| `bdd_quad_reads_compute_reads` | 4 of 4 |
| `bdd_quad_reads_lookups` | 8 of 8 |
| `bdd_quad_reads_read_faults` | 7 of 7 |
| `bdd_quad_reads_referrers_and_versions` | 6 of 6 |
| `bdd_quad_reads_transaction_reads` | 3 of 3 |
| `bdd_term_dictionary_dangling` | 3 of 3 |
| `bdd_wire_protocol_past_credit` | 1 of 1 |
| `bdd_index_maintenance_reverse_count` | 2 of 2 |

`system-identity/resume_keys.feature` and `tests/batch_steps.rs` were removed by `git rm`; all their scenarios were
COVERED.

`ultragraphdb_tests`' "A bound literal object fans out" took the delta's text: `<a>`, partition 1's row, before `<c>`,
partition 2's. Its Then now asserts that order; before, it compared the rows as a set.

### `common_compute`

- 137 unit tests, the property tests and every scenario drive their reads through a `ReadSession` over `ReadExecutor`,
  with their inputs and expectations unchanged.
- Three tests are removed, since they tested the session itself:
  - `compute_session_tests.rs`' two unit tests;
  - the property test `test_both_callers_get_one_answer`.
- After the move, these went:
  - `src/types/compute_session.rs` (by `git rm`) and its export;
  - `ReadRequest::of`, which only the session called.
- Doc comments that named a ComputeRead were restated: `route_sink.rs`' `with_credit` and `tests/endpoint_stage.rs`'
  `view_frame`.

### What the workers changed beyond the binding

- **W1: the frame bound in "A client holding no credit holds a frame's worth of names".**
  - The bound is now derived rather than stated: 2 frames (the terms' and the list's), plus the frames the rows of
    `DEFAULT_INITIAL_CREDIT + 1` Result frames at the floor take, plus the partition's initial credit. That comes to 16,
    in place of the stated 12. The run observed 14.
  - The step also asserts that the bound is below what the whole list takes.
  - It restates `ultragraphdb_common_storage`'s `TURN_BYTES` locally (256 KiB), naming its source, because the test
    crate does not depend on storage.
- **W2: the retention fixture.** Each graph holds `ROWS_PER_GRAPH = DEFAULT_INITIAL_CREDIT` rows, so the first graph's
  batches spend the client's starting credit and the read stalls before the second graph.
- **W3 and W4: session-token reads.** Where a scenario needs the read to wait on a partition's close, the read is a
  snapshot Begin naming no version and carrying a session token of the epoch (`SessionToken::new(1, 1, epoch)`). A named
  version above a partition's watermark is refused at once with code 27 (`PROTOCOL.md` §6.2, §11.11).
- **W4: the code-25 scenario.** "A later latest-state reverse read of another home is refused" runs in one read-only
  transaction at latest level. The transaction's first read binds it to partition 1, and the reverse read of a home on
  partition 2 is then refused with code 25.
- **W3: a gap carried over.** In "A span recorded after a Cancel only widens the commit's check", a refusal of conflict
  kind Range does not prove that the span recorded after the Cancel was counted, as opposed to the read's earlier spans.
  The old binding had the same gap. It is recorded here, not closed.

### The snapshot race and its fix

- `client::snapshot`/`snapshot_at` named a read version at the top of epoch latest + 1, which the partitions had usually
  not closed. Compute's Scans were refused at once with code 27 (`PROTOCOL.md` §6.2, §11.11). W4 saw it in 4 of 10
  serial runs.
- Both helpers are now async and wait, through `Scene::closed_through`, until every partition of the scene's cluster has
  closed the epoch, then build the read.
  - Every caller reads with no partition held.
  - The played partitions report epoch 9, past the epoch 1 their callers name.
- W4's local waits in `access_steps.rs` and `reverse_version_steps.rs` are gone.
- Proof: 10 serial runs of `bdd_quad_reads_referrers_and_versions`, 6 of 6 scenarios each time. Every other target using
  the helpers passed once in the compute group.

### "A read past the read set's room ends its transaction"

- The played partition now answers a Bind with a Bind reply at its `VERSION`, flagged `LAST` as its Closed reply is, and
  a Cancel with nothing.
- The scripted `TooLarge`, `detail` 524,288, on the dictionary Scan then reaches the client with its detail. The scenario
  passes. No library code changed.

### The old ComputeRead layer in `scene.rs`

Removed:

- `compute_client`, `answer_on`, `next_batch`, `next_of`;
- the `Scene` read builders `snapshot`, `snapshot_at`, `read_of` and `answer`;
- `Transaction::read`, and `Transaction.identity` with its initialiser.

`Scene::begin` stays for `commit_version`. Every read now goes through `tests/client.rs`.

### Found, not changed

- `AGENTS.md` still names `ComputeSession` in `common_compute`'s row. That table is checkpoint C's.

### The gate

- `make build` passes.
- The Clippy aspect over `//ultragraphdb_common/ultragraphdb_common_compute/...`,
  `//ultragraphdb/ultragraphdb_compute_tests/...` and `//ultragraphdb/ultragraphdb_tests/...` is clean.
- `rustfmt --check` over every changed Rust file is clean.
- `bazel test //ultragraphdb_common/ultragraphdb_common_compute/...`: 13 of 13 targets.
- The compute group passes 21 of 21 targets, 67 scenarios.
- The cluster group passes 40 of 40 targets; `bdd_quad_reads_spread` passes 8 of 8 scenarios.
- No run failed, so no rerun was needed and no intermittent is recorded.

## Checkpoint C: tasks 6.5 and 6.8 (2026-11-19, base `740b6cbd`)

### Task 6.5: the removals

Each was approved by the user. They landed in three places:

- Task 6.1 removed `ultragraphdb_frontend_tests`.
- The switch (checkpoint B) removed the `dev` and `dev-net` profiles' `frontend.toml`, the chart's
  `templates/frontend.yaml` and `profile/kube/frontend.toml`, the development image's frontend layer, the chart's
  frontend image value and every `copy_frontend` target.
- Commit `960582ff` removed `ultragraphdb/ultragraphdb_frontend` (`main.rs`, `service.rs`, its manifest and its
  `BUILD.bazel` with the image targets), the emptied `ultragraphdb_common_frontend`, their workspace member and
  dependency entries, and `common_service`'s `RoleKind::Frontend`, which the user confirmed as part of task 6.5. The
  same commit renamed `Endpoint::Frontend`, the system group's role address, to `Endpoint::System`, and
  `CloseRequest::frontend` to `CloseRequest::compute`, and brought `AGENTS.md` to two roles.

`FrontendTable` was removed by stage 5.8 (commit `cb78363a`, its task 7.2), so task 6.5 found nothing of it.

### Task 6.8: the renames

Four workers renamed each `frontend` in their crates by its meaning, each in a worktree from `960582ff`, under one set
of rules: the client endpoint or the sender of a client's requests became compute, the system group's host or address
became the system group. The table counts lines naming `frontend`, case-insensitive: before at `960582ff`, after the
worker's patch.

| Worker | Crate | Before | After |
|---|---|---|---|
| a | `ultragraphdb_common_compute` | 170 | 7 |
| b | `ultragraphdb_tests` | 103 | 0 |
| b | `ultragraphdb_compute_tests` | 1 | 0 |
| b | `ultragraphdb_client_core`, `_compio`, `_tokio` | 11, 2, 2 | 0, 0, 0 |
| b | `ultragraphdb_protocol_scram` | 8 | 0 |
| b | `ultragraphdb_tool_bench` | 10 | 0 |
| c | `ultragraphdb_storage_tests` | 89 | 1 |
| c | `ultragraphdb_storage` | 7 | 0 |
| c | `ultragraphdb_common_storage` | 64 | 0 |
| c | `ultragraphdb_cluster_metadata`, `_partition`, `_sequencer` | 14, 2, 4 | 1, 0, 0 |
| c | `ultragraphdb_core_catalog`, `ultragraphdb_store_apply` | 2, 1 | 0, 0 |
| c | `ultragraphdb_transactions_commit` | 5 | 0 |
| d | `ultragraphdb_common_system` | 38 | 7 |
| d | `ultragraphdb_common_config_check` | 41 | 34 |
| d | `ultragraphdb_common_role` | 13 | 10 |
| d | `ultragraphdb_protocol_wire` | 61 | 19 |
| d | `ultragraphdb_protocol_transport` | 15 | 0 |
| d | `ultragraphdb_common_config` | 1 | 0 |

`ultragraphdb_common_service_config` (16 lines, every one a refusal) was in no worker's set. After the merge, the
edits below took `common_compute` to 5, `storage_tests` and `cluster_metadata` to 0, and `common_system` to 2. Across
the workspace's members, 680 lines named `frontend` at `960582ff`; 86 do now.

Renamed by `git mv`:

| Before | After |
|---|---|
| `ultragraphdb_common_compute/features/transactions/sessions_frontend.feature` | `…/sessions_compute.feature` |
| `ultragraphdb_common_compute/tests/played/played_frontend.rs` | `…/played/played_system.rs` |
| `ultragraphdb_common_compute/tests/played_reads/played_frontend.rs` | `…/played_reads/played_system.rs` |
| `ultragraphdb_tests/features/transactions/failure_frontend.feature` | `…/failure_compute.feature` |
| `ultragraphdb_storage_tests/tests/played_frontend.rs` | `…/played_system.rs` |
| `ultragraphdb_common_system/src/types/system_session/system_session_frontend.rs` | `…/system_session_compute.rs` |
| `ultragraphdb_common_system/tests/types/system_session/system_session_frontend_tests.rs` | `…/system_session_compute_tests.rs` |

The target `ultragraphdb_tests:bdd_failure_frontend` became `bdd_failure_compute`, with its feature file.

### The user's decisions

- The `frontend` names that stay for good are the refusals of retired names: the settings table `[frontend]`,
  `compute.role_address`, `cluster.frontend_address`, instance names beginning `frontend`, and the role name `frontend`
  refused in credentials and the system group, with their tests and laws L6.2 and L6.5.
- The names that stay until task 1.2 are the wire's `RoleKind::Frontend` (role kind 1) with what decodes or matches it,
  and command 523 (ComputeRead) with its docs and `Endpoint::Compute`.
- The delta scenario "Nothing is named frontend" leaves out the refusals of retired names. Task 1.2's wire items are its
  known leftovers, and at task 1.2's gate it passes with none (`tasks.md`, task 1.2).
- What lies outside the search's scope stays as it is: `scripts/cluster_local.sh`'s guard refusing an old three-role
  cluster and `AGENTS.md`'s description of it, `ROADMAP.md`'s stage titles, `ATTRIBUTION.md`'s StarRocks rows
  (StarRocks' own frontend nodes), the comment in `.cargo/mutants.toml`, and `openspec/` and `specification/`, which are
  history.
- Comments: a history sentence dropped from `storage_tests`' `features/system/role_commands.feature` and a history
  clause from `common_compute`'s `tests/played/played_system.rs`; `metadata_group_access.rs`'s doc of `register_session`
  and `common_compute`'s `tests/tightening_steps.rs` name both compute and the system group; `SystemStart::CORE` keeps
  its value of 1, its doc restated.

### The in-process check path removed

With the user's approval, `common_system` lost the path by which the frontend's endpoint checked a session in its own
process: `SystemHandle::begin_commit`, `begin_recall` and `commit_ended`, `SystemAsked::BeginCommit` and
`CommitEnded`, `SessionMarks::end_in_process`, and the `None` mark (`Mark.connection` is a `u64`). Since the switch no
production code called it. `test_a_session_evicted_in_the_same_turn_is_refused_its_commit` now runs on the real path, a
Session and a role connection's Check taken in one turn, and expects outcome 12. `tool_bench`'s `owner_run` times a role
connection's Check and Ended.

### Comment widths

Every comment and description line the staged diff changed that ran past its file's width, 120 characters or 128 in
`common_compute`, is rewrapped, in 42 files; where the carried words did not fit the next line of the same paragraph,
the rewrap went on into the lines after it. Two kinds of line are left as they are:

- `ultragraphdb_tests`' feature descriptions, which hold each paragraph on one line throughout;
- the table row of `latest` in `protocol_wire`'s `subscribe/mod.rs` (131 characters), since a Markdown table row
  cannot wrap.

### The search

`git grep -i frontend` over the workspace's members, `bazel/` (the images) and `deploy/` (the chart and its profile)
finds 86 lines in 25 files, and no file or target name. Each line is in one of the two classes the user kept:

| Class | Lines | Where |
|---|---|---|
| Refusals of retired names, with their tests | 63 | `common_service_config` (16: its `lib.rs` and `settings_reader` docs, the cluster table's and service config's tests and property test), `common_config_check` (34: `validation.feature` and the tests of the retired names), `common_role` (10: credentials, role secret, joining and serve tests, L6.5), `common_compute`'s `joining_tests.rs` (3) |
| Task 1.2's wire items | 23 | `protocol_wire` (19: `role_kind.rs`, `member.rs`, `join_view_body_view.rs`, `members_view_body_view.rs`, command 523's docs in `command/mod.rs` and `compute_read/mod.rs`, the `role_kind` and `subscribe_view` tests), `common_compute`'s `role_links.rs` (2) and `common_system`'s `system_session/mod.rs` (2), each matching `RoleKind::Frontend` |

The shipped profiles (`ultragraphdb_config/profiles/`) and the chart's `kube` profile hold none.

### "Nothing is named frontend"

The delta's scenario is restated:

```gherkin
Scenario: Nothing is named frontend
  When the workspace's members, the images, the chart and the profiles are searched for `frontend`
  Then none is found but the refusals of the retired names, with their tests: the settings table `[frontend]`,
    `cluster.frontend_address`, instance names beginning `frontend` and the role name `frontend`, each refused by name
```

`scripts/test.sh` checks it, in the step "Check retired names" ahead of the build, as the user decided: a hermetic test
reads only its declared `data`, and binding it as one would need a target exporting the files of each of the 74 member
packages. The step searches the tracked and untracked files of `ultragraphdb*/`, `bazel/` and `deploy/`, and fails on any
file naming `frontend` that `scripts/frontend_leftovers.txt` does not list with its count of lines, on a count that
differs, and on a file or directory named `frontend`. The list holds 25 files: 15 refusals and 10 of task 1.2. Shown to
fail by a temporary edit (a line added to `serve_tests.rs`, a file `bazel/frontend_probe.txt`), both undone.

### The development image booted

Machine: this Mac, Docker 29.8.2 in OrbStack (`linux/arm64` daemon, amd64 by Rosetta). Each platform of
`//ultragraphdb/ultragraphdb_supervisor:development_multiarch` was loaded with `development_test_load -- --platform
linux/<arch>` and started with `docker run --platform linux/<arch>` on its default `dev` profile, under the shipped
seccomp profile, the client endpoint published. `/data` was listed through `docker cp`, and `docker stop` sent SIGTERM.

| | arm64 (native) | amd64 (Rosetta) |
|---|---|---|
| Image | `sha256:271af809…` | `sha256:5dd30456…` |
| Order in the supervisor's log | config `/ready`, storage started, storage `/ready`, compute started, compute `/ready`, every role is ready | the same |
| Supervisor's first line to "every role is ready" | 485 ms | 1,290 ms |
| Driver of every runtime | io_uring | polling, the emulation's refusal as in stage 5.8 |
| SIGTERM | compute, storage, config stopped, each 0; exit 0 | the same |
| `/data` | `config/`, `storage/p1/r1/`, `system/meta/`, `system/seq/`; no directory of compute's | the same |

### The gate

- `make build` passes.
- The Clippy aspect over the 23 packages with a change is clean.
- `rustfmt --edition 2024 --check` over the 172 changed Rust files is clean.
- `make test`, cached, passes every phase. The first run stopped at the compute group, so `make test` ran a second time,
  cached, and finished:

| Phase | Targets passed | Run, first / second |
|---|---|---|
| Doc tests | 20 of 20 | 0 / 0, all cached |
| Unit tests | 91 of 91 | 57 / 0 |
| Property tests | 10 of 10 | 5 / 0 |
| Concurrency tests | 5 of 5 | 5 / 0 |
| BDD tests | 52 of 52 | 51 / 0 |
| Integration, config | 1 of 1 | 1 / 0 |
| Integration, storage | 15 of 15 | 15 / 0 |
| Integration, compute | 21 of 21 | 21, 1 failed / 0 |
| Integration, supervisor | 2 of 2 | — / 2 |
| Integration, cluster | 40 of 40 | — / 40 |

`supervisor_tests:image_bdd` ran in Docker on arm64, not skipped: 7 of 7 scenarios.

One intermittent failure: in the first run, `ultragraphdb_compute_tests:bdd_quad_reads_compute_reads` failed its first
two scenarios, "A term absent where its list lies answers no row" and "A term and its list take two Scans". Each client
was refused with `Unavailable: the connection to the system group is closed`; the target's other two scenarios passed.
Run alone once, the target passed 4 of 4, and in the second `make test` the compute group passed cached. The failure
struck the first two reads of a shared cluster, which starts on its first use, while 21 targets ran side by side. It is
recorded and not root-caused: it did not repeat.

### Found, not changed

- `SystemHandle`'s other in-process asks, `check_session`, `register_session` and `session_held`, have no production
  caller: only tests, `common_system`'s `tests/bdd.rs` and `tool_bench`'s `owner_run`.
- `Mark.writes` is now always `true`, so `SessionMarks::protects` answers as `is_marked` does.
- `ultragraphdb_tests`' feature descriptions cite `openspec/changes/stage-5-5-…` and `stage-5-8-…` paths, which may now
  be archived.
