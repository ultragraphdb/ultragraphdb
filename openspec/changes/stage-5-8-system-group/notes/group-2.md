# Stage 5.8, group 2: the new settings, `ultragraphdb_common_service_config` and `ultragraphdb_common_role`

Base commit `885f5ec9`. This file records task 2.1, the Ultra Style lift of the two crates, and tasks 2.2 to 2.5, the five
phases: what the group adds, where a required setting is decided, the mapping of every corner case and law to its test,
the mutation runs, and what was found wanting. Group 2 adds and removes nothing else (design D2, "The new settings
arrive first and the old leave last"): `frontend.role_address`, `frontend.data_dir`, `cluster.frontend_address` and
`common_role::frontend_address` stay as they were.

## Task 2.1: the audit against `openspec/protocols/ULTRA_STYLE.md`

Scope: all of `src/` of both crates at the base, `ultragraphdb_common_service_config` (23 files, 1,238 lines) and
`ultragraphdb_common_role` (21 files, 1,482 lines). Every `file:line` below is as at `885f5ec9`.

How it was measured, as group 1 measured: Clippy under Bazel through the aspect, which takes the workspace's lint table,
with the lints Ultra Style's table states that the workspace table does not hold added on the command line:

```
bazel build --aspects=@rules_rust//rust:defs.bzl%rust_clippy_aspect --output_groups=clippy_checks \
  --@rules_rust//rust/settings:clippy_flag=-Dclippy::indexing_slicing \
  --@rules_rust//rust/settings:clippy_flag=-Dclippy::cast_possible_truncation \
  --@rules_rust//rust/settings:clippy_flag=-Dclippy::must_use_candidate \
  --@rules_rust//rust/settings:clippy_flag=-Dclippy::missing_errors_doc \
  --@rules_rust//rust/settings:clippy_flag=-Dclippy::missing_panics_doc \
  //ultragraphdb_common/ultragraphdb_common_service_config:all //ultragraphdb_common/ultragraphdb_common_role:all
```

`aquery` shows each `--deny=clippy::…` of `[workspace.lints]` and the five `-D` flags on the action. The aspect was shown
to bite in library code: a probe `pub fn` holding an `unwrap` failed it with `clippy::unwrap_used`, and was removed. It
does not bite on an `unwrap` inside a `#[test]` function (a probe there passed), so test code was checked by search: no
`unwrap`, `expect` or `panic!` in either crate's tests. As group 1 found, the aspect reads rules_rust's `.clippy.toml`,
not the repository's, so rule 4 was measured by a script counting each function's body lines, comments and blank lines
left out.

The aspect was clean over both crates at the base and after the lift; the review found the gaps below.

### `ultragraphdb_common_service_config`

| Rule | Result | Evidence |
|---|---|---|
| 1, acyclic control flow | conforms | no `dyn`, recursion or stored callback; `SettingsReader::read` takes an `impl FnOnce` it calls at once (`types/settings_reader/mod.rs:163`) |
| 2, bounds | conforms | no `while` or `loop`; every iteration is over the document's tables and keys, which `ultragraphdb_tool_toml` bounds at 256 and 4,096 |
| 3, planes | **gap, closed** | no module stated its plane. `src/lib.rs` now states the crate is control plane alone and allocates as it reads |
| 4, 70 lines | conforms | longest body under 30 lines |
| 5, invariants | conforms | no run-time `assert!` under `src/`; the crate holds no state past one reading |
| 6, smallest scope | conforms | `clippy::shadow_unrelated` clean |
| 7, results and panics | conforms | lint-clean; `let _config` (`types/service_config/mod.rs:59`) drops a `Result` that a collecting reader makes `Ok` on every input, as its comment states; `utils/partition_address.rs:19-21` adds with `checked_add` and subtracts with `saturating_sub` |
| 8, macros, `cfg` | conforms | none |
| 9, ownership | conforms | no `unsafe`, `Arc`, cell or lock |
| 10, zero warnings | conforms | aspect clean; no `#[allow]` or `#[expect]` |
| Types: `#[must_use]` | **gap, closed** | `ServiceConfigError::unknown_table` (`errors/service_config_error/mod.rs:54`) returned a value with none, unlike its two siblings |
| Types: exhaustive matches | conforms | the one `_` arm, `types/endpoint_table/mod.rs:127`, is over a tuple of `Option`s |
| Comments | **gap, closed** | `src/lib.rs:9-16` stated what the crate does not do: an `ULTRAGRAPHDB_<TABLE>_<KEY>` variable overriding the file and "the environment is passed in as data". Nothing in the crate reads the environment; `ServiceConfig::parse` takes the file's bytes alone. The paragraph now says what it reads |
| Naming | conforms | units and qualifiers last: `READ_RETAIN_MS_DEFAULT`, `HELD_BYTES_DEFAULT`, `block_size_max`, `epoch_ms_min` |
| Errors as the crate's type | left open | `partition_address` returns `Result<String, String>` (`utils/partition_address.rs:13`); other crates name it, and its signature keeps its form (design D2) |

### `ultragraphdb_common_role`

| Rule | Result | Evidence |
|---|---|---|
| 1, acyclic control flow | **gap, left open** | `Server` stores a function pointer, `welcome: fn(Welcome) -> Welcome` (`utils/serve.rs:54`), set by the public `Server::with_welcome` (`:103`), which `ultragraphdb_common_compute`'s tests and this crate's call. Replacing it changes a public signature, which this group does not. `types/role_client/mod.rs:175` takes `&dyn Display` in a closure, an exception its comment states |
| 2, bounds | **gap, closed** | the `loop` of `next_frame` (`utils/serve.rs:261`) did not say it is an event loop or what ends it; it now does, as `run`'s `while` (`:217`) did. `retry_until` ends at its deadline or a stop; `finish_open` counts its looks; `authenticate` takes `EXCHANGE_STEPS` |
| 3, planes | **gap, closed** | no module stated its plane. `src/lib.rs` now states that `serve_connection` and `RoleClient`'s requests carry frames, each read into a pooled buffer sized at the connection's start and each sent as the `Vec` the wire's builders return (group 1's *Left open*, 1), and that the settings, the secret, dialling and joining are control plane |
| 4, 70 lines | conforms | longest body `next_frame`, 53 lines (`utils/serve.rs:241`) |
| 5, invariants | conforms | no run-time `assert!` under `src/` |
| 6, smallest scope | conforms | `clippy::shadow_unrelated` clean |
| 7, panics on input | **gap, closed** | `retry_until` added the cluster's deadline to the clock, `now() + cluster.deadline()` (`utils/joining_role.rs:62`): `join_deadline_ms` is a setting, and one past what an `Instant` can hold panicked ("overflow when adding duration to instant"). It now adds with `checked_add`, and a deadline the clock cannot name never passes; the count of attempts beside it (`:65`) saturates. Test `joining_role_tests::test_a_deadline_past_the_clock_never_passes` was seen to fail with that panic on the old line, by a temporary edit, before it passed |
| 7, other counters | conforms | `RoleClient`'s `stream` (`types/role_client/mod.rs:100`, `:125`, `u64`) and `sent` (`:203`, `u128`) count one per frame of one connection; `ParkedBytes` saturates |
| 8, macros, `cfg` | conforms | the one `cfg`, `cfg(loom)` in `utils/sync.rs`, is stated there |
| 9, ownership | conforms | each `Arc` of an atomic (`types/connection_bound.rs:9-11`, `types/open_requests.rs:16-18`) and the `Rc` of cells of `ParkedBytes` (`types/parked_bytes.rs:11-14`) carries its reason |
| 10, zero warnings | conforms | aspect clean; the two `#[expect(clippy::disallowed_methods)]` (`utils/joining_role.rs:18`, `utils/serve.rs:35`) carry their reasons and are left as they are |
| Types: casts | conforms | `types/connection_bound.rs:41` and `types/role_client/mod.rs:73` state why they are lossless |
| Types: exhaustive matches | conforms | `clippy::wildcard_enum_match_arm` clean |
| Concurrency: cancellation | conforms | every `async fn` states what a dropped future leaves, or its type does (`RoleClient`, `RoleReader`, `RoleWriter`) |
| Division states its rounding | **gap, closed** | `utils/finish_open.rs:26` divided a stop's limit by its pause without saying it rounds down; it now says so, and why down |
| Naming, qualifiers last | left open | `PARTITION_INITIAL_CREDIT` and `COMPUTE_WELCOME_CREDIT` (`types/role_secret.rs:19`, `:22`) put the qualifier first; other crates name them |
| Loom (`tdd-protocol.md`) | left open | `OpenRequests` and `OpenFrame` share an `Arc<AtomicUsize>` through std's types, not `utils/sync.rs`, and have no Loom model: `openspec/notes/Loom-List.md`, row 14, "To do". Outside Ultra Style's rules and outside this group's changes |

### The lift and its evidence

Small and local in both crates: 5 lines of documentation and one attribute in `common_service_config`; in `common_role`
one arithmetic fix of three lines, two comments, the planes paragraph and one test. So the group made the lifts and went
on (task 2.1). They change nothing a caller sees except that a deadline past the clock no longer panics.

| Target | Before, at `885f5ec9` | After the lift |
|---|---|---|
| `common_service_config:ultragraphdb_common_service_config_tests` | 18 passed | 18 passed |
| `common_service_config:bdd` | 2 scenarios, 4 steps passed | 2 scenarios, 4 steps passed |
| `common_role:ultragraphdb_common_role_tests` | 40 passed | 41 passed (the deadline test) |
| `common_role:bdd` | 1 scenario, 2 steps passed | 1 scenario, 2 steps passed |
| `common_role:loom` | 3 passed | 3 passed |
| Clippy aspect, the workspace table and the five lints | clean | clean (run after phase 4, over the lift and the group's code together) |
| `make build` | | passes |

Each lift's lines are their own hunks: `common_service_config`'s in `src/lib.rs` (the paragraph on what the crate reads,
the plane) and `errors/service_config_error/mod.rs` (`#[must_use]`); `common_role`'s in `src/utils/joining_role.rs`
(`retry_until`), `src/utils/serve.rs`, `src/utils/finish_open.rs`, `src/lib.rs` (the planes paragraph) and the one test in
`tests/utils/joining_role_tests.rs`.

## Tasks 2.2 to 2.5: the five phases

Targets: `:ultragraphdb_common_service_config_tests` (unit), `:property` (new, tagged `property-test`) and `:bdd` of
`//ultragraphdb_common/ultragraphdb_common_service_config`; `:ultragraphdb_common_role_tests`, `:property` (new) and
`:bdd` of `//ultragraphdb_common/ultragraphdb_common_role`.

### Where the settings are decided

- **Which tables a file may hold, and whose.** `ServiceConfig` reads every table it holds for any file, whatever role
  reads it: `SettingsReader` refuses a table outside `TABLES` and nothing by role. So `[metadata]` and `[sequencer]`
  were already read in a storage instance's file, and nothing is added for them; the test
  `service_config_tests::test_the_moved_tables_read_in_a_storage_file_with_and_without_a_system_table` (C2.11) and law
  L2.4 state it. Refusing the two tables on an instance not hosting is the config service's System check, group 7's.
- **Where "required" is decided.** Two places: the role at its start, for its own role's settings (`RoleCluster::read`
  and `frontend_address` in `common_role`, `storage.role_address` and `storage.data_dir` in the storage binary's
  `service.rs`, `observability.address` in `RoleStart::from_settings`), and the config service's Required check
  (`ultragraphdb_common_config_check`, `utils/cross_checks.rs`, by role). `ServiceConfig` decided nothing required,
  except one rule of a table's own settings taken together: `EndpointTable::security` refuses half a certificate pair.
  A `[system]` table's two settings are required by the table, not by a role: the table's presence is what makes an
  instance the host (system-group, "The table SHALL name `system.role_address` … and `system.data_dir`"), and a table
  without them means nothing whoever reads the file. So the rule sits with the table, in `ServiceConfig`, as the
  endpoint's rule does, and is checked when the file is read: `SystemTable` holds both settings as `&str`, a table
  lacking either is refused with the existing `ServiceConfigError::missing` naming it, and `problems` lists each absent
  one. Every role's start, the config service's Parsing check and the storage binary of group 4 therefore see one answer,
  and group 4 needs no check of its own. The cluster's `system_address` stays a role's requirement, decided as
  `frontend_address` is: `common_role::system_address` refuses its absence with `RoleError::configuration` naming
  `cluster.system_address`.

### Phase 1: the surface

`SystemTable` (`src/types/system_table.rs`), its getters `role_address` and `data_dir`; `ServiceConfig::system`;
`ClusterTable::system_address`; `common_role::system_address`. Each new public body was a bare `unimplemented!()`, and
`SystemTable::read` was stubbed at its new branch alone, a file holding `[system]`, so every file the binaries run on
read as before; `system` joined `TABLES` and `cluster.system_address` was read into its field, since without them the
new tests would fail as an unknown table or an unread key and not on the stub. `SettingsReader::holds`, the stub's
condition, was written then. `make build` passed on the surface.

### Phase 2: the suite, observed failing

Run on the phase-1 surface, `bazel test //ultragraphdb_common/ultragraphdb_common_service_config:all
//ultragraphdb_common/ultragraphdb_common_role:all`:

| Target | Run | Failed | Passed |
|---|---|---|---|
| `common_service_config` unit | 31 tests | 13, each the `not implemented` panic | 18, the tests the crate had |
| `common_service_config` property | 5 tests | 5, each the `not implemented` panic | 0 |
| `common_service_config` bdd | 4 scenarios, 6 steps run | 2 scenarios, each at its `When`, the `not implemented` panic | 2, the scenarios the crate had |
| `common_role` unit | 45 tests | 4, each the `not implemented` panic | 41, the tests the crate had and the lift's |
| `common_role` property | 1 test | 1, the `not implemented` panic | 0 |
| `common_role` bdd | 2 scenarios | 1, the `not implemented` panic | 1, the scenario the crate had |
| `common_role` loom | 3 | 0 | 3 |

The panics were at `system_table.rs:24` (the stubbed branch), `service_config/mod.rs:151` (`system`),
`cluster_table.rs:53` (`system_address`) and `joining_role.rs:50` (`common_role::system_address`), and nowhere else.

The scenarios:

- system-group, "A system table without its directory stops the start", verbatim in
  `ultragraphdb_common_role/features/system-group/system_table.feature`, bound in `tests/bdd.rs` through
  `RoleStart::from_settings`, the start every role binary makes on its settings: the refusal carries exit code 78 and
  names `system.data_dir`. **The process's exit is group 4's**: the storage binary returning that code from `main` is
  bound over the built binary in `ultragraphdb_storage_tests` (tasks.md 4.2, the storage parts of "Configuration is
  explicit and validated at start"). The settings half is also a unit test, C2.3.
- role-binaries, "Configuration is explicit and validated at start", a storage instance's three tables: the requirement
  holds no scenario naming them (its four are a misspelt `[storage]` key, a missing `storage.data_dir`, a missing
  config-service file and the data layout, the first two bound over the storage binary in `ultragraphdb_storage_tests`
  and the last group 7's), so `ultragraphdb_common_service_config/features/role-binaries/storage_tables.feature` states
  the requirement's sentences for them in two scenarios, "A hosting storage instance's tables are read" and "An unknown
  setting in the system table stops the start", and says so in its description. See *Found wanting*, 1.

### The corner cases and their tests

Unit tests; `system_table_tests`, `cluster_table_tests` and `service_config/service_config_tests` are under
`ultragraphdb_common_service_config/tests/types/`, `joining_role_tests` under `ultragraphdb_common_role/tests/utils/`.

| Case | Test |
|---|---|
| C2.1 | `system_table_tests::test_a_storage_file_without_a_system_table_hosts_no_group` |
| C2.2 | `system_table_tests::test_a_system_table_with_both_settings_reads_as_written` |
| C2.3 | `system_table_tests::test_a_system_table_without_its_directory_is_refused_naming_it`; exit 78 of the start: the `common_role` scenario above |
| C2.4 | `system_table_tests::test_a_system_table_without_its_address_is_refused_naming_it` |
| C2.5 | `system_table_tests::test_an_empty_system_table_is_refused_naming_both` |
| C2.6 | `system_table_tests::test_an_unknown_key_in_the_system_table_is_refused_naming_it` |
| C2.7 | `system_table_tests::test_a_system_role_address_reads_on_either_carrier_as_written`; for `cluster.system_address`, `joining_role_tests::test_a_system_address_that_is_no_address_is_refused_naming_it`. See *Found wanting*, 2 |
| C2.8 | `cluster_table_tests::test_the_system_address_reads_in_every_role_file_on_either_carrier`; `joining_role_tests::test_the_system_address_is_the_setting_for_every_role` |
| C2.9 | `cluster_table_tests::test_the_frontend_and_system_addresses_read_side_by_side`; `joining_role_tests::test_the_frontend_and_system_addresses_are_each_their_own` |
| C2.10 | `service_config_tests::test_the_settings_in_use_read_as_before_with_none_of_the_new`, beside the crate's two tests of every setting in use, unchanged |
| C2.11 | `service_config_tests::test_the_moved_tables_read_in_a_storage_file_with_and_without_a_system_table` |

Beside them: `cluster_table_tests::test_an_absent_system_address_reads_as_absent`,
`joining_role_tests::test_an_absent_system_address_is_refused_naming_it` and
`system_table_tests::test_a_system_setting_of_the_wrong_kind_is_refused`. All 11 cases have a test.

### The laws and their property tests

| Law | Test |
|---|---|
| L2.1 | `common_service_config` `property`, `types/system_table_property_tests::test_a_drawn_system_table_reads_back_as_drawn` |
| L2.2 | `types/system_table_property_tests::test_a_system_table_lacking_a_setting_is_refused_naming_each_absent` |
| L2.3 | `types/service_config/service_config_property_tests::test_the_settings_in_use_read_alike_with_and_without_the_new` |
| L2.4 | `types/service_config/service_config_property_tests::test_the_moved_tables_read_alike_in_a_storage_and_a_frontend_file` |
| L2.5 | `common_role` `property`, `utils/joining_role_property_tests::test_the_system_address_is_the_address_drawn` |
| L2.6 | `types/system_table_property_tests::test_a_system_table_with_one_more_key_is_refused_naming_it` |

Each expects the values it drew and wrote into the file, and the setting names as the specification spells them. All 6
laws have a property test.

### Every error path through the public API

No variant is added. The new paths: `ServiceConfigErrorEnum::Missing` from a `[system]` table lacking a setting (C2.3 to
C2.5, L2.2); `Toml(UnreadKey)` for a key the table does not take (C2.6, L2.6); `Toml(WrongKind)` for a `[system]` value
of the wrong kind (`test_a_system_setting_of_the_wrong_kind_is_refused`); `RoleError::configuration` (exit 78) from `system_address` for an absent and an
unparsable `cluster.system_address` (C2.7, and the absent test). Each test asserts the variant or the exit code and the
name.

### Phase 3: the suite's inputs against the enumeration

No test pins a value only where two quantities coincide: `system.role_address` and `storage.role_address` differ in
every file a test writes (`:7100` and `:7200`), as do `system.data_dir` and `storage.data_dir`, and
`cluster.frontend_address` and `cluster.system_address`, so a read swapping them fails; the `[metadata]` and
`[sequencer]` values of C2.11 are seven distinct numbers; an absent `system_address` is tested beside a present
`frontend_address`, so a fall-back to it fails.

Added: to `system_table_tests::test_a_system_setting_of_the_wrong_kind_is_refused`, which phase 2 wrote for `parse`
alone, the list of problems: a value of the wrong kind is one a collecting reader reads as absent too, so the list must
hold its refusal and no `Missing` beside it. Its first form also asserted that the list holds one problem; phase 4
showed that the list names a value of the wrong kind twice, as `WrongKind` and as unread, for every setting of every
table, as it did before this group (`TomlDocument` leaves a value of another kind untaken). That assertion asked more
than the requirement and than the crate does anywhere, so it was taken out, and the test guards what it was added for:
no `Missing`.

### Phase 4: the implementation

`SettingsReader::required_string`, a string setting whose absence is refused as `ServiceConfigError::missing` and, when
collecting, recorded; `SystemTable::read` reading both settings when the file holds the table; the getters;
`common_role::system_address` as `frontend_address` is, its refusal naming `cluster.system_address` and the text it
holds. The suite turned green with no test changed but the one of phase 3.

| Target | Before the group | After |
|---|---|---|
| `common_service_config` unit | 18 | 31 |
| `common_service_config` property | none | 5 |
| `common_service_config` bdd | 2 scenarios, 4 steps | 4 scenarios, 8 steps |
| `common_role` unit | 40 | 45 |
| `common_role` property | none | 1 |
| `common_role` bdd | 1 scenario, 2 steps | 2 scenarios, 4 steps |
| `common_role` loom | 3 | 3 |

### Unchanged behaviour of what runs today

No profile, harness or binary names `[system]` or `cluster.system_address` (a search of every `.toml`, `.rs`, `.yaml`
and `.feature` outside `openspec/` finds them only in these two crates). For any other file `ServiceConfig::read` takes
the same settings into the same fields; the two differences are that `[system]` is no longer an unknown table and
`cluster.system_address` no longer an unread key. The shipped central files pass `ultragraphdb_config_cli` built from
this tree: `ultragraphdb/ultragraphdb_config/profiles/dev/targets.toml`, `profiles/dev-net/targets.toml` and
`deploy/helm/ultragraphdb/profile/kube/targets.toml`. One consequence for the config service until group 7: its
Parsing check reads files through `ServiceConfig::problems`, so a target holding a `[system]` table, refused before as
an unknown table, is now refused only when the table lacks a setting, and its System and System address checks do not
exist yet (design D2: "The config service's rules are therefore the old ones until the switch").

### Phase 5: mutation testing

`scripts/mutants.sh <crate> --diff 885f5ec9`, once per crate, run 1 of 3.

| Crate | Mutants | Caught | Unviable | Missed |
|---|---|---|---|---|
| `ultragraphdb_common_service_config` | 20 | 18 | 2 | 0 |
| `ultragraphdb_common_role` | 9 | 6 | 3 | 0 |

The two unviable mutants of `common_service_config` replace `SystemTable::read` and `ServiceConfig::system` with a
`Default` value, which `SystemTable` does not implement. The three of `common_role` replace `system_address`,
`retry_until` and `next_frame` with a `Default` value of `Address`, of a generic `T` and of `ReceivedFrame`, none of
which has one. Of `common_role`'s caught mutants, `retry_until`'s `>=` turned `<` is the line the lift changed; no
mutant was generated for `checked_add` or `saturating_add`, method calls cargo-mutants does not replace. No survivor,
so no second run and no entry in `.cargo/mutants.toml`. The group decodes no bytes from a client, a peer or the disk,
so it writes no fuzz target.

## Found wanting

1. **role-binaries, "Configuration is explicit and validated at start", names no scenario for the tables a hosting
   storage instance holds.** tasks.md 2.2 asks for that requirement's scenarios for "a storage instance's `[system]`,
   `[metadata]` and `[sequencer]` tables"; the requirement's four scenarios (`specs/role-binaries/spec.md`, the
   requirement's `#### Scenario:` lines) name a misspelt `[storage]` key, a missing `storage.data_dir`, a missing
   config-service file and the data layout. The reading implemented: the requirement's own sentences ("a file naming any
   other SHALL be refused as an unknown setting", "A role SHALL refuse to start on an unknown setting … naming the
   setting") stated for the new tables as two scenarios in `ultragraphdb_common_service_config`, whose feature file
   says they are derived. The specification could carry them, or tasks.md 2.2 could name only system-group's scenario.
2. **C2.7, "one that is no address: refused naming the setting", names no reader for `system.role_address`.**
   `notes/corner-cases.md` C2.7 (source D 2) gives the outcome; D2 and system-group give no place that parses the
   setting before group 4. `ServiceConfig` checks a value's kind alone, for every address setting alike
   (`storage.role_address`, `frontend.role_address`); an address is parsed by the role that listens or dials there.
   The reading implemented: `cluster.system_address`, the one new address a group-2 function parses, is refused naming
   the setting by `common_role::system_address`; `system.role_address` reads as written here, and its refusal is the
   hosting storage instance's at its listen (group 4) and the config service's Addresses check (group 7).
3. **C2.5, "refused naming both", holds for the list of problems, not for a role's start.** `ServiceConfig::problems`
   names both absent settings; `ServiceConfig::parse`, which a role's start calls, stops at the first refusal, as it
   does for every refusal of every table, and names one. The scenario of system-group names one setting, and design D2
   says nothing of a start naming two.
4. **The config service names a `[system]` table's absent setting without its setting field until group 7.**
   `ultragraphdb_common_config_check`'s `describe` (`src/utils/check_target.rs:151`) gives a `Missing` refusal no
   setting of its own, only its message, since no file refusal named one before. The defect still names the instance,
   the file and, in its text, `system.data_dir`. Group 7's System check reports the same absence with its setting.
