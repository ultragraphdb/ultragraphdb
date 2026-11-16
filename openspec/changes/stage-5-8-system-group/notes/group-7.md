# Stage 5.8, group 7: the switch, `ultragraphdb_common_config_check` and the settings of every binary and harness

Base commit `4c0e4a61`. This file records task 7.1, the Ultra Style lift of `ultragraphdb_common_config_check`, and tasks
7.2 (the config-service part), 7.3 and 7.4: the checks System, System address, Data and Fixed; the old settings and
`common_role::frontend_address` removed; the profiles, the chart's profile, `scripts/bench_cluster.sh` and every test and
harness that writes settings brought to the system group's settings. Task 7.5 and the rest of 7.2 are not this run's
(*Left for 7.5*). Counts are those Bazel's test logs report.

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
5. **"A tenant's data drop keeps its catalog"**: refused (2, 5), a conflict of the catalog version. Not root-caused; the
   likely cause is design D14: the Drop names the version its endpoint core's view holds right after the declaration, before
   that version takes effect at the partition's round. For groups 4 and 5.
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
