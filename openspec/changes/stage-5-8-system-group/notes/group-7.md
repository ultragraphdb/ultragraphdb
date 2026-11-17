# Stage 5.8, group 7: the switch, `ultragraphdb_common_config_check` and the settings of every binary and harness

Base commit `4c0e4a61`. This file records task 7.1, the Ultra Style lift of `ultragraphdb_common_config_check`, and tasks
7.2 (the config-service part), 7.3 and 7.4: the checks System, System address, Data and Fixed; the old settings and
`common_role::frontend_address` removed; the profiles, the chart's profile, `scripts/bench_cluster.sh` and every test and
harness that writes settings brought to the system group's settings. Task 7.5 and the rest of 7.2 follow in their own
section, *Task 7.5 and the role-binaries part of task 7.2*. Counts are those Bazel's test logs report.

## Task 7.1: the audit against `openspec/protocols/ULTRA_STYLE.md`

Scope: all of `src/` of `ultragraphdb_common_config_check` at `4c0e4a61`: 12 files, 1,769 lines, 71 functions.

How it was measured, as groups 1 to 6 measured: every file read in full against the rules, with the search patterns of
group 4's checklist (`dyn`, `fn(`, recursion, `loop`/`while`, unbounded collections, `assert!`, `let _`, `.ok()`, casts,
cells and atomics, `..=`, division); rule 4 by a script counting each function's lines, comments and blank lines left
out; Clippy under Bazel through the aspect with the five lints Ultra Style states beyond the workspace table
(`-Dclippy::indexing_slicing`, `cast_possible_truncation`, `must_use_candidate`, `missing_errors_doc`,
`missing_panics_doc`), clean after the lift.

| Rule | Result | Evidence |
|---|---|---|
| 1 | conforms | no recursion, `dyn`, function pointer or stored callback; closures called in place |
| 2, loops | **gap, closed** | `TargetFiles::from_bundle`'s `while` (`types/target_files.rs:93`) did not say what ends it; it does now: each turn moves past at least two bytes or refuses |
| 2 and 3, a bound | **gap, closed (a defect)** | `partitions_of` (`utils/cross_checks.rs:526`) walked and collected a storage instance's `storage.partition_count` whole, a `u32` no check bounds: a revision naming 4,294,967,295 held 2^32 partitions in memory and reported each one past the cluster's count. It now walks at most `PARTITIONS_WALKED_MAX`, one past `PlacementMap::PARTITIONS_MAX`, so such a count is still refused (a partition outside the cluster's) and refused at once. Test: `test_a_partition_count_of_billions_is_walked_no_further_than_the_cluster_holds`. Not run before the fix: the unfixed code allocates 16 GiB for that input |
| 3, planes | **gap, closed** | `lib.rs` states the crate is control plane alone and what bounds its work |
| 4 | conforms | longest `required`, 70 lines at the base, at the bound; Clippy's `too_many_lines` clean |
| 5, 8, 10 | conforms | no run-time `assert!`, no macro defined, no `cfg`, no `#[expect]` or `#[allow]` |
| 6 | conforms | `shadow_unrelated` clean; `let length: usize = length.parse()` refines one value |
| 7, `.ok()` | **gap, closed** | five `.ok()` dropped an error without saying why: `check_target.rs:80`, `:113` (a document read for its lines alone, its refusal reported by `problems`), `merge.rs:73` (an instance whose file does not parse is refused by the file's own check), `cross_checks.rs:51`, `:71` (`located`). Each now says why |
| 7, `let _` | **gap, closed in part** | `let _written = writeln!(…)` in `merge.rs` drops a `fmt::Result` writing to a `String`, which cannot fail; the functions now say so. `let _previous`/`let _none` of a map's `insert` keep their names, the repository's form |
| 7, arithmetic | **gap, closed** | `from_bundle`'s `at + line_end + 1` and `end + 1` now say why they cannot overflow: each indexes a byte of the bundle |
| 9 | conforms | no `Arc`, cell or atomic; `Box<Defect>` keeps a `Result` small |
| `#[must_use]` | **gap, closed** | `Defect::new` |
| `# Errors` | conforms | every public fallible function documents its error |
| Naming | **gap, closed in part** | `role_addresses`' closures took `|a|`; they take `|address|`. Left open: `Checking::report` and `located` take `instance` and `setting` as two `&str`, swappable (Ultra *Types*, "No mixed-up arguments"); private, with 60 call sites |
| Types | left open | `Instance::role()` is a `&'static str` matched with `_` where a role enum would be exhaustive (Ultra *Types*); a public signature, and `common_service`'s `RoleKind` names more roles than a target holds |

### The lift and its evidence

Small and local: comments, one attribute, one bound with its test. Files: `src/lib.rs`, `src/types/defect.rs`,
`src/types/target_files.rs`, `src/utils/check_target.rs`, `src/utils/cross_checks.rs`, `src/utils/merge.rs`,
`tests/utils/cross_checks_tests.rs`. The patch of the lift alone, taken before task 7.4 touched the same files, is
`task-7.1-lift.patch` (held outside the repository; see the report).

| Target | Before, at `4c0e4a61` | After the lift |
|---|---|---|
| `common_config_check:ultragraphdb_common_config_check_tests` | 42 passed | 43 passed (the bound's test) |
| `common_config_check:bdd` | 21 scenarios, 42 steps | 21 scenarios, 42 steps |
| Clippy aspect over the crate | | clean |

## Tasks 7.2 (the config-service part), 7.3 and 7.4

### What was built

`ultragraphdb_common_config_check` (`src/utils/cross_checks.rs`, `src/utils/check_target.rs`), the config-service
specification's table as stage 5.8 has it:

- **System** (`system`, `hosts_counted`, `host_holds`): whether an instance holds a table is read from the files it takes its
  settings from, its own and its role's, so an empty header counts and a file that does not parse tells nothing. No storage
  instance holding `[system]` is refused naming `system` and the target; each host after the first naming `system` and the
  first. A host lacking `[metadata]` or `[sequencer]` is refused naming the table; a host holding no partition naming
  `system`. Any other instance, a frontend and compute included, holding a moved table is refused naming the table. A host
  lacking `system.role_address` or `system.data_dir` is refused by the file's own check, which now names the setting
  (`describe` takes `ServiceConfigErrorEnum::Missing`: "it is absent"), and still counts as a host, so no second defect
  follows from it. The count is skipped while a storage instance's file does not parse, as coverage is.
- **System address** (`system_address`): with exactly one host, every instance's `cluster.system_address`, the host's own
  included, is compared by `reach`: scheme and port on TCP, the whole path on a Unix socket; a host may differ.
- **Data** (`data_dirs`, `system_data_dir`, `components`, `relation`): directories are compared component by component as
  written, a trailing separator ignored (`.` and `..` are components); `system.data_dir` equal to, inside or containing the
  `data_dir` of any instance on the host's host, its own included, is refused naming both settings and the instance.
- **Fixed** (`fixed_system`): against an active revision with exactly one host, another instance holding `[system]` is
  refused naming `system`, and a moved `system.data_dir` naming it; `system.role_address` is free.
- What read the frontend reads the host: **Required** (every role requires `cluster.system_address`; the frontend nothing of
  its own; a host `cluster.partition_count`, `sequencer.epoch_ms_min` and `epoch_ms_max`), **Bounds** of the sequencer, the
  metadata group and the partition count, **Partitions**' total and **Relations**' `epoch_ms_max`. `system.role_address` is
  a listener in **Addresses** and an address `Address::parse` checks.

The switch (design D2), in one change:

- `ultragraphdb_common_service_config`: `ClusterTable::frontend_address` and `FrontendTable` with `ServiceConfig::frontend`
  removed. `[frontend]` stays in the reader's tables holding no setting, so `frontend.role_address` and `frontend.data_dir`
  are refused as unknown settings by their names, as `cluster.frontend_address` is; with the table gone they would be
  refused as an unknown table, naming no setting, against config-service, "The frontend's old settings are refused by
  name". `src/types/frontend_table.rs` is out of the module tree and waits for deletion (*Files to delete*).
- `ultragraphdb_common_role`: `frontend_address` removed; `retry_until`'s refusal names the system group.
- `ultragraphdb_common_compute`: `group_address` removed; the compute binary dials `common_role::system_address`.
- `ultragraphdb_storage`: `dialled` and its `cluster.frontend_address` fallback removed; the partitions dial
  `system_address`.
- `ultragraphdb_supervisor`: the bootstrap password is drawn when the host's `system.data_dir` holds no `meta/`, where it
  read `frontend.data_dir`: strictly what removing the setting needs (it is task 7.5's "bootstrap password drawn for the
  hosting storage instance").
- The profiles `dev`, `dev-net` and the chart's `kube`: the storage instance hosts the system group (`[system]` at
  `unix:///run/ultragraph/system.sock`, `tcp://127.0.0.1:7081` and `tcp://storage-0.storage.ultragraphdb.svc.cluster.local:7081`,
  `system.data_dir = "/data/system"` per design D11), holding `[metadata]` with the bootstrap file, `[sequencer]` and
  `cluster.partition_count`; every role names `cluster.system_address`; the frontend holds its endpoint alone. 7081 was the
  frontend's role port and lies below every partition's (7100 and up). `config_tests`' profile scenario runs
  `ultragraphdb_config_cli` over the three: each passes.
- `scripts/bench_cluster.sh`: the same layout on ugdb-s (system group at `$S:7081`, `/var/tmp/m7/system`).
- Harnesses: `ultragraphdb_tests`' `one_host` (`SYSTEM_ROLE_ADDRESS` in place of `FRONTEND_ROLE_ADDRESS`, storage hosting the
  group, `not_hosting` for a second storage process, a `[metadata]` table of `sessions_max = 4096`, the bound §11.9 gives
  unnamed, because a settings list writes no empty table), `harness` (`storage_holding` gives both data directories,
  `frontend_holding` none), `cluster.rs` (`Spec::host` for the moved tables' settings, the first storage process hosting,
  `Cluster::system`, `host_log`), `bdd.rs`, `own_steps.rs`, `tightening_steps.rs`, `write_read_steps.rs`; `storage_tests`
  (`bdd.rs`, `system_bdd.rs`, `one_frame_steps.rs`, `played_home.rs`), `compute_tests` (`cluster.rs`, `system_steps.rs`),
  `frontend_tests` (`bdd.rs`, `endpoint_steps.rs`, `role_read_steps.rs`), `config_tests`, `common_config`'s tests,
  `supervisor_tests` (`bdd.rs`, `images.rs`). Step bindings whose mechanics needed the host where they started the
  frontend, the feature text unchanged (design D12 leaves text edits to task 8.1): storage_tests' "a storage process
  started with a different role secret connects to the frontend" (the hosting storage process takes the frontend's place;
  Members lists the host's partition alone); frontend_tests' TLS, bootstrap, role-secret and read scenarios (a hosting
  storage process starts before the frontend, and the frontend's `/ready` is awaited); the supervisor's "SIGTERM while the
  frontend waits for its partition" (`block_size_max = 1` written to the host's `[metadata]`).

### Phase 1 and 2: the surface and the failing run

The public surface does not change: `check_target` keeps its signature. Phase 1 put the four rules in as private functions
with bare `unimplemented!()` bodies (`system`, `system_address`, `system_data_dir`, `fixed_system`), called from
`cross_checks`; `bazel build` passed. The suite against it (logs kept outside the repository):

| Target | Phase 2 |
|---|---|
| `common_config_check:ultragraphdb_common_config_check_tests` | 59 run: 19 passed (the tests reaching no cross check), 40 failed, every one at the stub (`cross_checks.rs:116`) |
| `common_config_check:bdd` | 26 scenarios: 5 passed, 21 failed at the stub ("not implemented") |
| `common_config_check:property` (new) | 6 run, 6 failed at the stub |

The old settings' tests fail by assertion, not at a stub: the settings were still read until the switch removed them.

### Phase 3: the audit of the inputs

The fixtures keep apart what the rules compare: the host's `system.role_address` (`10.0.0.2:7100`) differs from every
partition's address, its `system.data_dir` (`/data/system`) from `storage.data_dir` (`/data/storage`) as siblings, and the
host is not the first file in order. A port, a host, a scheme and a path are varied one at a time and together (C7.13 to
C7.16). The directories' cases separate equal, inside, containing, siblings and a text prefix that is no component prefix.
Two generators were found too weak while their properties were shown to bite (below) and were strengthened: L7.2 varied
every address at once, so a host alone differing was almost never drawn with the rest agreeing; L7.1 drew each part of a
host by a coin, so an accepted target was rare. Each now draws one host (or one varied address) and its parts at odds that
make acceptance and each refusal likely.

### Phase 4: the suite at its end

| Target | Before (`4c0e4a61`) | After | Uncached (`--nocache_test_results`, `--local_test_jobs=1`) |
|---|---|---|---|
| `common_config_check:ultragraphdb_common_config_check_tests` | 42 | 59 passed | 0.1 s |
| `common_config_check:bdd` | 21 scenarios | 26 scenarios passed | 0.1 s |
| `common_config_check:property` (new) | | 6 passed | 0.3 s |
| `common_service_config:ultragraphdb_common_service_config_tests` | | 31 passed | 0.1 s |
| `common_service_config:property` | | 5 passed | 0.1 s |
| `common_role:ultragraphdb_common_role_tests` | | 46 passed | 0.4 s |
| `common_compute:ultragraphdb_common_compute_tests` | | 172 passed | 3.2 s |
| `common_config:ultragraphdb_common_config_tests`, `bdd` | | 15 passed, 8 scenarios | 0.1 s each |
| `ultragraphdb_tests:unit` | | 35 passed | 21.6 s: `test_until_answering_fails_on_a_path_that_never_answers_with_success` waits `START_SECONDS`, unchanged by this group |

Each property was shown to bite, by a temporary edit since reverted: comparing whole addresses (L7.2 failed), dropping the
"contains" relation (L7.3), never refusing a moved host (L7.4), reporting a System address defect twice (L7.5), and ignoring
a moved table outside storage instances (L7.1). L7.6 failed in phase 2, the settings still read.

### Scenarios bound

`ultragraphdb_common_config_check/features/config-service/validation.feature`, config-service, "The config service
validates every setting before it serves one": "Two hosting instances are refused", "A target with no hosting instance is
refused", "The frontend's old settings are refused by name", "The books' directory inside a partition's is refused", "A
system address that differs is refused", beside the scenarios of the requirement bound since stage 5.5, whose fixtures now
hold a host. The role-binaries and kubernetes-deployment scenarios of task 7.2 are not this run's; 7.2 is not ticked.

### Corner cases C7.1 to C7.28

Tests in `tests/utils/cross_checks_tests.rs` unless named otherwise.

| Case | Test |
|---|---|
| C7.1 | `test_a_target_with_no_hosting_instance_is_refused`; bdd "A target with no hosting instance is refused" |
| C7.2 | `test_two_hosting_instances_are_refused`; bdd "Two hosting instances are refused" |
| C7.3 | `test_one_host_among_one_or_eight_storage_instances_is_accepted` |
| C7.4 to C7.6 | `test_a_host_lacking_a_system_setting_is_refused_naming_it` |
| C7.7 | `test_a_host_lacking_a_moved_table_is_refused_naming_it` |
| C7.8 | `test_a_host_holding_no_partition_is_refused` |
| C7.9 | `test_a_moved_table_outside_the_host_is_refused` |
| C7.10 | `test_every_system_defect_is_listed` |
| C7.11 to C7.16 | `test_a_system_address_reaches_the_host_by_its_scheme_and_port` (TCP), `test_a_system_address_reaches_a_unix_host_by_its_path`; bdd "A system address that differs is refused" |
| C7.17 | `test_the_hosts_own_system_address_is_checked` |
| C7.18 | `test_a_missing_system_address_is_required` |
| C7.19 to C7.21 | `test_the_groups_directory_sharing_a_path_on_its_host_is_refused`; bdd "The books' directory inside a partition's is refused" |
| C7.22 to C7.24 | `test_directories_are_compared_component_by_component`; the trailing separator between two instances in `test_one_host_holds_a_data_directory_once` |
| C7.25 | `test_only_a_data_dir_on_the_groups_host_is_compared` |
| C7.26 | `test_the_system_role_address_takes_part_in_the_address_checks` |
| C7.27 | `test_the_host_and_its_directory_stay_fixed` |
| C7.28 | `test_the_old_settings_are_refused_by_name`; `common_service_config`'s `service_config_tests::test_the_old_settings_are_refused_by_name`; bdd "The frontend's old settings are refused by name" |

### Laws

`tests/property/utils/cross_checks_property_tests.rs`, target `property`, tagged `property-test`, derandomized; the drawn
targets are built by `tests/property/utils/drawn_target.rs`.

| Law | Test |
|---|---|
| L7.1 | `test_the_system_check_accepts_exactly_one_complete_host`: one host (a second at one in four), each part kept at three in four, a moved table elsewhere at one in four; the System check's defects are those naming `system`, `metadata`, `sequencer` or a setting of theirs |
| L7.2 | `test_the_system_address_check_accepts_exactly_the_agreeing_targets` |
| L7.3 | `test_the_data_check_refuses_exactly_a_shared_path_on_the_host`: the model compares text, a directory equal to another or followed by a separator beginning it |
| L7.4 | `test_the_fixed_check_keeps_the_host_and_its_directory` |
| L7.5 | `test_every_planted_defect_is_listed_and_no_other`: seven plantable defects (Parsing, System, System address, Data, Identity, Bounds, an old setting), each in an instance of its own |
| L7.6 | `test_an_old_setting_is_refused_as_unknown` |

Group 2's laws L2.3 and L2.4 (`common_service_config`) drew `cluster.frontend_address` and `[frontend]` as settings in use;
they draw the settings the switch kept now, and L2.4 compares a storage instance's moved tables with a frontend file of
`[endpoint]` alone. Group 2's C2.9 tests (`cluster_table_tests`, `joining_role_tests`) and group 6's C6.4 tests
(`joining_tests`) state the switch's outcome: a file naming `cluster.frontend_address` is refused at the start.

### Phase 5

Not run: task 7.7 holds `scripts/mutants.sh` over `common_config_check` and `ultragraphdb_supervisor`. No decoder of bytes is
added; `TargetFiles::from_bundle` is unchanged.

## The gate

`make build` passes. `bazel test //... --test_tag_filters=-integration_test`: 176 targets, all pass. In the first such
run, before the harness was finished, `common_frontend:bdd_endpoint` failed once under the full parallel load ("When the
frontend dials a role address whose peer accepts and says nothing, carrying a request": "Error came on stream 2", 19.2 s);
alone it passes in 2.2 s, and it passed in the final run. Not this group's crate (*Found wanting* 6).

Clippy through the aspect over `common_config_check`, `common_service_config`, `common_role`, `common_compute`, the storage,
compute and supervisor binaries and `ultragraphdb_tests`: no finding on a line this group changed; `ultragraphdb_tests`'
`must_use_candidate`, cast and `indexing_slicing` findings and `common_compute`'s three in
`partition_rows_frames_tests.rs` stand as before.

The integration targets, each run once after the switch (the baseline is the same targets at `4c0e4a61`):

| Target | Baseline | After | Time |
|---|---|---|---|
| `ultragraphdb_tests:bdd` | 0 of 38 + 36 scenarios | 190/202, 10/10, 35/36, 36/38, 35/36 over its five runs | 257 s |
| `frontend_tests:bdd` | 3/20 | 16/20 | 57 s |
| `storage_tests:bdd` | 17/98 | **98/98, passes** | 95 s |
| `storage_tests:system_bdd` | 5/6 | **6/6, passes** | 4.6 s |
| `storage_tests:system_partitions_bdd` | 4/4 | **4/4, passes** | 7.7 s |
| `compute_tests:bdd` | 84/89 | 88/89 | 27.5 s |
| `config_tests:bdd` | 44/44 | **44/44, passes** (the profiles pass `ultragraphdb_config_cli`) | 4.9 s |
| `supervisor_tests:bdd` | timed out at 300 s | 27/28 | 22 s |

Every remaining failure, root-caused:

1. **The group's quiet time** (`PROTOCOL.md` §11.5; design D7). The system group evicts no session and answers every check
   of kind 1 with 14 for twice `transaction_ms` after it begins to listen, 20 s at the default. Scenarios that start a
   cluster of their own and fill the registry or resend a commit at once meet it, each failing with "the registry is full,
   and the group evicts no session for twice transaction_ms after it begins to listen" or "the session has a request in
   flight" (code 14): `ultragraphdb_tests` "A resent empty Commit answers not committed", "An eviction reaches every core at
   once", "An evicted session is told so", "A retry of a session evicted meanwhile is unknown", "A Register answered busy is
   sent again", and through tokio "A retried commit commits once". Three more fail on the same clusters with `TimedOut` or a
   lost connection and are read as the same cause without proof: "A session with a commit in flight is not evicted", "A full
   registry of commits in flight refuses a Register as busy", "A Register busy past its attempts returns code 14". The
   behaviour is the specification's; the scenarios are client-facing features of task 8.1, which must wait out the quiet
   time or run their clusters at a short `transaction_ms`.
2. **"A catalog version past its cap is refused at its Commit"**: refused with code 6 for passing one Declare frame (§11.13,
   design D13). Group 5's *Found wanting* 1, the user's to decide.
3. **"A token above the open epoch is refused"** (all four runs): §4.8 now holds the Begin until the close arrives, and the
   step's deadline answers `TimedOut`. Group 5's *Found wanting* 3: the scenario contradicts this stage's text and was
   retired from `common_frontend`; it stands in `ultragraphdb_tests/features/transactions/tokens.feature` for task 8.1.
4. **"A partition silent past the wait withdraws the tightening"**: the step stops the process `storage` with SIGSTOP,
   which is now the system group's host, so the frontend's Commit ends `OutcomeUnknown` instead of a code 24 from the group.
   It needs a partition silent in a storage process that does not host the group (task 8.3's failure runs).
5. **"A tenant's data drop keeps its catalog"**: refused (2, 5), a conflict of the catalog version. **Root-caused
   2026-11-16:** the step sent `DropCommand::tenant_data(TENANT)`, whose `catalog` is 0, "a tenant holding none" (`PROTOCOL.md`
   §6.5), in a tenant holding version 1 after `name`'s declaration; the frontend relays the client's frame unchanged
   (`client_session_one_frame.rs`, `drop_request`, `drop_event`) and the partition refused it as §6.5 requires
   (`commit_step_round.rs`, `drop_stale`). Neither the frontend nor the partition was at fault. The step now names version 1,
   the first one above none (§11.12), in a cluster of its own spec (`own_catalog`, so no earlier scenario's declaration moves
   the number), and waits for a Describe listing `name`, the version in effect at the latest close (§11.13, *Its epoch*).
   Passes over the binaries. `cluster_partition`'s C4.12/C4.13 test now names `catalog` 0 beside 3 and 5.
6. **frontend_tests** "Epoch closes reach storage in order", "The frontend answers a partition's lease and its placement",
   "A frontend without its role address stops the start", "The frontend's /health answers once its listeners are bound":
   each feature text names the frontend's sequencer, its answers to a partition or its role address, which the frontend
   no longer has (design D2, D5). The text is role-binaries' (task 7.2's other part) and task 8.1's.
7. **compute_tests** "A peer that stops answering without closing is a lost dependency": the storage process the step
   stops now hosts the group, and compute names the group's address for a loss of both (group 6's *Found wanting* 4), where
   the step asserts the partition's address. A storage process apart from the host is needed (task 8.1).
8. **supervisor_tests** "The data layout matches a node's": its text is stage 5.5's (`meta/`, `seq/` under `/data`); the
   profile follows design D11 (`/data/system/meta/`, `/data/system/seq/`), as role-binaries' modified scenario of this
   change states. Task 7.2's role-binaries part.

## Found wanting

1. **`[system]` outside a storage instance.** config-service, System, refuses a frontend or compute instance holding a
   moved table, and says nothing of one holding `[system]`, which only a storage process reads. Not refused; the user's to
   state.
2. **`cluster.partition_count` on an instance not hosting.** Only the host reads it. A different value in a frontend's,
   compute's or another storage instance's file is checked by nothing and stated nowhere.
3. **`[system]` split across a role's file and an instance's.** Each file is checked alone (Parsing), so a `[system]` table
   whose `role_address` stands in `storage.toml` and whose `data_dir` in `storage_1.toml` is refused, though the instance's
   merged settings hold both. Behaviour of the per-file check since stage 5.5; the profiles keep `[system]` whole.
4. **An empty `[metadata]` table is required.** System refuses a host lacking a `[metadata]` table, though every metadata
   setting has a default (§11.9). A host on defaults must write the header; the test harness writes `sessions_max = 4096`.
5. **Test targets past the bound.** `ultragraphdb_tests:unit` takes 21.6 s from one test of the harness waiting
   `START_SECONDS`; the integration targets take 4.6 s to 257 s, as before this group. Neither is split here.
6. **`common_frontend:bdd_endpoint` under load.** The scenario of a silent role address failed once at 19 s in the full
   parallel run, with an error frame where silence was awaited; it passes alone. Not root-caused; not this group's crate.
7. **The frontend's early refusal of a Drop's `catalog` is absent.** `PROTOCOL.md` §6.5: "The frontend refuses one whose
   `catalog` is not the tenant's current version the same way, before any partition sees it"; design D14: "The frontend's
   check stays as an early refusal". `drop_request` (`client_session_one_frame.rs`) relays the client's Drop unchanged and
   checks nothing against its view; the partition's check alone decides. Not changed here.
8. **No reply tells a client a tenant's catalog version.** A Drop of kind 3 or 4 names "the tenant's catalog version the
   client drops under" (§6.5), an Ingest the version it encoded against (§6.8), an Install the version it builds on (§8.4);
   only an Install's reply names a version. A declaring Commit answers `(c + 2) << 22` (§11.13, *Its epoch*), and Describe
   carries entries, not the version's fixed part (§8.3, `CATALOG.md` §2). A client of a tenant declared through Writes learns
   the number only by counting its own declarations from none, as "A tenant's data drop keeps its catalog" does. The user's
   to decide.
9. **A system link stalls under frames of a megabyte.** `common_frontend:bdd_catalog_cap` failed 2 of 16, 1 of 10, 2 of 30,
   7 of 60 and 9 of 40 runs (several runs at once, under another agent's load), and 0 of 34 in other batches: a declaring
   Commit is never answered (`no frame came` after 5 s), at Commits 0 to 4. At the stall the group holds no new version, a
   Members ask on the core's link answers nothing within 2 s, the client's connection to the frontend answers a Begin, and the
   view's closes keep arriving. So the frontend-to-group direction of the link stops while the group-to-frontend direction runs:
   the Declare of about 1 MiB is never taken by the group. The frontend and the group's sessions share one compio runtime there
   (polling driver, kqueue), and the stall needs frames large enough to wait for a socket's room. Over the binaries the
   scenario passed. **Root-caused and fixed 2026-11-17:** compio-driver 0.12.5's polling driver registers a socket's read and
   write under the write's key on kqueue; a write finished first turns the read's event stale, and the driver then returns
   from the whole batch, losing every later event, the group's read of the Declare among them (compio-rs/compio#1119).
   Upstream's fix (PR #1121, commit 5666663, in no release yet) is applied under Bazel as
   `bazel/patches/compio_driver_poll_stale_event.patch`; `protocol_transport_compio`'s carrier tests pin the interleaving,
   failing 10 of 10 before the patch, and `bdd_catalog_cap` passed 40 of 40 after it. The role server and the transport
   were not at fault.

## Left for 7.5

- The supervisor's start order (config service, storage, compute, frontend): it still starts the frontend first, awaiting its
  `/health`, which works because the frontend dials until the group answers.
- The chart: the frontend a Deployment with no volume; the bootstrap Secret mounted into the hosting storage pod alone (the
  `kube` profile now names `/secrets/bootstrap.secret` in the storage instance's file, which the chart mounts only in the
  frontend's pod today, so an install fails the first start until then); the storage Service and pod declaring port 7081;
  `scripts/cluster_local.sh`.
- The development image booted over the new profile (task 8.4), and the role-binaries and kubernetes-deployment scenarios of
  task 7.2.

## Files to delete

- `ultragraphdb_common/ultragraphdb_common_service_config/src/types/frontend_table.rs`: `FrontendTable` is removed from the
  module tree; the file compiles into nothing.

## Notes for the commit

Two commits. The lift first: its patch, taken before task 7.4 touched the same files, applies to `4c0e4a61` alone
(`src/lib.rs`, `src/types/defect.rs`, `src/types/target_files.rs`, `src/utils/check_target.rs`, `src/utils/cross_checks.rs`,
`src/utils/merge.rs`, `tests/utils/cross_checks_tests.rs`); then the rest. `Cargo.lock` takes `hegeltest` among
`ultragraphdb_common_config_check`'s dependencies.

## Task 7.5 and the role-binaries part of task 7.2

Base commit `cb78363a`, the switch. Counts are those the test logs report; times are Bazel's for a run with
`--nocache_test_results`.

### What was built

- **The start order** (design D10; role-binaries, "The supervisor starts, stops and reports the roles"). `ultragraphdb_supervisor`
  starts the config service, then storage, compute and the frontend (`START_ORDER`), waiting on each one's `/ready` before the
  next starts. It waited on the frontend's `/health` first and its `/ready` last; the frontend is now started last and its
  `/ready` is the last wait. The stop order is unchanged. Its help says so.
- **The bootstrap password drawn for the hosting storage instance.** The supervisor draws `metadata.bootstrap_secret_file`
  only for an instance holding `[system]`, and only while no `meta/` stands under its `system.data_dir` (`first_start` takes
  the directory, no longer an `Option`). Task 7.4 had already moved the test from `frontend.data_dir` to `system.data_dir`; an
  instance without `[system]` drew the password since `first_start(None)` held.
- **The development profile's `[system]` table and `/data/system`.** Task 7.4 did both (`profiles/dev/storage.toml`); nothing
  left.
- **The chart** (design D11; kubernetes-deployment). `templates/frontend.yaml`: the frontend a Deployment of one replica,
  `Recreate`, with no volume, no claim, no role port and the read key and role secret alone mounted; the headless Service
  `frontend` removed, since nothing dials the frontend but clients at `frontend-client`. `templates/storage.yaml`: the Service
  and the pod declare `system` on 7081, the pod mounts the bootstrap secret, its comment says it hosts the group. `values.yaml`,
  `templates/secrets.yaml` and `templates/compute.yaml`: comments that named the frontend as the books' host. The `kube`
  profile already named `tcp://storage-0.storage.ultragraphdb.svc.cluster.local:7081` (task 7.4).
- **`scripts/cluster_local.sh`.** `WORKLOADS` in the start order with the frontend a Deployment; `up` over a cluster whose
  frontend is still a StatefulSet fails, naming `make cluster-local-down`, since no data written before the change is read
  after it (D11) and an apply would run the new Deployment beside the old StatefulSet, both behind `frontend-client`.
- **`bazel/oci_images`**: nothing names the old layout (`/data` and `/run/ultragraph` alone); unchanged.
- **`AGENTS.md`**: the supervisor's row and *Local Kubernetes*.

Decision taken here, within the design: the frontend's Deployment uses `Recreate`, as compute's does, so a rollout never runs
two frontends while the config service holds one frontend instance (D11 and the Non-Goals say one frontend until stage 5.9). A
`RollingUpdate` would shorten a rollout's gap; it is the user's to choose.

### Scenarios bound (role-binaries)

| Scenario | Feature | Change |
|---|---|---|
| The start order reaches readiness | `supervisor_tests/features/role_binaries/supervisor.feature` | text of this change; the log's order: config `/ready`, storage started and `/ready`, compute started and `/ready`, the frontend started and `/ready` |
| The data layout matches a node's | same | text of this change; `system/meta`, `system/seq`, `storage/p1/r1` exist, and `/data` holds exactly the first components of the profile's `data_dir` settings (`config`, `storage`, `system`), which the frontend's file names none of |
| A signal during the start stops what was started | same | not a spec scenario; its When named the old order ("while the frontend waits for its partition"), now "while storage waits for its partition": storage's `/ready` never answers (`block_size_max = 1` refuses its first lease), and neither compute nor the frontend is started |
| The development container uses Unix sockets | `features/role_binaries/images.feature`, `image_bdd` | text of this change; the system group's socket read from the host's `[system] role_address`, the exact set of sockets the container adds (system, compute, each partition), and the frontend's file naming no `role_address` |
| The same frames cross TCP, and the other scenarios of the requirement | unchanged | bound since stage 5.5; pass under the new order |
| The frontend's /health answers once its listener is bound | `frontend_tests/features/role_binaries/role_binaries.feature` | text of this change ("its client address"); holds `CLIENT_ADDRESS`, `/health` 503 while held, a TCP connection to it once 200 |
| A frontend without the system group's address stops the start | same | replaces "A frontend without its role address": the frontend requires `cluster.system_address` (`common_role::system_address`), exit 78 naming it |

`images.rs`' `start_development` read `bootstrap_secret_file` from `dev/frontend.toml`, which since the switch names none, so
every development-container scenario of `image_bdd` failed at `cb78363a` with "bootstrap_secret_file in the profile" (read from
the code, not run: task 7.4's gate did not run `image_bdd`). It reads the host's `dev/storage.toml` now.

**Left for task 8.1**, in `frontend_tests/features/role_binaries/role_binaries.feature`, both failing with "Connection refused"
at the system group's address, since the frontend they start hosts nothing and no storage process runs:

- "Epoch closes reach storage in order" is wire-protocol's ("Roles exchange role commands"), whose text this change modifies to
  "the system group's sequencer closes epochs 1 to 5"; the frontend takes no part, so its home is a suite over a hosting
  storage process (`storage_tests`' `system_bdd`).
- "The frontend answers a partition's lease and its placement" is in no specification of this change or of `openspec/specs/`;
  the group's answers to Lease and Placement are system-group's and `common_system`'s (`features/client-endpoint/owner.feature`,
  "A lease is answered from a rising mark"). To be removed or moved; the user's call.

**kubernetes-deployment, not bound as Gherkin: a decision is needed.** No kubernetes-deployment scenario has ever been bound
as Gherkin: stage 5.7's group 5 checked them by hand on the local `container` cluster
(`openspec/changes/archive/2026-11-09-batch-commits-through-the-roles/notes/group-5.md`, §5.4). A hermetic target cannot render
the chart, as no Bazel toolchain provides `helm` (`MODULE.bazel`); a target driving `container`, `helm` and `kubectl` would be
neither hermetic nor near the 10 s bound. The ways I see: a `rules_helm` toolchain and a test crate checking the rendered
objects (the pod specs, the mounts, the ports, the absence of secrets), which covers "A pod runs unprivileged", "The frontend's
pod holds no bootstrap secret" and "No secret is in the repository" and leaves the cluster scenarios; or the cluster scenarios
bound over the built binaries, as task 8.2 plans for two of them; or the hand-run record of 5.7. What was checked here instead:
`helm lint` (Helm 4.2.2) passes; `helm template` renders the frontend a Deployment with no claim and no volume but its Secrets,
mounting the read key and the role secret alone; the storage StatefulSet with 7081 on its Service and pod and the bootstrap
secret among its mounts; and, into the namespace `graphs` under `example.internal` with `partitions=2`, every
`cluster.system_address` and `system.role_address` naming `storage-0.storage.graphs.svc.example.internal:7081` and both
partition counts 2. Not run: `make cluster-local` (an `-c opt` build of five images for both architectures, which would hold
Bazel from the agent working beside this one); the chart has not been installed on a cluster since the change.

### Phases

Phase 1: no public surface changes; the supervisor and the chart are a binary's wiring and deployment. No law, so no property
target (`tdd-protocol.md`, "A group without a law writes no property target").

Phase 2, the scenarios above against the supervisor at `cb78363a` (logs kept outside the repository):

| Target | Phase 2 | Failing for |
|---|---|---|
| `supervisor_tests:bdd` | 28 scenarios: 26 passed, 2 failed, 21.5 s | the start order and the signal during the start, each at "started instance=frontend" before storage's readiness |
| `frontend_tests:bdd` | 20 scenarios: 18 passed, 2 failed, 14.0 s | the two scenarios left for 8.1 alone; the two rebound ones passed at once, the behaviour being task 7.4's |
| `supervisor_tests:image_bdd` | 7 scenarios: 6 passed, 1 failed | the Unix-socket scenario, its new Then matching no step until the binding took its text |

"The data layout matches a node's" passed at once under its new text: the layout is task 7.4's profile. Its old text was the
one failing (group 7's gate, item 8).

Phase 3, the inputs: the start-order binding asserts the order of eight log lines, which no earlier order satisfies; the data
layout compares the set under `/data` with the profile's settings, so a frontend directory, a stray `meta/` or `seq/` at the
top, or a missing one fails it; the socket set is compared exactly; the held client address is distinct from the observability
address `/health` is read on.

Phase 4: the code above.

### Corner cases

| Case | Test |
|---|---|
| Storage never ready: nothing after it starts, and a signal stops what started | "A signal during the start stops what was started" |
| The password drawn once, for the host, before its first start | "A bootstrap password removed after the first start stays removed"; the image scenarios, which read it from the host's file |
| The frontend holding no data directory and no socket | "The data layout matches a node's"; "The development container uses Unix sockets" |
| The frontend's one required setting absent | "A frontend without the system group's address stops the start" |
| The client address held a moment, and past the bound | "The frontend's /health answers once its listener is bound"; "A port held past the bound ends the start" (task 7.4) |
| The chart into another namespace and domain, two partitions | `helm template` by hand (above); no test |
| A cluster of the old layout | the script's refusal; not run |

### The gate

`make build` passes. `bazel test //... --test_tag_filters=-integration_test`: 176 of 176 pass (10 executed, the rest cached,
the other agent's changes in the tree). Clippy through the aspect over the supervisor and the two test targets: clean.

The integration targets, once each, `--nocache_test_results`:

| Target | Result | Time |
|---|---|---|
| `supervisor_tests:bdd` | **passes**, 28 scenarios, 56 steps | 20.6 s |
| `supervisor_tests:image_bdd` (Docker 29.8.2) | **passes**, 7 scenarios, 14 steps | 17.0 s |
| `frontend_tests:bdd` | 18 of 20: the two scenarios left for 8.1 | 13.2 s |
| `config_tests:bdd` | **passes**, 44 scenarios, 88 steps | 3.6 s |

### Found wanting

1. **kubernetes-deployment has no place to run as Gherkin** (above). Task 7.2 stays open for it.
2. **Three targets past the 10 s bound**: `supervisor_tests:bdd` 20.6 s, `image_bdd` 17.0 s, `frontend_tests:bdd` 13.2 s, as
   before this task (22 s and 57 s at task 7.4's gate). Each starts its processes per scenario at declared ports and is
   `exclusive`, so a split by feature file brings each target under the bound and shortens no run. Not split here.
3. **`start_three_hosts`** (`images.rs`) still starts the frontend's container before storage's. The roles retry, so it
   passes, and "The same frames cross TCP" states no order; left as it is.
4. **`values.yaml`'s header** says the objects' fixed names are those "the role addresses of the `kube` profile name"; the
   frontend's is named by none now. Left.

## Task 7.2's kubernetes-deployment part, and the two scenarios out of `frontend_tests`

Decided by the user on 2026-11-16, after *Found wanting* 1 of task 7.5: the rendered chart's scenarios hermetically under
Bazel with rules_helm (a), the running cluster's over the built binaries as task 8.2 plans (b); "Epoch closes reach storage in
order" moved to `storage_tests`' `system_bdd`, and "The frontend answers a partition's lease and its placement" removed.

### (a) The rendered chart: `ultragraphdb_chart_tests`

- `MODULE.bazel` takes `rules_helm` 0.34.0 from the Bazel Central Registry (`periareon/rules_helm`, published 2026-10-05; 0.35.0
  of 2026-11-06 was under two weeks old). Its toolchain downloads Helm 4.2.0 checksummed per platform, and the module registers
  its toolchains itself; it brings `rules_go` 0.63.0 and `gazelle` 0.53.0, which build its packager and templater from Go
  sources with a Go SDK of their own. `MODULE.bazel.lock` gains 348 lines and changes none. Nothing runs a Helm of the host.
- `deploy/helm/ultragraphdb/BUILD.bazel`: `:chart`, the chart as `helm_package` makes it (templates, values, `Chart.yaml`, the
  `kube` profile as the `.Files` the ConfigMap reads), and `:sources`, every file of the chart.
- `ultragraphdb/ultragraphdb_chart_tests`, a test-only crate as `ultragraphdb_config_tests` is: `helm_template` renders `:chart`
  as `scripts/cluster_local.sh` does (the release `ultragraphdb` in its namespace) and as the release `graphs` into the namespace
  `graphs` under `example.internal` with `partitions=2`; aspect_bazel_lib's `yq` (already a dependency) writes each rendered
  object as a JSON line; the test reads them with `serde_json`, already in the workspace. No YAML crate was added. The renders
  sit in the test's package, so `deploy/helm/` in its runfiles holds the chart's sources alone, which a scenario searches.
  `Cargo.lock` takes the crate as a member (an entry with its four dependencies, checked with `cargo metadata --offline
  --locked`).

`features/kubernetes-deployment/chart.feature`, 9 scenarios: the spec's "A pod runs unprivileged", "The frontend's pod holds no
bootstrap secret" and "No secret is in the repository", verbatim; and what the four requirements state of the objects, under
names of their own: the workloads of one cluster (two StatefulSets of one replica, each data directory its settings name below
its claim's mount, the frontend and compute Deployments holding no volume but a Secret's projection, a headless Service per
StatefulSet and for compute publishing not-ready addresses, `frontend-client` serving the endpoint's port); the frontend's
container declaring its endpoint's and observability ports and no other; the storage pod declaring the port
`system.role_address` names on its Service and container, the group's logs below its claim, and it alone mounting the bootstrap
secret, inside the mount its `bootstrap_secret_file` lies in; every `cluster.system_address` and `system.role_address` naming
`<set>-0.<service>.<namespace>.svc.<domain>` at the storage Service's `system` port, as an outline over both releases, with the
partition count; and each role's environment holding the four bootstrap variables of a clear-text service, naming the config
Service, the profile's target, its own instance and the read key's file. Every expected value is read from the rendered
objects or settings, or is a name the spec states (65532, 65534).

The search for a secret refuses a PEM block, a run of at least 32 characters of the base64 alphabet holding a digit, a
lowercase and an uppercase letter (a random draw of that length lacks one with a chance below 10^-7), a run of 32 hexadecimal
digits, and any value the render drew into the four Secrets. A first bound, any run of 32 base64 characters, refused
`profile/kube.toml` for the path `/etc/ultragraphdb/profiles/kube/targets`.

Failing first: the suite was written after the chart it checks (task 7.5), so it was shown to bite by a temporary edit, since
reverted, putting the bootstrap secret and a role port 7081 back into `templates/frontend.yaml`: 9 scenarios, 6 passed, 3
failed ("holds no bootstrap secret", "declares … no other", "it alone of every pod mounts the bootstrap secret").

### (b) The running cluster over the built binaries: `ultragraphdb_tests`

`features/kubernetes-deployment/pods.feature` and `tests/kubernetes_steps.rs`, registered in `tests/bdd.rs`; `Cluster::observability`
added to `tests/cluster.rs`. A pod is a process started through the config service, its deletion with a grace period of zero a
SIGKILL, its workload starting it again the same instance on the settings the config service holds, over the same data
directories. Six scenarios, verbatim, in the first run alone (compio over TCP):

| Scenario | Bound as |
|---|---|
| An empty namespace comes up | a cluster from nothing: the config service and every role `/ready`, the frontend's settings naming no `data_dir`, a client authenticating and committing |
| A storage pod restarts at its address | two storage processes, the non-hosting one killed and started again: Members equal before and after and listing partition 2 at its address, a write to an IRI of partition 2 committed |
| A missing role secret stops a role | storage started with `role_secret_file` naming no file: exit 78 naming it; the file written, started again and `/ready` |
| A storage pod killed under writes keeps every acknowledged commit | 8 one-frame writes acknowledged, the hosting storage process killed as a ninth leaves (acknowledged if answered within 1 s), started again; once the group lists compute again, every acknowledged object read back. Compute runs with `cluster.join_deadline_ms = 10000` (*Found wanting* below) |
| The hosting storage pod killed keeps the books | a session registered, a declaring Commit (naming no session, `PROTOCOL.md` §11.13), the host killed and started again: the session's next commit accepted, the latest catalog version unchanged |
| A deleted frontend pod loses nothing | a session's write committed, the frontend killed and started again, a new client resuming the session one above its last request and committing; both objects read back |

`tests/cluster.rs` gains `Spec::compute`, a compute process's settings beyond the harness's, and `Cluster::observability` and
`Cluster::log`. Each waits 300 ms after its last commit before the kill, so the frontend's Ended reaches the group and no mark of the closing
connection holds the session for twice `transaction_ms`.

Not bound: "An upgrade keeps the Secrets". It is Helm's `lookup` against a cluster's Secrets and `scripts/cluster_local.sh`
creating only those absent; `helm template` reads no cluster, and the built binaries hold no Secret. It needs a cluster.

First run: 6 scenarios, 5 passed; "The hosting storage pod killed keeps the books" was refused "a Commit of declarations names no
session": the step committed the declaration through the session, which §11.13 forbids. The step was wrong; it commits with no
session now. The six bind behaviour tasks 7.4 and 7.5 built, so none failed for want of code.

### The two scenarios out of `frontend_tests`

- "Epoch closes reach storage in order" is in `storage_tests/features/system/role_commands.feature` (`@system @hosting`, run
  by `system_bdd`), its When as this change's wire-protocol text: "the system group's sequencer closes epochs 1 to 5 and
  storage is connected". A hosting storage process starts; a role connection as storage subscribes from 0 and reads epochs 1
  to 5 in order. `system_bdd`: 7 scenarios, 5.1 s.
- "The frontend answers a partition's lease and its placement" is removed: no specification holds it, and the system group's
  answer to a Lease is `common_system`'s "A lease is answered from a rising mark"
  (`features/client-endpoint/owner.feature`), its Placement storage_tests' "The hosting instance serves the books".
- Their bindings in `frontend_tests/tests/bdd.rs` are removed (`frontend_alone`, `as_storage`, the four steps, the World's
  `epochs`, `leased` and `ranges`, and the imports they alone used).

### The gate

`make build` passes. `bazel test //... --test_tag_filters=-integration_test`: 179 of 179 pass, `chart_tests:bdd` among them
(9 scenarios, 18 steps; 0.2 s uncached in the gate run, 3.2 s on its first run with the sandbox cold). Clippy through the aspect over
`chart_tests:bdd`, `frontend_tests:bdd`, `storage_tests:system_bdd` and `ultragraphdb_tests:bdd`: none on a line of this task;
`ultragraphdb_tests` stops on three "this lint expectation is unfulfilled", `tests/lossy_link.rs:50` and `:126`, unchanged since
the base, and `tests/own_steps.rs:35`, the file another agent is editing.

The integration targets, `--nocache_test_results`:

| Target | Result | Time |
|---|---|---|
| `storage_tests:system_bdd` | **passes**, 7 scenarios, the moved one among them | 7.0 s (5.1 s alone) |
| `frontend_tests:bdd` | 18 of 18 scenarios passed, then the binary did not exit until Bazel's timeout (966 s); run again alone: **passes**, 18 of 18 | 12.9 s |
| `ultragraphdb_tests:bdd`, the six scenarios alone (first run) | **passes**, 6 of 6 | 15.9 s |
| `ultragraphdb_tests:bdd`, whole | every run's summary printed, then TIMEOUT (1,335 s): 198/208, 10/10, 35/36, 36/38, 35/36 | |

The whole suite's failures are task 7.4's list (*The gate*, items 1 to 5: the group's quiet time, the Declare frame's cap, the
token above the open epoch, the silent partition under SIGSTOP of the host, the tenant's data drop); none is one of the six. Its
first run grew from 190/202 at task 7.4 to 198/208 by the six. In the gate's first whole run, before compute's deadline was
set, "A storage pod killed under writes keeps every acknowledged commit" failed with "no compute process answers" (*Found wanting*
1 below).

### Found wanting

1. **A restarted host leaves the frontend ready and its reads refused for up to half compute's deadline.** When the storage
   process hosting the system group restarts, the frontend reads Members again as its connection to storage closes (design
   D9) and finds no compute: the restarted group holds no Join from before, and compute sends its Join again only when its
   next ask, every half `join_deadline_ms` (30 s at the default of 60 s, `common_compute`'s `ASKS_PER_DEADLINE`), finds its
   address unlisted. Until then the frontend, `/ready` since the group answered, refuses every read with code 17 "no compute
   process answers". kubernetes-deployment, "A storage pod killed under writes keeps every acknowledged commit", says the
   commits read back "once the pod is ready". The scenario runs its compute at a deadline of 10 s and waits until Members
   lists compute; the cause is design D9 and D10's, the user's to decide (compute asking at once when its connection to the
   group closes, the frontend's `/ready` asking for a compute process, or the group keeping its members across a restart).
   Task 7.4's "A client's declaration of `name` a property is acknowledged, …, the storage process restarts, and a client
   writes" fails in the whole run with the same refusal.
2. **A binary hanging at its end.** `frontend_tests:bdd` and the whole `ultragraphdb_tests:bdd` each printed their last
   summary and then ran until Bazel's timeout, under a load average of 12 with another agent's builds; `frontend_tests` run
   again alone ended at once, and so do the six scenarios alone. Not root-caused: the last step of each is the drop of what
   started the processes, which waits for each to stop.
3. **"An upgrade keeps the Secrets" is not bound** (above); it needs a cluster.
