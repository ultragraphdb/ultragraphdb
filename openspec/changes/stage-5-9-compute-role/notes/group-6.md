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
