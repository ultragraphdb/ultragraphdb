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
