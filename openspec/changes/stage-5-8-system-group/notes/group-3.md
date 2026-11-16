# Stage 5.8, group 3: the system group, `ultragraphdb_common_system`

Base commit `e7545937`. This file records the first part of group 3, tasks 3.1 to 3.3, which change no behaviour: task
3.1, the Ultra Style lift of `ultragraphdb_common_frontend`; task 3.2, the cut, in three steps; and task 3.3, the move
into the new crate `ultragraphdb_common_system`. Each step lists the files it changed and the test counts before and
after it, so each can be committed on its own. Tasks 3.4 to 3.8 are a later run.

Counts are those Bazel's test logs report: a unit target's `test result`, a BDD target's scenarios and steps.

## Task 3.1: the audit against `openspec/protocols/ULTRA_STYLE.md`

Scope: all of `src/` of `ultragraphdb_common_frontend` at the base, 65 files, 9,757 lines, 449 functions. Every
`file:line` below is as at `e7545937`.

How it was measured, as groups 1 and 2 measured: Clippy under Bazel through the aspect, which takes the workspace's lint
table, with the lints Ultra Style's table states that the workspace table does not hold added on the command line:

```
bazel build --aspects=@rules_rust//rust:defs.bzl%rust_clippy_aspect --output_groups=clippy_checks \
  --@rules_rust//rust/settings:clippy_flag=-Dclippy::indexing_slicing \
  --@rules_rust//rust/settings:clippy_flag=-Dclippy::cast_possible_truncation \
  --@rules_rust//rust/settings:clippy_flag=-Dclippy::must_use_candidate \
  --@rules_rust//rust/settings:clippy_flag=-Dclippy::missing_errors_doc \
  --@rules_rust//rust/settings:clippy_flag=-Dclippy::missing_panics_doc \
  --keep_going //ultragraphdb_common/ultragraphdb_common_frontend:all
```

The library and the unit test target are clean. The `bdd` target reports three `clippy::indexing_slicing` in
`tests/bdd.rs` (`:139`, `:350` twice) and one unfulfilled `#[expect(clippy::disallowed_methods)]` (`tests/bdd.rs:258`):
both are the aspect's limit groups 1 and 2 recorded, since it reads rules_rust's `.clippy.toml` and not the repository's,
which allows indexing in tests and holds the disallowed methods. Rule 4 was measured by a script counting each
function's body lines, comments and blank lines left out.

| Rule | Result | Evidence |
|---|---|---|
| 1, acyclic control flow | conforms | no recursion and no stored callback; the two `&dyn Display` closures carry their reason (`utils/owner_thread.rs:187`, `utils_test/role_double.rs:56`); `Owner::run` takes its clock as `impl Fn`, static dispatch |
| 2, bounds | **gap, closed** | six loops did not say what ends them: `Owner::run`'s event loop (`types/owner/owner_turn.rs:40`), `learn_missed`'s walk of the durable closes (`:178`), `RoleSession::next_push` (`types/role_session.rs:243`, at most two turns), `connect_back`'s retry (`:371`, ends at the cluster's deadline), `ClientSession::next_push` (`types/client_session/client_session_session.rs:73`) and `CoreJoins::follow` (`types/core_joins.rs:26`). Each now says so. The loops of `role_link/role_link_tasks.rs` (`:68`, `:99`) and `utils/declarations.rs:259` said so already; `take_turn` is bounded by `InboxTurn`, `while_syncing` by `ASKS_PER_TURN_MAX` |
| 2, queues | **gap, left open** | `JoinFanOut`'s per-core channels are unbounded (`types/join_fan_out.rs:28`) with no stated bound; task 5.5 removes them with the Join channel of task 3.2. `OwnerHandle`'s channel states its bound (`types/owner_handle.rs:24-26`) |
| 3, planes | **gap, closed** | no module stated a plane. `src/lib.rs` now states that a client session's requests, the owner's asks on a commit's path and the role links carry frames, the data plane, a frame sent being the `Vec` the wire's builders return and an ask allocating its reply's channel; and that authentication, the throttle, joining, the owner's start, snapshots and the building of a catalog version are control plane |
| 4, 70 lines | conforms | longest bodies 67 (`types/owner/owner_answer.rs:30`, `take`), 65 (`utils/term_rules.rs:283`, `check_change`; `types/client_session/client_session_requests.rs:57`, `request`); 0 of 449 above 70 |
| 5, invariants | conforms | no run-time `assert!` under `src/` |
| 6, smallest scope | conforms | `clippy::shadow_unrelated` clean |
| 7, results and panics | conforms | lint-clean; the `.ok()`s fold a refusal into an `Option` the caller reads as absent (`types/owner_handle.rs:44-45`, an owner that stopped; `types/role_links.rs:101`, an address that does not parse), none drops an error the caller should see |
| 8, macros, `cfg` | conforms | no `macro_rules!`; the only `cfg` is `feature = "test-utils"` (`src/lib.rs:16`, `:44-48`) |
| 9, ownership | **gap, closed** | `AskedCredentials` (`types/asked_credentials.rs:14`) shared an `Rc<RefCell<…>>` without saying why; it now does. Every other `Rc` and cell states its reason: `EndpointCore` (`types/endpoint_core.rs:18-20`), `RoleLinks` (`types/role_links.rs:29-30`), `RoleLink` (`types/role_link/mod.rs:59-62`), `RouteSink` (`types/route_sink.rs:21`), `OneFramePhases` (`types/one_frame_phases/mod.rs:23`) |
| 10, zero warnings | conforms | aspect clean over the library and the unit tests; the five `#[expect(clippy::disallowed_methods)]` (`types/role_session.rs:353`, `types/client_session/mod.rs:228`, `utils/owner_thread.rs:88`, `:98`, `:135`) carry their reasons and are fulfilled under the repository's `clippy.toml` |
| Types: casts | conforms | every `as` states why it is lossless (`types/client_session/client_session_requests.rs:305`, `client_session_reads.rs:262`, `client_session_writes.rs:284`, `:296`, `:318`, `client_session_one_frame.rs:227`, `:298`, `client_session_bind.rs:320`, `client_session_prepared.rs:121`, `utils/declarations.rs:158`, `:212`) |
| Types: exhaustive matches, `#[must_use]`, `# Errors` | conforms | `clippy::wildcard_enum_match_arm`, `must_use_candidate`, `missing_errors_doc` clean |
| Division | conforms | the four `%` are over a count checked non-zero (`types/role_links.rs:128`, `:134`, `:202`; `client_session_one_frame.rs:227`) |

### The lift and its evidence

Small and local: 14 lines of comments and documentation added and one changed, in 6 files, no code. So the step made the lift and went on.

Files: `src/lib.rs`, `src/types/asked_credentials.rs`, `src/types/client_session/client_session_session.rs`,
`src/types/core_joins.rs`, `src/types/owner/owner_turn.rs`, `src/types/role_session.rs`, all under
`ultragraphdb_common/ultragraphdb_common_frontend/`.

| Target | Before, at `e7545937` | After the lift |
|---|---|---|
| `common_frontend:ultragraphdb_common_frontend_tests` | 94 passed | 94 passed |
| `common_frontend:bdd` | 135 scenarios, 270 steps passed | 135 scenarios, 270 steps passed |
| Clippy aspect, the workspace table and the five lints | library and unit tests clean | library and unit tests clean |
| `make build` | | passes |

## Task 3.2: the cut

Three steps in `common_frontend`, each changing no behaviour (design D1, "The cut comes before the move"), each gated
by `make build` and the crate's suite.

### Step (a): the throttle leaves the owner

`AuthThrottle` and `LoginLimits` leave `Owner` and `OwnerStart`. The first endpoint core holds the throttle and every
core asks it by message (design D8), as each asked the owner:

- `LoginAsked` is now the throttle's: `Delay` (the wait an attempt from a source makes, which `Verifier` returned
  beside the verifier), `Admit` and `Outcome`, each as the owner answered it.
- `ThrottleHandle` is the way to it, as `OwnerHandle` is to the owner: `delay`, `admits`, `outcome`, over an unbounded
  channel whose bound is the connections', as `OwnerHandle`'s is. `AuthThrottle::serve` gives the handle and the run,
  which takes one ask at a time and ends once every handle is gone; its ban is logged with the text, target and fields
  the owner logged it with.
- The owner's `Asked::Login` is now `Asked::Verifier`, answered with the principal and its verifier alone;
  `OwnerHandle::verifier(name)` returns that, `OwnerHandle::admits` and `login_outcome` are gone, and an ask of the
  verifier waits for a sync's end as `Login` did.
- `EndpointCore::new` takes the `ThrottleHandle` beside the `OwnerHandle`; `EndpointCore::logins` returns it and
  `admits` asks it. `ClientSession` asks the owner for the verifier and the throttle for the wait at once, and tells
  the throttle each outcome.
- The frontend binary serves the throttle, of the settings' `login_limits` as before, on core 0's runtime when it binds
  the client endpoint (`Clients::bind`), and hands every core its handle. `OwnerStart::with_logins` and
  `OwnerStart::logins` are gone.

What a client sees is unchanged: the same counts by the same rules, the same delays and bans and the same audit line. Two
timings differ, neither a rule: `Admit` and `Outcome` no longer wait behind an owner's sync, since they reach no log,
and the wait and the verifier are asked at once, of the throttle and of the owner, where one ask of the owner
answered both.

Test edits forced by the cut, no assertion changed: the five calls of `verifier(name, None).await.0` read
`verifier(name).await` (`tests/bdd.rs`, `tests/endpoint_steps_requests.rs`, `tests/types/owner/mod.rs`,
`tests/types/owner_tests.rs`, `tests/types/role_session_tests.rs`); the endpoints of `tests/types/client_session_tests.rs`
and `tests/endpoint_stage.rs` serve a throttle of the default limits, and the ban test hands its endpoint the limits
`LoginLimits::new(1, 1, ban)` it handed the owner. `auth_throttle_tests.rs` and every feature file are unchanged. Two
tests were added for the new handle, `tests/types/throttle_handle_tests.rs`: a served throttle counts what it is told,
in order, and a stopped one admits no one.

Files: under `ultragraphdb_common/ultragraphdb_common_frontend/`, `src/lib.rs`, `src/types/mod.rs`,
`src/types/asked.rs`, `src/types/auth_throttle/mod.rs`, `src/types/auth_throttle/auth_throttle_serve.rs` (new),
`src/types/client_session/client_session_session.rs`, `src/types/endpoint_core.rs`, `src/types/login_asked.rs`,
`src/types/owner/mod.rs`, `src/types/owner/owner_answer.rs`, `src/types/owner/owner_syncing.rs`,
`src/types/owner_handle.rs`, `src/types/owner_start.rs`, `src/types/throttle_handle.rs` (new), `tests/bdd.rs`,
`tests/endpoint_stage.rs`, `tests/endpoint_steps_requests.rs`, `tests/types/client_session_tests.rs`,
`tests/types/mod.rs`, `tests/types/owner/mod.rs`, `tests/types/owner_tests.rs`, `tests/types/role_session_tests.rs`,
`tests/types/throttle_handle_tests.rs` (new); and `ultragraphdb/ultragraphdb_frontend/src/service.rs`. The 3.1 lines
in `src/lib.rs`, `src/types/owner/owner_turn.rs` and the others are hunks of their own.

| Target | Before | After |
|---|---|---|
| `common_frontend:ultragraphdb_common_frontend_tests` | 94 passed | 96 passed (the two throttle handle tests) |
| `common_frontend:bdd` | 135 scenarios, 270 steps passed | 135 scenarios, 270 steps passed |
| `make build` | passes | passes |

### Step (b): a Join is told by a channel

`RoleSession` registers a Join with its owner, as before, and then sends it on a `futures::channel::mpsc` sender that
`RoleShared` holds, a concrete sender with no trait object or stored callback. `RoleShared` holds the cluster, the way to
the owner and that sender alone: `RoleShared::new(cluster, owner, joins)`, `with_cores` gone. What `RoleSession::join`
did after the owner's answer is the frontend's new `JoinedRoles`, which `JoinedRoles::new(cluster, links, readiness)`
returns with the sender, `with_cores(fan_out)` gives the other cores, and `follow` runs until every sender is gone: for
each Join in the order it was told, `RoleLinks::joined`, `JoinFanOut::send` and a detached `connect_back`, which dials
the role, hands the link to `RoleLinks::connected` and sets the readiness for a partition reached, with its retry, its
deadline and its log lines as they were. `connect_back` and its clock moved with their `#[expect]` from
`types/role_session.rs` to `types/joined_roles/mod.rs`. The frontend binary spawns `follow` on core 0's runtime beside
the role listener.

Every Join is taken to the links, the other cores and a connection back exactly as before; the one difference is when.
The links learn a Join once the core's task takes it from the channel, after the role session has built the Join's
answer, where they learned it before the answer was built. The two run on one thread, and the task takes the Join at the
session's next wait, so a request another connection had ready may be routed first, by the links as they were; design
D1 chooses this order. The channel is unbounded, as `JoinFanOut`'s are, and task 5.5 removes both.

Tests: the two tests of the connection back, `test_a_storage_join_is_connected_back_and_makes_the_frontend_ready` and
`test_a_role_refusing_past_the_deadline_is_given_up`, test what `JoinedRoles` does and moved unchanged in their
assertions from `tests/types/role_session_tests.rs` to `tests/types/joined_roles_tests.rs`, with copies of the four
helpers they use, their endpoint now passing its joins to a `JoinedRoles`. One test was added there,
`test_a_join_at_an_unparsed_address_is_not_connected_back`, since the Join at an address that does not parse in
`test_the_role_commands_are_answered_from_the_owner` no longer reaches a task that logs it. The other tests of
`role_session_tests.rs` build their `RoleShared` through `shared_of`, whose joins no task takes; `role_endpoint` lost the
readiness only the two moved tests read. The BDD stage (`tests/endpoint_stage.rs`) follows its joins with a
`JoinedRoles` over its links, so its scenarios see what joined as before. The BDD target and the unit target passed
three runs each, uncached.

Files: under `ultragraphdb_common/ultragraphdb_common_frontend/`, `src/lib.rs`, `src/types/mod.rs`,
`src/types/joined_roles/mod.rs` (new), `src/types/role_session.rs`, `tests/endpoint_stage.rs`, `tests/types/mod.rs`,
`tests/types/joined_roles_tests.rs` (new), `tests/types/role_session_tests.rs`;
`ultragraphdb/ultragraphdb_frontend/src/service.rs`; and `AGENTS.md`, the `common_frontend` row, one line naming
`ThrottleHandle` of step (a) and `JoinedRoles` of this step, so it goes with this step's commit.

| Target | Before | After |
|---|---|---|
| `common_frontend:ultragraphdb_common_frontend_tests` | 96 passed | 97 passed (two moved within the crate, one added) |
| `common_frontend:bdd` | 135 scenarios, 270 steps passed | 135 scenarios, 270 steps passed |
| `make build` | passes | passes |

### Step (c): `describe_of` and the three beside it move to `core_catalog`

#### The dependency check, first run

Checked first, as the step asks, by adding `ultragraphdb_protocol_wire = { workspace = true }` to
`ultragraphdb_core_catalog`'s `[dependencies]` and building `//ultragraphdb_core/ultragraphdb_core_catalog`. Bazel
refuses it at analysis, with two cycles:

```
ERROR: .../ultragraphdb_protocol_wire/BUILD.bazel:11:13: in rust_library rule
//ultragraphdb_protocol/ultragraphdb_protocol_wire:ultragraphdb_protocol_wire: cycle in dependency graph:
.-> //ultragraphdb_protocol/ultragraphdb_protocol_wire:ultragraphdb_protocol_wire
|   //ultragraphdb_store/ultragraphdb_store_quads:ultragraphdb_store_quads
|   //ultragraphdb_core/ultragraphdb_core_catalog:ultragraphdb_core_catalog
`-- //ultragraphdb_protocol/ultragraphdb_protocol_wire:ultragraphdb_protocol_wire
ERROR: .../ultragraphdb_core_catalog/BUILD.bazel:11:13: in rust_library rule
//ultragraphdb_core/ultragraphdb_core_catalog:ultragraphdb_core_catalog: cycle in dependency graph:
.-> //ultragraphdb_core/ultragraphdb_core_catalog:ultragraphdb_core_catalog
|   //ultragraphdb_protocol/ultragraphdb_protocol_wire:ultragraphdb_protocol_wire
|   //ultragraphdb_store/ultragraphdb_store_quads:ultragraphdb_store_quads
`-- //ultragraphdb_core/ultragraphdb_core_catalog:ultragraphdb_core_catalog
```

Why. Design D1 holds that the wire crate's tests depending on `core_catalog` close no cycle, since "they are targets of
their own beside its library". That holds of their `deps`, and not of the library's `aliases`:
`ultragraphdb_protocol/ultragraphdb_protocol_wire/BUILD.bazel:14` sets `aliases = aliases()`, and `aliases()` of
`@crates//:defs.bzl` returns the package's aliases of every dependency kind, dev-dependencies included. `aliases` is a
label-keyed attribute, so each key is an edge of the library target: `bazel query
'labels(deps, //ultragraphdb_protocol/ultragraphdb_protocol_wire:ultragraphdb_protocol_wire)'` lists its five normal
dependencies, while `deps(…, 1)` adds the eight dev-dependencies the aliases name (`core_catalog`, `core_mapping`,
`store_posting`, `store_quads`, `store_sim`, `tool_bdd`, `traits_blocks`, `traits_store`; `Cargo.toml:29-37`). With
`core_catalog` depending on the wire library, the wire library reaches `core_catalog` through its aliases, directly and
through `store_quads` (which depends on `core_catalog`, `ultragraphdb_store/ultragraphdb_store_quads/Cargo.toml:23`).

Cargo was not run, by the rule that builds and tests are Bazel's: a cycle through a dev-dependency is one Cargo accepts
by its documented rules, so the edge is Bazel's problem alone.

The probe was reverted: `core_catalog`'s `Cargo.toml` is as at the base. While it stood, something outside this run
(the IDE's `cargo metadata`, by the look of it) wrote the new edge into `Cargo.lock`'s entry for `core_catalog`; that one
line was taken out again by hand, so `Cargo.lock` is as at the base.

What would let the step go on, each a decision for the user, none made here:

1. The wire library's `aliases` restricted to its normal dependencies, in `ultragraphdb_protocol_wire/BUILD.bazel`, e.g.
   `aliases = {label: name for label, name in aliases().items() if label in all_crate_deps(normal = True)}`, or the
   attribute dropped where every alias equals the crate's own name. This touches the wire crate, which another agent
   holds during this run, so it was neither made nor tried. The same pattern stands in every package's `BUILD.bazel`;
   only the wire's matters here.
2. `describe_of` and the three beside it in a crate beneath both the owner and the endpoint other than `core_catalog`,
   which design D1 weighed and decided against (decided 2026-11-12).

Task 3.3 moves "what step (c) left in `utils/declarations.rs`", and the owner calls `describe_of` (`types/owner/
owner_catalog.rs:57`, `:73`), so `common_system` would name an item of `common_frontend` if the owner moved without it.
The first run therefore stopped here and reported.

#### The fix, and the check run again

The user took option 1: `ultragraphdb_protocol/ultragraphdb_protocol_wire/BUILD.bazel` sets no `aliases` on the library,
with a comment saying why, since the wire library renames no dependency. With it, `core_catalog`'s edge to the wire
analyses and builds: `bazel build //...` passes, and the wire's own targets (`ultragraphdb_protocol_wire_tests`, `bdd`,
`property`) pass. Cargo's `Cargo.lock` takes the one line the new edge needs, `"ultragraphdb_protocol_wire"` in
`ultragraphdb_core_catalog`'s dependencies, and no version.

#### The step

`describe_of`, `preorder`, `kinds` and `declared_predicate` moved verbatim from
`ultragraphdb_common_frontend/src/utils/declarations.rs` to the new `ultragraphdb_core_catalog/src/utils/describe_of.rs`;
`describe_of` is public there, exported at the root and carries `#[must_use]`, the other three are private.
`core_catalog` depends on `ultragraphdb_protocol_wire`; its crate documentation names `describe_of`, and its `AGENTS.md`
row names the dependency and the function. The owner's catalog code imports it from `core_catalog`. What stays in
`declarations.rs` is the building of a catalog version: `declaration_of`, `merged`, `tightens`, `restored`,
`version_bytes` and their helpers.

No test names the four directly: `describe_of` is tested through the owner's Describe and schema id
(`tests/types/owner/owner_catalog_tests.rs` and the frontend's BDD scenarios), so no test moved with it and none was
added. The clippy aspect with the five extra lints is clean over `describe_of.rs`; `core_catalog`'s other files hold
`must_use_candidate` and `missing_errors_doc` gaps of their own, outside this group's lifts.

Files: `ultragraphdb_core/ultragraphdb_core_catalog/Cargo.toml`, `src/lib.rs`, `src/utils/mod.rs`,
`src/utils/describe_of.rs` (new); `ultragraphdb_common/ultragraphdb_common_frontend/src/utils/declarations.rs`,
`src/types/owner/owner_catalog.rs`; `Cargo.lock` (one line); `AGENTS.md` (the `core_catalog` row).

| Target | Before | After |
|---|---|---|
| `core_catalog:ultragraphdb_core_catalog_tests` | 57 passed | 57 passed |
| `core_catalog:bdd` | 5 scenarios, 10 steps passed | 5 scenarios, 10 steps passed |
| `common_frontend:ultragraphdb_common_frontend_tests` | 97 passed | 97 passed |
| `common_frontend:bdd` | 135 scenarios, 270 steps passed | 135 scenarios, 270 steps passed |
| `make build` | passes | passes |

Commit note: the two frontend files this step edited, `utils/declarations.rs` and `types/owner/owner_catalog.rs`, move
whole in task 3.3, so in the working tree this step's edits to them sit inside 3.3's renames. Steps (c) and 3.3 are
therefore committed together, or (c) is committed first from these notes' list with 3.3's moves made after it.

## Task 3.3: the move into `ultragraphdb_common_system`

The owner, its handle and asks, its start and thread, the snapshot file, its role session, the membership and what
step (c) left of `utils/declarations.rs` moved from `ultragraphdb_common_frontend` into the new crate
`ultragraphdb_common/ultragraphdb_common_system`, with their tests, features and test utilities. Every file that moved
whole moved with `git mv`, so `git log --follow` reads its history; the frontend files left are edited in place.

Names, by design D1's table: `Owner` is `SystemGroup`, `OwnerHandle` `SystemHandle`, `Asked` `SystemAsked`, `OwnerStart`
`SystemStart`, `start_owner` `start_system`, `RoleSession` `SystemSession`; `Membership`, `ReportAsked`, `DropStep` and
`TenantFacts` keep theirs. Three names D1's table does not give follow the same rule: `OwnerThread`, which `start_system`
returns, is `SystemThread`; `RoleShared`, the state `SystemSession` shares, is `SystemShared`; the test utility
`owner_run` is `system_run`. Two `Debug` names follow their types, `SystemGroup { … }` and `SystemThread { … }`, and the
one test reading the first (`system_group_tests.rs`, `test_an_owners_debug_form_names_its_membership_and_waits`) reads
`"SystemGroup {"`. Nothing else was renamed: the thread is still `frontend-owner`, its runtime still logs `core =
"owner"`, the refusals still say "the frontend's owner stopped", and no setting, exit code or wire byte changed. The
secrets fix of `e7545937` moved unchanged: `SystemThread`'s hand-written `Debug` prints the secret as `<redacted>`, and
`test_a_running_owner_never_prints_its_secret` moved with it. The five `#[expect(clippy::disallowed_methods)]` of the
moved code moved with it.

| From `common_frontend` | To `common_system` |
|---|---|
| `src/types/owner/mod.rs`, `owner_answer.rs`, `owner_catalog.rs`, `owner_check.rs`, `owner_syncing.rs`, `owner_turn.rs` | `src/types/system_group/mod.rs`, `system_group_answer.rs`, `system_group_catalog.rs`, `system_group_check.rs`, `system_group_syncing.rs`, `system_group_turn.rs` |
| `src/types/owner_handle.rs`, `asked.rs`, `owner_start.rs`, `role_session.rs` | `src/types/system_handle.rs`, `system_asked.rs`, `system_start.rs`, `system_session.rs` |
| `src/types/membership.rs`, `report_asked.rs`, `tenant_facts.rs` | the same names |
| `src/utils/owner_thread.rs`, `snapshot_file.rs`, `declarations.rs` | `src/utils/system_thread.rs`, `snapshot_file.rs`, `declarations.rs` |
| `src/utils_test/owner_run.rs` | `src/utils_test/system_run.rs` |
| `tests/types/owner/mod.rs` and its five `owner_*_tests.rs` | `tests/types/system_group/mod.rs` and five `system_group_*_tests.rs` |
| `tests/types/owner_tests.rs`, `role_session_tests.rs`, `membership_tests.rs`, `report_asked_tests.rs` | `tests/types/system_group_tests.rs`, `system_session_tests.rs`, `membership_tests.rs`, `report_asked_tests.rs` |
| `tests/utils/owner_thread_tests.rs` | `tests/utils/system_thread_tests.rs` |
| `tests/bdd.rs`, the owner's steps | `tests/bdd.rs`, with its 16 steps; the frontend's `tests/bdd.rs` is written anew: its step modules, `Steps { ran }` and `main` |
| `features/client-endpoint/owner.feature`, `features/partitions/placement.feature`, `features/transactions/sessions.feature` | the same paths |

New files of the crate, each with the repository's header: `Cargo.toml` (`[lints] workspace = true`, the `test-utils`
feature, `[[test]] unit` and `[[test]] bdd`), `BUILD.bazel` (the library with `crate_features = ["test-utils"]`, `doc`,
the unit `rust_test` rooted at `tests/mod.rs` tagged `unit-test`, the `bdd` target tagged `bdd-test` with the features
as data), `src/lib.rs` (what the crate is, and its planes), `src/types/mod.rs`, `src/utils/mod.rs`,
`src/utils_test/mod.rs`, `tests/mod.rs`, `tests/types/mod.rs`, `tests/utils/mod.rs`.

`common_system` names no item of `common_frontend` (`grep -rn common_frontend ultragraphdb_common/ultragraphdb_common_system`
finds nothing). `common_frontend` depends on `common_system` and imports `SystemHandle`, `TenantFacts` and `DropStep` from
it; `EndpointCore::owner` returns the `SystemHandle`. Its dependencies are otherwise unchanged: those the move left unused
in its `src/` (`traits_log`, `transactions_log`, `error_cluster`, `traits_catalog`; `cluster_metadata`,
`cluster_sequencer`, `core_catalog` and `common_service_config` are named by its tests alone) go in task 5.5, as
`tasks.md` has it. The frontend binary imports the moved items from `common_system` and depends on it;
`ultragraphdb_tool_bench`'s owner check (`src/utils/owner_run.rs`) depends on `common_system` in place of
`common_frontend`, which it no longer names. `Cargo.lock` takes the new package and the four new edges, and no version.
`AGENTS.md`: the new crate's row, the `common_frontend`, `ultragraphdb_frontend` and `ultragraphdb_tool_bench` rows
brought to it, and `common_system` among the users of `futures`, `getrandom`, `oxrdf` and `compio`.

Left on disk, not deleted: the three directories the moves emptied, `ultragraphdb_common_frontend/src/types/owner/`,
`tests/types/owner/` and `features/partitions/`. Git tracks none of them; removing them is the user's call.

The tests in the frontend that start the group (`tests/endpoint_stage.rs`, `tests/types/client_session_tests.rs`,
`tests/types/joined_roles_tests.rs`, `tests/tightening_steps.rs`) changed their imports and the renamed names alone.

| Target | Before the move | After the move |
|---|---|---|
| `common_frontend:ultragraphdb_common_frontend_tests` | 97 passed | 56 passed |
| `common_system:ultragraphdb_common_system_tests` | | 41 passed |
| Unit tests, sum | 97 | 97 |
| `common_frontend:bdd` | 135 scenarios, 270 steps passed | 127 scenarios, 254 steps passed |
| `common_system:bdd` | | 8 scenarios, 16 steps passed |
| BDD, sum | 135 scenarios, 270 steps | 135 scenarios, 270 steps |
| `ultragraphdb_tool_bench` targets | pass | pass |
| Clippy aspect, the workspace table and the five lints, over `common_system` | | library and unit tests clean; the `bdd` target reports the aspect's two known limits (`indexing_slicing` in a test, an `#[expect]` fulfilled under the repository's `clippy.toml`), as the frontend's did |
| `make build` | passes | passes |

## The gates

**Over 3.1, 3.2 (a) and 3.2 (b)**, before `e32306f5`: `make build` passed; `bazel test //...
--test_tag_filters=-integration_test` passed, 148 targets; `make test` failed one integration scenario, "The frontend
serves clients on every core" (`ultragraphdb_tests`, 201 of 202 in its first set), 15 runtimes logged against 16 CPUs.
That came of `fe27ebff`, an ancestor of the base, where the role's first runtime stopped logging `core=0`; the user's
commit `e32306f5` restored the line.

**At the end, over 3.2 (c) and 3.3**, on HEAD `e32306f5`:

| Gate | Result |
|---|---|
| `make build` | passes |
| `bazel test //... --test_tag_filters=-integration_test` | 150 targets (148 and the new crate's two), all pass |
| `make test` | passes in full: doc 20, unit 80, property 5, Loom 5 and BDD 40 targets; integration 7 of 7 |

Integration scenarios: `compute_tests:bdd` 88, `config_tests:bdd` 44, `frontend_tests:bdd` 20, `storage_tests:bdd` 98,
`supervisor_tests:bdd` 28, `supervisor_tests:image_bdd` 7, `ultragraphdb_tests:bdd` 202, 10, 36, 38 and 36; all passed.

## Tasks 3.4 to 3.8: the system group serves its commands

Base commit `2ab8acfb`; the mutation base `dbfd958e`, the commit before task 3.1. Counts are those Bazel's test logs report.

### What was built

`ultragraphdb_common_system`:

- `SystemSession` serves the system group's role address by `PROTOCOL.md` §11.12's endpoint table, by the connection's role, the
  SCRAM username its exchange named (read from the client-first-message in `Session::authenticating`, taken as the role at
  `authenticated`). A command the table does not have that role send is refused as `CommandNotServed` naming it, a connection
  error (§2.5 step 3, §2.6). New: Challenge, Verify, Session (pushed once durable, `Answer::Open`, so the Checks behind it on the
  connection are answered meanwhile), Check (`check_request` first), Ended (no answer; a body its rules refuse dropped
  unanswered), Declare of both kinds (pushed once the group answers, its reply the request's command header with `version` and
  `commit_version` filled and the request's sections), Defaults across frames. Changed: Catalog answered across frames within the
  requester's `max_frame` (from `welcomed`), a request of `version` 0 naming an epoch answered by `catalog_at`; Subscribe's
  `latest` checked with `SubscribeView::check_sender` and begun at the latest durable close; CloseRequest's `partition` by role
  (`partition` 0 from the frontend alone, 1 or above from storage alone, `FieldValue` `"partition"`, dropped unanswered as every
  CloseRequest refusal is). A dropped session tells the group its connection closed. The module became a folder:
  `system_session/mod.rs` (moved with `git mv`), `system_session_session.rs` (the `Session` impl), `system_session_frontend.rs`,
  `system_session_drop.rs`.
- `SessionMarks` (private, `types/session_marks.rs`): the marks per role connection, a closed connection's held for twice
  `transaction_ms` from the instant the group took its close, the quiet time of twice `transaction_ms`, half-open, from the instant
  the group's run begins (`SystemGroup::run`). It replaces the owner's `in_flight` map; the frontend's in-process marks
  (`begin_commit`, `begin_recall`, `commit_ended`) are marks of no connection and behave as before.
- `SystemGroup`: `system_group_sessions.rs` (the checks' outcomes 12, 21, 14, 1; Ended; a Session's refusals Error 1, code 7,
  code 14, the quiet time refusing one that would evict); `system_group_reads.rs` (Defaults, Catalog at an epoch through
  `settled_at`, Challenge and Verify through `ScramVerifier::sign` and `ScramVerifier::unknown` at `DEFAULT_ITERATIONS`); Check,
  Ended, a connection's open and close answered while a log syncs, from every command appended (`MetadataReads`); a declared
  version made durable in its own turn (`Durable::Offered`) so the next close assigns its epoch; Declare and Defaults of a tenant
  the group does not hold refused with code 7, `detail` the tenant; a Drop of everything removing the tenant's default-entry
  records (`MetadataGroup::remove_defaults`).
- `SystemHandle`: `connect`, `disconnect`, `session`, `check`, `ended`, `declare_version`, `drop_version`, `defaults`,
  `catalog_at`, `challenge`, `verify`, `latest_close`; `declare` and `drop_catalog` keep their signatures over the new two.
- `SystemStart::with_transaction_ms` and `transaction_ms`, 10,000 unless set (§11.9). `DropStep::Kept`, the step of a Drop of a
  graph or of a tenant's data.

`ultragraphdb_cluster_metadata`: `MetadataCommand::CATALOG_BYTES_MAX` is the wire's `CATALOG_BYTES_MAX`, 16,777,216 (design D13),
`ultragraphdb_protocol_wire` a normal dependency (it was a dev-dependency), a compile-time check that a version at the cap is one
command, written in chunks; `MetadataCommand::RemoveDefaults { tenant }`, kind 19, and `MetadataGroup::remove_defaults`;
`MetadataReads::session_principal` and `MetadataReads::sessions`, from every command appended.

Edits outside the two crates, each forced by the work:

- `ultragraphdb_common_role`: `Answer::Close(ProtocolError)`, the one way a session refuses by a frame-level rule and closes the
  connection, which the role table's `CommandNotServed` is (§2.5 step 3, §2.6); `serve.rs` answers it with
  `refuse_connection`. `ultragraphdb_error_protocol` moved from its dev- to its normal dependencies.
- `ultragraphdb_common_storage`: its two matches over `Answer` (`partition_core_frames.rs`) and three in its tests take the new
  variant; a partition never answers it.
- `ultragraphdb_protocol_transport`: `Endpoint::Frontend`, the address the system group listens at in this stage, serves the seven
  commands; the endpoint tests expect them (`test_each_role_endpoint_serves_its_row_of_the_table`,
  `test_the_system_groups_commands_are_served_at_its_role_address_alone`, which replaces the test that they are served nowhere).
- `ultragraphdb_common_frontend` and `ultragraphdb_tests`: the scenario "A catalog version past its cap is refused at its
  Commit" takes the specification's new text (16,777,217 bytes, `detail` 16,777,216) and builds a version past the new cap over
  19 Writes; the old text asserted the cap D13 replaces. The frontend's client path is unchanged.
- `ultragraphdb_cluster_metadata`'s feature: the superseded scenario "A catalog version at its cap travels in one Catalog frame"
  and its two steps are retired; its replacements are bound in `common_system`.
- `Cargo.lock`: `hegeltest` in `common_system`'s dependencies; no version changed.

### Phase 1 and 2: the surface and the failing run

The surface compiled with bare `unimplemented!()` at every new decision (the checks, Ended, a Session, the four reads, the seven
handlers, the role table, the codec, apply and Debug of the new command, `remove_defaults`, the two reads accessors, `Answer::Close`
in `serve.rs`); `bazel build //...` passed. The suite written against it, observed failing:

| Target | Phase 2 |
|---|---|
| `cluster_metadata:ultragraphdb_cluster_metadata_tests` | 200 tests, 185 passed, 15 failed: 10 at the unimplemented bodies, 5 at the assertions of the new cap and kind text |
| `cluster_metadata:bdd` | 28 scenarios, 56 steps passed (the retired scenario gone) |
| `common_system:ultragraphdb_common_system_tests` | 99 tests: 36 passed, 61 failed, 2 still running at the 300 s timeout |
| `common_system:bdd` | first with the moved features alone: 8 scenarios, 5 passed, 3 failed (the owner check reads the stubbed `MetadataReads`); with the new features, stopped by the 90 s timeout |
| `common_system:property` | 11 tests: 7 failed, 4 still running at the 90 s timeout |

The hangs are tests whose role connections wait on a session that panicked at a stub; the failing run is the unimplemented panic or
an assertion of a value the stubbed code had not yet changed.

### Phase 3: the audit of the inputs

The suite's inputs were read against the corner cases for coincidence: principals of distinct identities (`alice`, `bob`); sessions
numbered apart from principals; `transaction_ms` 50 ms, so twice it, 100,000 µs, differs from an epoch's 1 to 5 ms and from the
clock's step; a closed connection's close taken by the group before the clock moves (each test round-trips an ask after
`disconnect`); the boundaries at one tick before, at and one tick past each bound; `sessions_max` drawn from 1 to 4. No case was
found where two quantities coincide; three test setups were corrected in phase 4 (below), none weakened.

### Phase 4: the implementation and the suite at its end

Three test setups were wrong and were corrected, no assertion changed: `recovered` creates the tenants a start names, as
`start_system` does; a close is taken by the group before the clock moves (a round trip after `disconnect`); a group whose
accepting task holds a handle is stopped by dropping that task first. One BDD step raced a close still due from its ticking task
and now finds the close that announced the version.

| Target | Before (`2ab8acfb`) | After |
|---|---|---|
| `common_system:ultragraphdb_common_system_tests` | 41 passed | 99 passed |
| `common_system:bdd` | 8 scenarios, 16 steps | 53 scenarios, 106 steps passed |
| `common_system:property` | none | 11 passed (two Hegel state machines and nine properties) |
| `cluster_metadata:ultragraphdb_cluster_metadata_tests` | 195 passed | 200 passed |
| `cluster_metadata:bdd` | 29 scenarios, 58 steps | 28 scenarios, 56 steps (one retired) |
| `protocol_transport` targets | pass | pass |
| `common_role` targets | pass | pass |

The three `common_system` targets passed three uncached runs each in a row.

### Scenarios bound, and the halves of groups 4 and 8

`features/system-group/system_group.feature` (32), `features/metadata-group/metadata_group.feature` (7),
`features/sequencer/sequencer.feature` (5), `features/schema-changes/restart.feature` (1), each verbatim from the change's spec.
The scenarios of the modified metadata-group and sequencer requirements whose text this change left as it was stay bound at the
state machines in `cluster_metadata` and `cluster_sequencer`. "A system table without its directory stops the start" stays bound
in `common_role`, as group 2 bound it.

What each scenario of a host process binds here, and what lands later:

| Scenario | Bound here | Lands in |
|---|---|---|
| The hosting instance serves the books | the group answers a Placement at the role address it listens at | group 4: which process listens where |
| The frontend holds no book | the books are `common_system`'s dependencies (its manifest, a runfile) | group 5: the frontend binary drops the crate |
| The books recover in the storage process | the group over the simulated storage, killed and recovered | group 4: the storage process |
| The group's logs lie beside the partitions' | `start_system` over `<data>/system` keeps `meta/` and `seq/` there | groups 4 and 7: the container's `/data` |
| A partition's sync holds up no close | closes come while another log's sync is held, and are read again from the last held | group 4: a partition's own log |
| A partition of the hosting process dials the group; of another process | storage connections join, subscribe and take every close | group 4: the partitions dialling |
| The storage binary links no schema compiler | `common_system`'s manifest names no `ultragraphdb_schema` crate | group 4: the storage binary's graph |
| A Bind naming an unknown tenant is refused | Placement's runs list no run for tenant 9 | group 4: the partition's refusal |
| A crash before the close; a check passes after the host restarts; an unknown name keeps one salt | the group killed and recovered over the simulated storage | group 8: the hosting process killed |

### Corner cases C3.1 to C3.74

Tests named by file under `tests/types/` (`sg_` for `system_group/system_group_`, `ss_` for `system_session/system_session_`):

| Cases | Test |
|---|---|
| C3.1, C3.2 | `sg_sessions_tests::test_a_registry_with_room_registers_and_evicts_none`; `ss_frontend_tests::test_a_session_is_answered_with_its_bytes` |
| C3.3 | `test_a_full_registry_evicts_its_oldest_session` |
| C3.4, C3.6 | `test_a_marked_session_is_not_evicted_until_its_mark_ends` |
| C3.5, C3.6 | `test_a_registry_full_of_marked_sessions_refuses_a_session` |
| C3.7, C3.8 | `test_a_closed_connections_marks_hold_the_registry_for_twice_the_limit` |
| C3.9 | `test_a_registry_of_one_session` |
| C3.10, C3.11 | `test_a_held_identity_is_refused_whoever_names_it` |
| C3.12 | `test_an_unknown_principal_registers_nothing` |
| C3.13 | `test_a_sessions_refusals_follow_their_order` |
| C3.14 | `test_an_evicted_identity_registers_again` |
| C3.15 to C3.20 | `test_a_checks_outcome_follows_its_order` |
| C3.21, C3.22 | `test_a_marked_session_is_busy_on_every_connection` |
| C3.23, C3.24 | `test_a_frame_of_checks_is_answered_whole_and_in_order`; over the wire `ss_frontend_tests::test_a_check_is_answered_whole_over_the_frontends_connection` (a frame of one and of three; a frame of as many as `max_frame` holds is not tested here, its bound being the wire's) |
| C3.25 | `ss_frontend_tests::test_a_check_is_answered_whole_over_the_frontends_connection` |
| C3.26, C3.27, C3.29 to C3.31 | `test_an_ended_ends_its_own_connections_marks` |
| C3.28 | `test_an_orphaned_mark_is_ended_from_another_connection` |
| C3.29, C3.32 | `ss_frontend_tests::test_an_ended_is_answered_by_nothing` |
| C3.33 to C3.35, C3.38 | `test_a_closed_connections_mark_ends_at_twice_the_limit`; over the wire `ss_session_tests::test_a_closed_connections_marks_hold_for_twice_the_limit` |
| C3.36, C3.37 | `test_two_closed_connections_marks_end_each_at_its_time` |
| C3.39 | `test_a_full_registry_evicts_a_closed_connections_session_at_twice_the_limit` |
| C3.40 to C3.42, C3.45 to C3.47, C3.49 | `test_the_quiet_time_answers_no_resend` |
| C3.43 to C3.47 | `test_the_quiet_time_evicts_no_session` |
| C3.48 | `test_a_restart_keeps_the_sessions_and_loses_the_marks` |
| C3.50 | `sg_syncing_tests::test_a_check_is_answered_while_a_close_syncs_and_a_session_after` |
| C3.51 to C3.53 | `sg_syncing_tests::test_a_check_reads_what_the_registry_holds_from_every_command_appended` |
| C3.54 | `sg_syncing_tests::test_what_arrives_while_a_sync_is_held_is_answered_after_it_in_order` |
| C3.55 | `sg_syncing_tests::test_a_registration_lost_with_its_sync_was_never_answered` |
| C3.56 to C3.58 | `sg_catalog_tests::test_a_declare_builds_on_the_current_version_alone` |
| C3.59 | `sg_catalog_tests::test_a_version_built_on_a_pending_one_takes_the_same_close` |
| C3.60 | the catalog state machine (law L3.19) |
| C3.61 | `sg_reads_tests::test_a_catalog_request_at_an_epoch_never_answers_a_version_being_checked`; the scenario "A check outlives the connection that asked for it" |
| C3.62, C3.63 | `sg_catalog_tests::test_a_declare_whose_asker_left_changes_nothing` |
| C3.64 | `cluster_metadata`: `test_a_version_past_its_bytes_limit_is_refused`, `test_a_version_at_its_cap_is_held_across_recovery`, the codec's and the snapshot's length refusals; the scenarios of the cap and of a version past one frame; `ss_session_tests::test_a_catalog_version_past_one_frame_travels_in_several` |
| C3.65, C3.66 | `sg_catalog_tests::test_a_drop_that_changes_nothing_builds_no_version` |
| C3.67 | `sg_catalog_tests::test_a_drop_of_everything_removes_the_default_entries` |
| C3.68 | `sg_catalog_tests::test_a_silent_partition_withdraws_a_tightening_at_the_wait`; both passed: the catalog state machine and the scenario "A check passes after the host restarts" |
| C3.69 | `sg_catalog_tests::test_a_declare_of_an_unknown_tenant_is_refused`; `sg_reads_tests::test_defaults_of_an_unknown_tenant_is_refused` |
| C3.70 | `sg_reads_tests::test_defaults_lists_the_recorded_predicates_in_order`; `ss_frontend_tests::test_defaults_continue_across_frames` |
| C3.71, C3.72 | `sg_reads_tests::test_a_catalog_request_at_an_epoch_answers_what_a_client_reads_there` |
| C3.73, C3.74 | `ss_session_tests::test_a_close_request_is_taken_by_its_senders_role`; the sequencer scenarios of CloseRequest |

Every case has a test; C3.24's frame of as many checks as `max_frame` holds is covered by the wire's own bound alone.

### Laws L3.1 to L3.35

The property tests are Hegel state machines (`#[hegel::state_machine]`, `hegel::stateful::machine`, hegeltest 0.52.6 offers the
stateful API) where the law is one, and drawn properties otherwise, all under `tests/property/`, target `property`.

| Laws | Test |
|---|---|
| L3.1 to L3.14 | `system_group_sessions_property_tests::test_the_registry_agrees_with_its_model`: a state machine of Session, Check, Ended, a connection's open and close, time passing (spans at and around `transaction_ms` and twice it) and a restart, against a model written from §11.5 and §13.23 to §13.25, `sessions_max` drawn from 1 to 4; every answer compared, and after every step the registry's holdings and `sessions_max` |
| L3.15, L3.16 | `test_a_check_reads_every_command_appended_while_a_sync_is_held`, drawn `sessions_max` (the machine has no Sync command; this property and C3.50 to C3.55 hold the two laws) |
| L3.17 to L3.21, L3.23 to L3.26, L3.28 | `system_group_catalog_property_tests::test_the_catalog_agrees_with_its_model`: a state machine of Declares of both kinds (bases current, one below, one above, 0; tightening, loosening, both, neither), default entries, closes, reports passed and refused, restarts, Catalog requests at a drawn epoch and askers that leave, against a model written from §11.13, §13.7 and §13.27 |
| L3.22, L3.27 | the same machine for a refusing and a passing report and a restart with a version being checked; the wait of `catalog_check_ms` and a silent partition by C3.68's unit test, which the machine leaves out (its clock moves an epoch a step) |
| L3.29 | `test_a_version_past_the_cap_is_refused` (a drawn 1 to 64 bytes past the cap) and `system_session_session_property_tests::test_a_version_reads_back_whole_at_any_max_frame` (a drawn size to 1 MiB at a drawn `max_frame`); the cap itself by the unit and BDD tests |
| L3.30 | `system_group_reads_property_tests::test_defaults_answer_the_records_in_their_texts_order` (0 to 64 predicates of at most 4 KiB fit one frame at the floor; across frames by C3.70's unit test) |
| L3.31 | `test_a_challenge_answers_one_salt_a_name` |
| L3.32 | `test_a_verify_holds_exactly_for_the_right_proof` |
| L3.33 | `system_session_session_property_tests::test_no_reply_holds_a_key` |
| L3.34 | `test_a_subscription_at_the_latest_begins_there` |
| L3.35 | `test_the_endpoint_table_by_role`, over the 13 commands the table names that a role client can build; `UnknownCommand` for a number §3 lacks is the transport's refusal, which its own tests hold |

Every law has a test.

### Phase 5: fuzzing

A `SystemSession` decodes nothing of its own: every frame it reads is read by a view of `ultragraphdb_protocol_wire`, whose fuzz
targets `session`, `check`, `ended`, `declare` and `defaults` (group 1) and the existing ones cover the decoders, with their seeds
in the wire's `testdata/fuzz_seeds/`, the convention's place. No seed was added under `common_system`, which has no fuzz target.
`cluster_metadata`'s `command_decode` decodes the new kind 19; its seeds are the encodings of the kinds 1 to 13 its tests compare
them against, as the kinds 14 to 18 have none, so none was added. The runs of `scripts/fuzz.sh` belong to the stage's final
verification.

### Phase 5: mutation testing, closed early

The user stopped the runs on 2026-11-16 once the group's suite was complete, and closed the group without further runs. What
`scripts/mutants.sh --diff dbfd958e` left (`target/mutants-ledger/`, `mutants.out/`):

| Crate | Runs counted | Outcome kept |
|---|---|---|
| `ultragraphdb_common_system` | 2 | the last run: 30 mutants, 28 caught, 2 unviable, 0 missed |
| `ultragraphdb_cluster_metadata` | 1 | not kept: the run's output was overwritten by the next crate's |
| `ultragraphdb_core_catalog` | 1 | not kept: stopped in its baseline build |
| `ultragraphdb_common_role` | 1 | not kept |
| `ultragraphdb_protocol_transport` | 1 | not kept |
| `ultragraphdb_common_storage` | 1 | not kept: stopped in its baseline build |

No survivor was recorded and `.cargo/mutants.toml` gained no entry. The runs this group leaves undone, each crate's run over
the diff from `dbfd958e` with its outcome kept, belong to the stage's final verification.

### Found wanting

1. **The role table's refusal is a connection error.** §11.12: the system group "refuses, as `CommandNotServed` naming the
   command, a command the table does not have the connection's role send"; §2.5 step 3 names `CommandNotServed` a frame-level
   rule, and §2.6 closes the connection for one. Implemented: Error 1 on stream 0 and the close, through the new
   `Answer::Close`. A CloseRequest refused so is dropped unanswered as every CloseRequest refusal is (§13.16), and an Ended
   from another role is refused as `CommandNotServed` like any command, while an Ended's body refusal is dropped (§13.25).
2. **"The instant the group begins to listen."** §11.5 counts the quiet time from it; in this stage the group listens through the
   frontend's role listener. Implemented: the instant `SystemGroup::run` begins; group 4 starts the listener after recovery, so
   the two coincide there.
3. **The iteration count of an unknown name.** §13.8: "the iteration count the system group derives a principal's verifier
   with". The group has no such setting: the bootstrap principal is derived with `DEFAULT_ITERATIONS`, 600,000, and a principal
   created otherwise may hold another count. Implemented: `DEFAULT_ITERATIONS`.
4. **A Drop of everything's removal "in the same command."** system-group, "The system group builds the catalog versions it is
   asked for", and §13.27 ask that a Declare of `drop` 4 remove the tenant's default-entry records "in the same command", and no
   command of the metadata state machine removed one (design, *Non-Goals*: the state machines keep their commands). Implemented:
   `MetadataCommand::RemoveDefaults`, kind 19, appended in the same turn as the version's offer and durable with it before the
   reply; read as "the same Declare". A crash between the two rounds leaves the version without the removal, which the Drop's
   resend completes (a Declare of kind 2 may be sent again, §11.13). Whether the two belong in one log command is the user's.
5. **A reply's body sent as a request.** §13.27, §13.28, §13.8 and §13.9 say a body with its reply fields 0 is a request and do not
   say what the system group does with a reply-shaped body it receives. Implemented: taken as a request, its reply fields
   overwritten in the answer; nothing is refused.
6. **A declared version's durability.** §11.13 has a version "committed as pending" and assigned at the next close; the owner made
   an offer durable only at the next sync of anything, so a version waited one close more. Implemented: the offer is synced in its
   own turn (`Durable::Offered`), and the next close assigns it. A behaviour change on the frontend's path too: a declaring
   Commit can be answered one epoch earlier.
7. Open, unchanged: **the principal of a registration appended and not yet durable** (C3.52). Implemented: the principal the
   appended registration names, read from every command appended (`MetadataReads::session_principal`); the old owner check read
   it from what is durable, which no in-process path could observe since the frontend checks only after the Register answered.
8. Open, unchanged: **a Defaults answer past `catalog_bytes_max`**. The group answers every record it holds, across frames.

### Notes for the commit

The working tree's index holds staged entries no step of this run made (`git add` was not run here); the files listed above are the
change.
