# Stage 5.9, group 5, task 5.1: the Ultra Style lift of `ultragraphdb_common_frontend`

Base commit `85edbeb3`. This file records the second half of task 5.1: the lift of
`ultragraphdb_common/ultragraphdb_common_frontend` into `openspec/protocols/ULTRA_STYLE.md` before task 5.2 merges the crate
into `ultragraphdb_common_compute`. The lift changes no behaviour and no public signature, so `ultragraphdb_frontend` and
`ultragraphdb_frontend_tests` are untouched. Counts are those Bazel's test logs report: a unit or property target's
`test result`, a BDD target's scenarios and steps.

Stage 5.8's task 3.1 lifted this crate on 2026-11-16 at `e7545937` (the archived change's `notes/group-3.md`). Since that
base the crate lost 19 files and gained 16, 3,819 lines in and 3,980 out, most of it by groups 5 and 8: `system_link/`, `system_turns.rs`,
`endpoint_view.rs`, `check_round_trips.rs`, `relay_authenticator.rs`, `throttle_handle.rs`, `recall_order.rs`,
`declare_bound.rs`, `utils_test/system_play.rs`, and the rewritten `client_session/` handlers, `role_link/` and
`role_links.rs`. This lift reads all of `src/` again and changes what those groups wrote or what task 3.1 left.

## The audit against `openspec/protocols/ULTRA_STYLE.md`

Scope: all of `src/` at `85edbeb3`, test utilities included: 62 files, 9,596 lines, 459 functions. After the lift: 66 files,
9,743 lines, 461 functions.

How it was measured, as stage 5.8's groups measured: every file written or rewritten since task 3.1 read in full against
the rules, the rest by the search patterns of its group 4's checklist (`dyn`, `fn(`, recursion, `loop`/`while`, unbounded
collections and channels, `assert!`, `let _`, `.ok()`, `drop(`, casts, cells and atomics, `cfg`, single-letter names,
arithmetic); rule 4 by a script counting each function's lines, comments and blank lines left out; Clippy under Bazel
through the aspect, once with the workspace table alone and once with the five lints Ultra Style states beyond it
(`-Dclippy::indexing_slicing`, `cast_possible_truncation`, `must_use_candidate`, `missing_errors_doc`,
`missing_panics_doc`). Both runs were clean over the library, the unit, the property and the eleven BDD targets before the
lift and are clean after it.

| Rule | Result | Evidence |
|---|---|---|
| 1, acyclic control flow | conforms | no recursion and no stored callback; the one `&dyn Display` closure carries its reason (`utils_test/role_double.rs`); `poll_fn` closures are called in place |
| 2, loops | **gap, closed** | three loops did not say what ends them: `ClientSession::pump` (`client_session_bind.rs`: each step that does not return takes the queue's front), `describe_credited` (`client_session_requests.rs`: one credit and one frame per pass) and `EndpointView::in_effect` (the versions held). Each says so now. The event loops of `system_link/` and `role_link/`, `answering`, `token`, `until_ready` and `per_frame` said so already |
| 2, queues | **gap, closed in part** | `LinkState`'s `waiting`, `checks_out`, `asks`, `outgoing`, `watchers` and `decided`, `EndpointView`'s `holds` and `Routes`' `open` did not say what bounds them; each says so now. Left: none has a capacity fixed at startup that answers an operating error when full, nor have `SystemTurns`' queues, `ClientSession`'s tables and event channel or `ThrottleHandle`'s channel, each bounded through its connections' bounds alone. A capacity is a new refusal, a behaviour change. See *Found*, 1 |
| 3, planes | **gap, closed** | `RoleLink` (data plane) and `RelayAuthenticator` (control plane) state theirs; `lib.rs`, `SystemLink`, `SystemTurns` and `EndpointView` did |
| 4, 70 lines | conforms | `clippy::too_many_lines` clean; the longest bodies, signature and braces counted, are 71 lines (`check_change`, `ClientSession::request`, `drop_request`) |
| 5, invariants | **gap, closed in part** | `src/` held no check at all. Seven `debug_assert!` now state what a function relies on and the types do not hold: a stream is never 0 (`LinkState::stream`, `RoleLink::open`), a turn runs once nothing waits to leave (`system_link_tasks::turn`), a frame takes between one entry and what its bytes hold (`per_frame`, two), one count is held for each open request (`ClientSession::recount`), each partition of a retried Commit is asked or answered (`retried_commit`). All hold over the whole suite and the binary's battery. Left: no `invariant(...)?`; the crate has no error of its own to carry an `InvariantError` (its refusals are the wire's `ErrorReply`) and no dependency on `ultragraphdb_error_invariant`, and a returned invariant error is a new way for a session to end |
| 6, smallest scope | conforms | `clippy::shadow_unrelated` clean |
| 7, results | **gap, closed in part** | 19 dropped results did not say why: a reply's send to a waiter gone (`system_link_tasks.rs`, five), `Header::stamp_stream`'s result on a frame that holds its header (`client_session_bind.rs`, `client_session_one_frame.rs` three, `client_session_reads.rs`), the frames `close_request`, `subscribe` and `RoleLink::cancel` could not build, `changed_or`'s timeout, `run`'s `select` and its sending task's join, `delivered_close`'s `close`, `closed`'s `withdraw`, `spawn_members`' `until_ready`. Each says why now. `Turn::ended_frame` and `check_frame` say when a frame that cannot be built is `None`. `empty_batch` built a `TermTable` it dropped; removed. Left: a frame that cannot be built is dropped without an answer in `send_bind` (its transaction then waits out its limit), `close_request`, `subscribe`, `RoleLink::cancel` and `credit`; answering it is a behaviour change. Left: `deliver` of both links and `RoleDouble::join` return their refusal as a `String`, which the caller logs; the crate has no error type |
| 7, panics | conforms | no `unwrap`, `expect`, `panic!` or indexing; every `as` says why it is lossless; unchecked `+`, `-` and `<<` are over counts the code bounds itself (attempts, frames, entries of `per_frame`, a played group's versions), none over a value from a peer. `Instant + Duration` takes a client's `deadline_ms`, a `u32` of milliseconds, 49.7 days at most |
| 8, macros, `cfg` | conforms | no `macro_rules!`; the only `cfg` is `feature = "test-utils"` (`lib.rs`) |
| 9, ownership | **gap, closed** | `RoleDouble` shared three `Rc`s of `RefCell`s without saying why; it says so now, as the split-out `RelayMechanism` does. Every other `Rc` and cell stated its reason. No `unsafe`, `Arc`, lock or atomic |
| 10, zero warnings | conforms | both Clippy runs clean; the one `#[expect(clippy::disallowed_methods)]` (`client_session::now`) carries its reason; no `#[allow]` |
| Types: `#[must_use]` | **gap, closed** | `SystemTurns::queue_check` and `take`, `EndpointView::hold` and `released` return what their callers must read and take `&mut self`, which `must_use_candidate` does not look at |
| Types: mixed-up arguments | left open | functions taking several `u64` of different meaning (`SystemLink::declare(tenant, built_on, principal, ..)`, `EndpointView::catalog(tenant, version, epoch, ..)`, `close(epoch, placement, ..)`, the handlers' `(stream, transaction)`) and returning tuples of them (`declare`, `catalog_at`, `challenge`): public signatures the binary and its battery follow, and the merge of task 5.4 rewrites the handlers |
| Types: exhaustive matches, casts, widths | conforms | `wildcard_enum_match_arm` and the cast lints clean; the wire's `Command` is compared, never matched with `_` |
| Concurrency: cancellation | **gap, closed** | `SystemLink`'s async methods did not say what a dropped future leaves; its documentation now says so for a check, an ask, `token` and `answering`, and `run`, `read`, `next_frame`, `until_ready` and `EndpointCore::admits` say theirs. See *Found*, 2 |
| Concurrency: clocks, awaits | conforms | the one clock is `client_session::now`, in the shell; no borrow of a `RefCell` crosses an `.await` (`await_holding_refcell_ref` clean) |
| Naming | **gap, closed in part** | closures took `f`, `c`, `r` and `p`; they take `frame`, `commit`, `recalling` and `owner`. `poll_fn`'s `cx` is `context` (12 uses). Paths written out in signatures are imported (`HoldOutcome`, `SessionCheck`, `Instant`, `Waker`). Left: `txn`, 211 uses in seven files of `client_session/`, where `transaction` already names the transaction's stream in the same functions; a rename renames both |
| Comments | conforms | every public item documented with `# Errors`; the added comments wrap at 120 columns |
| Off-by-one, division | conforms | `per_frame`'s `most / 2` now says it rounds down; every `%` is over a count checked non-zero |
| Layout (`AGENTS.md`) | **gap, closed in part** | three files held two public types. `Turn` has `types/turn.rs`, `HoldOutcome` `types/hold_outcome.rs`, `RelayMechanism` `types/relay_mechanism.rs` and the state it shares with `RelayAuthenticator` `types/relay_state.rs`; `lib.rs` exports each from its module, the crate's public names unchanged. Left: see *Left*, 1 to 3 |

## Left

1. **Folder modules, each a file move.** A type past 25 lines belongs in `types/<type>/mod.rs` with each trait's
   implementation in a file of its own: `endpoint_view.rs`, `system_turns.rs`, `role_links.rs`, `relay_authenticator.rs`
   (with its `Authenticator` and `Debug`), `check_round_trips.rs`, `endpoint_core.rs`, `endpoint_limits.rs`,
   `login_limits.rs`, `route_sink.rs`, `throttle_handle.rs`, `client_transaction.rs`, `counted_frame.rs` (with its `Drop`),
   `commit_hold.rs` (with its `Drop`), and `ClientSession`'s `Drop`, which sits in `client_session_session.rs`. No file was
   moved in this lift: a move touches the index, and task 5.2 moves every file of the crate by `git mv`.
2. **Private types beside the type they serve:** `LinkState` and `Ask` (`system_link/mod.rs`), `Outgoing`, `Route` and
   `Routes` (`role_link/mod.rs`), `OneFrame`, `DropRelay` and `Recalling` (`client_session/mod.rs`), `Held`
   (`role_links.rs`), `Failures` (`auth_throttle/mod.rs`), `Step` (`client_session_bind.rs`), `OneFrameFacts`
   (`client_session_one_frame.rs`), `Slots` (`utils/term_rules.rs`), `Played` (`utils_test/system_play.rs`). One module each
   makes every field `pub(super)` or wider; task 5.4 rewrites the session's three and group 4 the link's.
3. **Test files that mirror no source file.** `turn.rs` and `relay_mechanism.rs` are tested through their owners in
   `system_turns_tests.rs` and `relay_authenticator_tests.rs`; `utils_test/system_play.rs`, `double_frame.rs` and
   `double_session.rs` have no test file, where `AGENTS.md` asks for test utilities to be tested. A test added or moved
   changes a target's count, which this lift holds equal.
4. The rows above marked left: capacities (rule 2), `invariant()` (rule 5), frames that cannot be built and refusals as
   `String` (rule 7), mixed-up arguments (*Types*), `txn` (*Naming*).

### Skipped for task 5.7

Task 5.7 removes the relay of reads, so these were read and not lifted: `RoleLinks`' spread over compute processes
(`computes`, `compute_turn`, `next_compute`, `compute_count`, the compute arms of `joined` and `learned`); the session's
relayed read (`compute_read`, `dispatch_read`, `members_again`, `members_read`, `read_closed`, `relay_result`,
`read_refused`, `read_credited`, `read_cancelled_answering`), its hung-compute timer (`hung_at`, the hung part of
`timers` and `next_deadline`) and `ClientRead`'s `tried`, `attempt`, `members_read` and `last_frame`; `RoleLink::credit`
and `RouteSink`'s credit; `Execute`'s ComputeRead (`client_session_prepared.rs`); `RoleDouble`'s `batch_frame`, and
`SystemPlay::compute`. What they break and nothing here closes: `tried` grows by one address per attempt with no stated
bound; `state.attempt + 1`, `state.frames += 1` and `deadline.max(last_frame) + scan_stall` are unchecked; `SessionEvent`'s
`MembersRead` and `Queued::Read` follow them. Closed there because it cost one line: `empty_batch`'s dropped `TermTable`.

## Found

Each is a behaviour of the code as stage 5.8 left it. None is changed here; each is group 5's or group 4's to take with the
code it rewrites.

1. **`LinkState::watchers` grows with frames read, not with waiters.** `system_link_tasks::run` waits on
   `select(reading, stopping)`, and `spawn_members` on `select(found, withdrawn)`: each wake of the first future polls the
   second, which pushes another clone of its task's waker. Between two changes of the view, `watchers` so holds one waker
   for each frame the link read, where one for each waiter would do. A close empties it, so its length is bounded by the
   frames read within `epoch_ms_max`. Keeping a waiter's waker once (`Waker::will_wake`) bounds it by the waiters.
2. **A dropped `SystemLink::token` leaves its outcome in `decided` for the link's lifetime.** Another waiter's pass moves
   every released hold into `decided`, and only the hold's own waiter takes it out. Today only the runtime's end drops a
   request's future (`ClientSession`'s documentation), so nothing accumulates; a request cancelled mid-wait would.
3. **The property `test_a_begin_asks_no_other_process` raced the link's own Members reads.** See the next section.

## The property's race, fixed apart from the lift

`tests/property/types/system_link_property_tests.rs`, law L5.3: a Begin naming no token sends no frame to the system
group. The property counted every command the played group took but EpochClose, before and after each Begin. The link's
own task reads Members every 100 ms (`MEMBERS_RETRY`) until the group lists every partition of the placement, which this
property's group never does, for the cluster's deadline of two seconds. A Members landing between the two counts failed
the law for a frame no Begin sent.

It failed once in the lift's verification, on sources whose logic the lift had not changed, with the machine also running
the parallel lift of `ultragraphdb_common_compute`: "no frame to the group, left: 7, right: 6", and Hegel's "failed 0 of 10
replays". With the assertion made to name the commands seen, 48 runs in parallel (`--runs_per_test=48
--local_test_jobs=24`) failed once: `[Subscribe, Members, Placement, Catalog, Challenge, Verify, Members, Members]`, the
last a Members.

The fix leaves Members out of both counts, as EpochClose is, and keeps the commands in the assertion's message. 144 runs
in three such invocations then passed. The law still fails for every frame a Begin could send: Catalog, Placement, Check,
Session, CloseRequest. It is a test's change alone, with a commit message of its own.

## The lift and its evidence

No logic changed: comments and documentation, seven `debug_assert!`, four `#[must_use]`, four closure bindings and
`cx` renamed, four imports for written-out paths, one unused value removed, and three types with the state one of them
shares moved into four new files. 147 lines in and 136 out over 20 files of `src/`, and 136 lines in the four new files.

Files, all under `ultragraphdb_common/ultragraphdb_common_frontend/`:

- changed: `src/lib.rs`, `src/types/mod.rs`, `src/types/client_session/mod.rs`, `client_session_bind.rs`,
  `client_session_commit.rs`, `client_session_one_frame.rs`, `client_session_reads.rs`, `client_session_requests.rs`,
  `src/types/endpoint_core.rs`, `src/types/endpoint_view.rs`, `src/types/relay_authenticator.rs`,
  `src/types/role_link/mod.rs`, `src/types/role_link/role_link_tasks.rs`, `src/types/role_links.rs`,
  `src/types/system_link/mod.rs`, `system_link_reads.rs`, `system_link_tasks.rs`, `src/types/system_turns.rs`,
  `src/utils_test/role_double.rs`, `src/utils_test/system_play.rs`;
- new: `src/types/turn.rs`, `src/types/hold_outcome.rs`, `src/types/relay_mechanism.rs`, `src/types/relay_state.rs`;
- the property's fix, its own commit: `tests/property/types/system_link_property_tests.rs`.

| Target | Before, at `85edbeb3` | After the lift |
|---|---|---|
| `common_frontend:ultragraphdb_common_frontend_tests` | 114 passed | 114 passed |
| `common_frontend:property` | 12 passed | 12 passed |
| `common_frontend:bdd` | 38 scenarios, 76 steps | 38 scenarios, 76 steps |
| `common_frontend:bdd_catalog` | 11 scenarios, 22 steps | 11 scenarios, 22 steps |
| `common_frontend:bdd_catalog_cap` | 1 scenario, 2 steps | 1 scenario, 2 steps |
| `common_frontend:bdd_compute` | 9 scenarios, 18 steps | 9 scenarios, 18 steps |
| `common_frontend:bdd_endpoint` | 22 scenarios, 44 steps | 22 scenarios, 44 steps |
| `common_frontend:bdd_held` | 7 scenarios, 14 steps | 7 scenarios, 14 steps |
| `common_frontend:bdd_quad_reads` | 14 scenarios, 28 steps | 14 scenarios, 28 steps |
| `common_frontend:bdd_routing` | 17 scenarios, 34 steps | 17 scenarios, 34 steps |
| `common_frontend:bdd_system_link` | 16 scenarios, 32 steps | 16 scenarios, 32 steps |
| `common_frontend:bdd_tightening` | 7 scenarios, 14 steps | 7 scenarios, 14 steps |
| `common_frontend:bdd_timer` | 4 scenarios, 8 steps | 4 scenarios, 8 steps |
| The eleven BDD targets together | 146 scenarios, 292 steps | 146 scenarios, 292 steps |
| Clippy aspect, the workspace table | clean | clean |
| Clippy aspect, the five lints added | clean | clean |
| `rustfmt --check` over `src/` and the property's file | | clean |
| `common_frontend:doc` | | builds |
| `make build` | | passes |
| `ultragraphdb_frontend_tests:bdd`, one invocation | | 30 scenarios, 60 steps passed |

Three runs of the thirteen targets, in order: on the lifted sources all thirteen passed; after the added comments were
wrapped again, twelve passed and `property` failed as the section above says; after the property's fix `property` ran and
passed, and the other twelve, whose inputs the fix does not reach, came from Bazel's cache of the second run. The counts
above are the third run's.
