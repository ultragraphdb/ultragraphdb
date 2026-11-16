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

### Step (c): not made, the new edge closes a cycle under Bazel

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
The move therefore waits for step (c), and this run stopped here.

## The gate over tasks 3.1, 3.2 (a) and 3.2 (b)

Run over the working tree holding the three steps, on HEAD `dbfd958e` (the other agent's commit, which touched none of
these files):

| Gate | Result |
|---|---|
| `make build` | passes |
| `bazel test //... --test_tag_filters=-integration_test` | 148 targets, all pass |
| `make test` | build, doc (20), unit (79), property (5), Loom (5) and BDD (39) targets pass; integration: 6 of 7 targets pass, `//ultragraphdb/ultragraphdb_tests:bdd` fails one scenario of 202 |

Integration counts: `compute_tests:bdd` 88 scenarios, `config_tests:bdd` 44, `frontend_tests:bdd` 20,
`storage_tests:bdd` 98, `supervisor_tests:bdd` 28, `supervisor_tests:image_bdd` 7, all passed; `ultragraphdb_tests:bdd`
202 scenarios (201 passed, 1 failed), then 10, 36, 38 and 36, all passed.

The failure, reproduced on a second run: "The frontend serves clients on every core"
(`ultragraphdb/ultragraphdb_tests/features/client-endpoint/processes.feature:13`), step
`ultragraphdb/ultragraphdb_tests/tests/own_steps.rs:260`, `assertion left == right failed: a runtime per CPU: [1, 2, …,
15]`, 15 against 16. It does not come of this run's change; it stands at `e7545937`. The step counts the distinct `core=`
values of the frontend's "a runtime on the … driver" lines and expects one per CPU. Since `fe27ebff` (an ancestor of
`e7545937`), the role's first runtime is built by `role_runtime` and logs "the role's first runtime on the … driver"
with no `core` field, where it was `core_runtime(config, 0)` logging `core=0`
(`ultragraphdb_common/ultragraphdb_common_service/src/utils/run_service.rs:316-332`). What is left logs core 1 (the
owner, `utils/owner_thread.rs`, `OWNER_CORE`) and cores 1 to 15 (the endpoint cores, `ultragraphdb_frontend`'s
`start_cores`): 15 values on this 16-CPU machine. This run changed none of those lines. That it fails at `e7545937` was
established from the code and its history, not by a run at that commit, since a run there needs a second checkout and a
full build.
