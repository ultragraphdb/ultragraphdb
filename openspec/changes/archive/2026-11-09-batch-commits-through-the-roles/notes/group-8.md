# Group 8: verification

All on this Mac, 2026-11-10, at `4d0941dc` with this group's edits.

## 8.1 Ultra Style

`make format` and `make fix` leave the tree unchanged, and `make fix` reports no warning. `cargo clippy --workspace
--all-targets -- -D warnings` is clean.

The rules no lint checks, reviewed over the ten files this change adds under a `src/`. These are `metadata_reads.rs`,
`core_joins.rs`, `join_fan_out.rs`, `login_asked.rs`, `login_limits.rs`, `owner_syncing.rs`, `role_connection_phases/mod.rs`,
`referrer_cache/mod.rs`, and the log crate's `utils_test/` with `gated_storage.rs`. The review also covered the lines that join
them to their crates.

- **Rule 1.** No added function recurses. `ReferrerCache::held` calls `put`, and `put` calls nothing of the cache's.
- **Rule 2.**
  - `while_syncing` takes at most `ASKS_PER_TURN_MAX` (1,024) asks per sync. A turn makes at most four syncs through it: two
    in `answer_durable`, the sequencer's close, and the metadata group's learning of that close. So at most 4,096 asks
    wait in `Owner::waiting`, and the next turn takes all of them before at most 1,024 more from the inbox.
    `Owner::run`'s doc now states that bound.
  - `CoreJoins::follow` is an event loop that ends with its fan-out.
  - `JoinFanOut`'s channels are unbounded. They carry one `Join` per role that joins, which the cluster's roles bound.
  - The gate's waits end at `DUE`.
- **Rule 3.** The planes are stated now:
  - `ReferrerCache` is data plane. Held sets take memory as they are put, at most the budget fixed when the store opens
    (`REFERRER_CACHE_BYTES`, 16 MiB a partition).
  - `RoleConnectionPhases` is data plane; recording allocates nothing.
  - `MetadataReads` is control plane; reading allocates nothing.

  The cache allocates as it holds, within its budget, not at startup. Rule 3 stays open for the store, as stage 5.5's
  group 12 left it.
- **Rule 5.** Only a round's apply puts a set, once its batch is written, and a drop of a tenant's data forgets the
  tenant, so a held set is the stored one. `changeset_core_referrers_tests.rs` and the two drop tests in
  `changeset_core_drop_tests.rs` pin both.
- **Rules 8 and 9.**
  - `QuadStore`'s `Mutex<ReferrerCache>` now carries its reason: executions running beside each other read held sets
    through `&self` while a round's apply puts them, and the lock spans one lookup, put or forget.
  - `RoleConnectionPhases`' `Rc<RefCell>` and `SyncGate`'s `Rc<Cell>` already stated theirs.

### Coverage

`scripts/coverage.sh` over each crate's added files:

| Crate | Files | Uncovered at first | Added | Uncovered now |
|---|---|---|---|---|
| `ultragraphdb_cluster_metadata` | `metadata_reads.rs` | 0 | — | 0 |
| `ultragraphdb_store_quads` | `referrer_cache/mod.rs` | 0 | — | 0 |
| `ultragraphdb_common_frontend` | `core_joins.rs`, `join_fan_out.rs`, `login_asked.rs`, `login_limits.rs`, `owner/owner_syncing.rs` | 8 | `CheckSession` and `SessionHeld` asked while a sync is held (`owner_syncing_tests.rs`) | 0 |
| `ultragraphdb_common_storage` | `role_connection_phases/mod.rs` | 7 | `test_a_due_window_is_reported_and_starts_anew` | 2 |
| `ultragraphdb_transactions_log` | `utils_test/gated_storage.rs`, `utils_test/mod.rs` | not compiled under Cargo | `tests/utils_test/gated_storage_tests.rs`, two tests; the crate's own `test-utils` as a dev-dependency | 4 |

The log crate's suite built without `test-utils` under Cargo, so the gate was never compiled into it and the first
measurement found no line to count. Measured through `ultragraphdb_common_storage`, which uses it, 16 lines were uncovered.
The crate's suite now takes its own test utilities, as `ultragraphdb_common_frontend`'s does, and runs every operation
through the gate as well as a sync held at it.

Left uncovered, with their reasons:

- `role_connection_phases/mod.rs:71-72`, the `requests` and `window_us` fields of the report's `tracing::info!`. tracing
  evaluates a field only when a subscriber takes the event, and the crate's tests install none. The same report's due path,
  the window taken and cleared, is covered.
- `gated_storage.rs:53-56`, `SyncGate::until_waiting`'s error once no sync reached the gate within `DUE` (10 s). A test of
  it waits 10 s.

## 8.2 Mutation testing

`scripts/mutants.sh <crate> --diff 25c26cd1` ran over every library crate whose `src/` this change touched, in two lanes of six
jobs. Then a fix pass, and one `--re` run of the mutants it meant to kill.

Three crates were left out:
- `ultragraphdb_tool_bench`, by the owner's standing decision.
- The service binaries `ultragraphdb_frontend`, `ultragraphdb_storage` and `ultragraphdb_compute`, whose `service.rs`
  wiring their own `*_tests` crates test. `cargo mutants -p` on a binary runs none of those tests.

| Crate | Mutants | Caught | Missed | Unviable | Timeout | After the fix pass |
|---|---|---|---|---|---|---|
| `ultragraphdb_common_frontend` | 97 | 61 | 14 | 20 | 2 | 14 of 14 caught |
| `ultragraphdb_common_storage` | 32 | 23 | 1 | 7 | 1 | 1 survivor, below |
| `ultragraphdb_store_quads` | 48 | 37 | 9 | 2 | 0 | 10 of 10 caught |
| `ultragraphdb_transactions_log` | 29 | 18 | 6 | 4 | 1 | 12 of 12 caught |
| `ultragraphdb_store_apply` | 15 | 11 | 0 | 4 | 0 | — |
| `ultragraphdb_transactions_commit` | 11 | 9 | 2 | 0 | 0 | 2 of 2 caught |
| `ultragraphdb_common_service_config` | 10 | 10 | 0 | 0 | 0 | — |
| `ultragraphdb_cluster_metadata` | 6 | 3 | 2 | 1 | 0 | 3 of 3 caught |
| `ultragraphdb_common_service` | 4 | 0 | 0 | 4 | 0 | — |
| `ultragraphdb_common_role` | 2 | 1 | 0 | 1 | 0 | — |
| `ultragraphdb_common_compute` | 2 | 0 | 0 | 2 | 0 | — |
| `ultragraphdb_cluster_partition` | 2 | 0 | 2 | 0 | 0 | 2 of 2 caught |
| `ultragraphdb_common_config_check` | 1 | 1 | 0 | 0 | 0 | — |

A `--re` run re-tests every mutant its expression names, so its count can exceed the survivors it was run for.

The timeouts are mutants the suites hang on rather than pass: `PartitionCore::turn`, `Owner::turn` and
`Owner::answer_durable` returning `Ok(())`, and `SyncGate::release` doing nothing. The unviable mutants return
`Default::default()` for types with no default.

The tests the fix pass added:
- **`ultragraphdb_common_frontend`:**
  - `auth_throttle_tests.rs`: a ban capped at a century; a new source forgetting no other while the table has room; a full
    table forgetting counting sources, and making room from a ban once it ends; a ban of no time banning nothing.
  - `one_frame_phases_tests.rs`: a window given its core being the one its clones share.

  The zero ban is a value `LoginLimits::new` accepts and the settings check refuses, so that test pins behaviour production
  settings never reach.
- **`ultragraphdb_store_quads`:** `QuadStore`'s held referrer sets, answered by group and whole and gone with their tenant;
  `ReferrerCache::is_empty` false with one generation holding a set.
- **`ultragraphdb_transactions_log`:** every gated call checked against the simulated storage's trace; `until_waiting`
  returning once a sync waits and not before, over two rounds.
- **`ultragraphdb_transactions_commit`, `ultragraphdb_cluster_partition`:** requests handed over between `hold_rounds` and
  `release_rounds` close no round, and share the one the release closes. Until now only the storage crate's suite called
  either.
- **`ultragraphdb_cluster_metadata`:** the reads see a command once it is applied; a session no command registered is not
  held.

Recorded, not killed:

- `ultragraphdb_common_storage/src/types/storage_session.rs:75:18: replace || with && in StorageSession::answered`. The mutant
  keeps a stream's read times after the stream ends with requests unanswered, and keeps empty queues in the map. A later
  request on a reused stream is then measured from a stale read time, but the number of requests counted is the same.
  `RoleConnectionPhases` exposes only that count, so the difference shows in the logged latencies and nowhere a test reads.

## 8.3 The suites

The fix pass's tests first failed `cargo clippy -D warnings` twice: a shadowed binding in `gated_storage_tests.rs`, and
`Instant::now` in `role_connection_phases_tests.rs`, which the workspace disallows. The test now takes its clock from a
compio timer's first tick, as its neighbours do. Both lints are clean again, and `cargo fmt --all --check` passes.

`make test` with OrbStack running, so the supervisor's Docker image tests ran (`image_bdd` passed), exit 0:

| Phase | Targets |
|---|---|
| doc | 21 of 21 |
| unit | 79 of 79 |
| BDD | 37 of 37 |
| integration | 7 of 7 |

It ran twice: once before the two lint fixes, and once after, on the final tree, from Bazel's cache.

The five role batteries were built for `aarch64-unknown-linux-gnu` and run under Apple's `container` 1.7.1. Each ran in a
Wolfi container as uid 65532, on kernel 6.18.56, with 8 CPUs, 8 GiB and `/tmp` on the named volume
`ugdb-linux-suite-tmp` (ext4), as in group 3:

| Battery | Scenarios | Time |
|---|---|---|
| `ultragraphdb_storage_tests` | 98 of 98 | 51 s |
| `ultragraphdb_frontend_tests` | 20 of 20 | 11 s |
| `ultragraphdb_compute_tests` | 88 of 88 | 22 s |
| `ultragraphdb_supervisor_tests` | 28 of 28 | 11 s |
| `ultragraphdb_config_tests` | 44 of 44 | 6 s |

## 8.4 Cleanup

What this change left on this Mac, listed on 2026-11-10 before anything is removed:

- **`container` machines:**

  | Machine | Image | CPUs | Memory | State |
  |---|---|---|---|---|
  | `ugdb-c` | `cgr.dev/chainguard/wolfi-base:latest` | 1 | 2 GiB | running |
  | `ugdb-s` | `debian:trixie` | 5 | 8 GiB | running |
  | `ugdb-f` | `debian:trixie` | 5 | 8 GiB | running |
  | `ugdb-m6` | `cgr.dev/chainguard/wolfi-base:latest` | 11 | 12 GiB | stopped |

- **`container` volumes:** `ugdb-m7s`, `ugdb-m7f`, `ugdb-m6` and `ugdb-linux-suite-tmp`, each declared at 512 GiB and sparse.
- **`container` clusters:** none. `container cluster list` shows none; the `ultragraphdb` cluster of group 5 is gone.
- **Script state:** `target/cluster_local/kubeconfig`, the kubeconfig `make cluster-local` wrote.
- **`container` images:**
  - `ultragraphdb/*`: `client_bench:local`, `client_bench:probe`, `compute:local`, `compute:probe`, `config:local`,
    `development:probe`, `frontend:local` and `storage:local`.
  - 18 untagged platform images.
  - `busybox:1.37.0` and `kindest/node`.
  - `debian:trixie` and `cgr.dev/chainguard/wolfi-base:latest`, the measurement machines' bases.

`container system df`: images 4.54 GB, 2.58 GB reclaimable.

Removed on 2026-11-10, with the owner's go-ahead:
- the four machines (`container stop`, `container delete`);
- the volumes `ugdb-m7s`, `ugdb-m7f` and `ugdb-m6`;
- the kubeconfig;
- every image listed above but the Wolfi base, the tagged ones reclaiming 1.88 GB.

`target/cluster_local/` remains, empty; `make cluster-local` creates it again.

Once 8.3's Linux suites had run, `ugdb-linux-suite-tmp` and the Wolfi base image went too, the image reclaiming 119.4 MB.
`container image list`, `container volume list`, `container list --all` and `container cluster list` now show nothing.
