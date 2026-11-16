# Stage 5.8, group 5: the frontend without books, `ultragraphdb_common_frontend`

Base commit `443a7f54`. Tasks 5.1 to 5.4 and 5.6, and task 5.5 as a step of its own after them (approved by the user on
2026-11-16). Counts are those Bazel's test logs report: a unit target's `test result`, a BDD target's scenarios and steps.
Times are a target alone, uncached (`--nocache_test_results`), on this machine with the other groups' agents running.

## What was built

`ultragraphdb_common_frontend`:

- `SystemLink`: one endpoint core's role connection to the system group, dialled as `frontend`, dialled again every 100 ms
  while closed, subscribed with `latest` on each connection (Subscribe first on the connection), its closes applied to the
  core's `EndpointView`. Asks on a stream of their own: `session`, `challenge`, `verify`, `declare`, `drop_catalog`,
  `defaults` and `catalog_at` (each read across frames, a Credit of one frame granted per Catalog and Defaults frame read),
  `schema_id` and `known` (a tenant's current version read once at its first use, code 7 `detail` 0 taken as none),
  `members`, `read_placement`, `close_request` (`partition` 0, nothing awaited). `check` queues a check for the next turn and
  awaits its outcome; `ended` queues an Ended, kept across a closed connection and sent on the next. A turn is one run of
  the sending task: when nothing else waits it takes `SystemTurns::take` of what was queued and sends the Ended frame, then
  the Check frame (`PROTOCOL.md` §11.5, *Turns*). Each Check frame is timed from its queueing for the wire to its reply into
  `CheckRoundTrips`. A closed connection refuses every check and ask waiting or asked with code 17; `lost_since`, `is_up`,
  `answering` (waits for the view, code 17 past its deadline), `token` (holds a Begin's token, code 24 `detail` 0 at the
  deadline), `max_frame`, `stop`, `with_core`, `round_trips`, `view`.
- `EndpointView`: sans-I/O; the latest close (a close at or below it applies nothing), the placement and its tenants, each
  tenant's highest version learned, the schema id of each version read (`describe_of` over its declarations), the tenants
  to read again after a subscription opened again (`reopened`: nothing is answered until its first close and then Placement
  and each held tenant's current version are read), and the Begins a token holds (`hold`, `released`, `next_deadline`), a
  close taken before a deadline at the same instant. `HoldOutcome`.
- `SystemTurns` and `Turn`: the turn as a sans-I/O queue, the driver the tests order a turn exactly with (`take`), with
  `checks_max` and `ended_max` per `max_frame`, computed by building frames.
- `CheckRoundTrips`: the round trips over a `PhaseWindow`, logged as `check round trips` with `round_trip_p50_us`,
  `round_trip_p99_us` and `round_trip_mean_us`; `EndpointCore::report_phases` reports it beside the one-frame phases.
- `RelayAuthenticator`, `RelayMechanism`: the transport's authenticator of a relayed exchange (design D8): the session reads
  the client's messages with `ScramRelay`, asks Challenge and Verify, and prepares each step's answer; the frontend holds no
  verifier. A Challenge or Verify the group cannot be asked ends the connection with Error 17, counting no failure.
- `RecallAnswer`, `RecallDecision`, `recall_decision`, `second_check_answer`, `check_refusal`: §11.5's order of a Recall's
  answers and the outcomes' codes, pure.
- `declare_fits`: a Write whose declarations pass one Declare frame within the link's `max_frame` is refused with code 6,
  `detail` the `max_frame` (§11.13).
- `SystemPlay` (test utility, `test-utils`): the system group played at a `RoleDouble`'s address (task 5.2).
  `RoleDouble::drop_connections`.
- Changed: `EndpointCore::new` takes the `SystemLink` (`system()` replaces `owner()`); `ClientSession::new` takes the
  `RelayAuthenticator`; `RoleLinks::learned` (Members in place of what was held), `lists`, `deadline`, `closed_found` (a link
  found closed asks Members again); a read finding no compute process reads Members once more before it is refused
  (design D9); a Begin naming a token ahead of the view waits for its close (§4.8), a one-frame write naming one goes to its
  partition; Describe from Catalog at the latest close and Defaults, answered across frames under the client's credit
  (`DescribeFrames` checks `catalog_bytes_max`, code 6); a declaring Commit and a Drop through Declare; the Recall's
  second Check of kind 1 (12, 21, 14; 18 on outcome 1), a Recall ending with no Ended since it marks nothing.

`ultragraphdb_common_role`: `Session::authenticating_step`, the hook before each step of an exchange, whose refusal ends the
connection with it; `serve_connection_with`, a connection authenticated by an `Authenticator` in place of credentials.
`ultragraphdb_protocol_transport`: `ServerConnection::close_connection(&ErrorReply)`, which the hook's refusal uses.

`ultragraphdb_client_core` (by a delegated run): a Describe read across frames through `DescribeFrames`, credit granted as
frames are read; `TestServer::with_describe`.

## The settings until group 7's switch (design D2)

Tasks 5.1 to 5.4 leave the frontend binary on both settings, decided by one fact: whether its settings name
`cluster.system_address`.

- **Named** (`[cluster] system_address` set): the frontend hosts nothing. It reads no `frontend.role_address`, no
  `frontend.data_dir`, no `[metadata]` and no `[sequencer]`, listens at its client and observability addresses alone, and each
  endpoint core's `SystemLink` dials `cluster.system_address` (`common_role::system_address`, the setting group 2 added).
- **Not named** (every profile in the tree today): the frontend hosts the system group as before: `start_system` over
  `frontend.data_dir` with the `[metadata]` and `[sequencer]` tables, and the role listener at `frontend.role_address` serving
  `SystemSession` and taking each Join to `JoinedRoles`. The one difference from before is how the endpoint cores ask the
  group: each core's `SystemLink` dials `frontend.role_address`, the address the group listens at in this process, so the
  client path is the one path a cluster with a storage-hosted group takes, and no `ClientSession` asks a `SystemHandle` in
  process. A group that stops ends the role with its outcome, as the owner's did.

Task 5.5 removed the second branch (below): the frontend requires `cluster.system_address`, refused at its start as a
configuration error when absent, and a profile naming only `cluster.frontend_address` has no host until group 7's switch
rewrites the profiles. That is the design's order (D2, D12): the integration targets first pass at group 7's gate.

## Phase 1 and 2: the surface and the failing run

The new types' surface compiled with bare `unimplemented!()` bodies (`SystemTurns`, `Turn`, `EndpointView`, `CheckRoundTrips`,
`RelayAuthenticator`, `SystemLink`, `recall_decision`, `second_check_answer`, `check_refusal`, `declare_fits`), with the two
additions to `common_role` and `protocol_transport`; `bazel build` passed. The suite written against it, observed failing:

| Target | Phase 2 |
|---|---|
| `common_frontend:ultragraphdb_common_frontend_tests` | 99 tests: 56 passed (the suite at the base), 43 failed, each at an `unimplemented!()` of `check_round_trips` (2), `endpoint_view` (13), `relay_authenticator` (5), `system_link` (14), `system_turns` (9) |
| `common_frontend:property` | 10 tests: 8 failed at an `unimplemented!()` (`endpoint_view` 3, `relay_authenticator` 1, `system_turns` 1, `declare_bound` 1, `recall_order` 2); 2 passed, L5.11 and L5.12, which state laws of `AuthThrottle`, which this group leaves unchanged |
| `client_core:ultragraphdb_client_core_tests` | 213 tests: 12 failed at the `unimplemented!()` of the continued Describe's reader, 201 passed |

Not stubbed first, and judged by the suite already standing: the rewiring of `ClientSession`, `EndpointCore`, `CommitHold` and the
binary onto the link, which the 127 scenarios of `common_frontend:bdd` and the unit tests at the base judge, run against a real
system group served at a role address (`tests/endpoint_stage.rs`). Laws L5.3 and L5.9 were written in phase 4, after the link
existed, and so were never seen failing against a stub; the BDD scenarios of `features/client-endpoint/system_link.feature` were
written in phase 4 too.

## Phase 3: the audit of the inputs

Sessions numbered apart from principals (sessions 1 to 8, principal 41); a frame's count found by bisection over built frames,
apart from the arithmetic of `checks_max`; a token one and two closes ahead; a close one tick before and at the deadline; versions
3 and 4 apart from epochs 7 to 12; a Defaults answer of 2,000 predicates and a Catalog of 700,000 bytes, each past one frame of
the floor; an initial credit of the Describe stream apart from its two frames. No case where two quantities coincide was found.

## Phase 4: the implementation and the suite at its end

Test setups corrected, no assertion of a standing test changed: the BDD stage's cores dial the group at the stage's role
address and wait for their view; the stage's group runs with `transaction_ms` 1, so its quiet time (twice `transaction_ms`,
`PROTOCOL.md` §11.5) has passed before a client's first request, where the owner checked a Recall in process with no quiet time;
`Stage::stop` stops the link, whose connection otherwise holds the group's handle; the client session tests register their
doubles with the group, which Members then lists.

Scenarios retired, superseded by this change's text (as group 3 retired one): "A token above the open epoch is refused"
(`features/transactions/sessions_frontend.feature`; `PROTOCOL.md` §4.8 now holds such a Begin and sends such a write to its
partition; its replacement is "A token ahead of the core's view waits and is not refused") and "A Describe past the client's
frame is refused" (`features/wire-protocol/server_rules.feature`; §8.3's *Continuation*; its replacements are "A Describe
continues across frames" and "A Describe past the catalog's bound is refused"). The scenario "A catalog version past its cap is
refused at its Commit" keeps its assertion and builds its version within the one-frame rule (Found wanting 1).

`ultragraphdb_client_tokio` and `ultragraphdb_client_compio`: their conformance test served the golden `describe_reply`, which
holds flags 0 (Found wanting 4); a client reading a continued Describe waits for a frame flagged `LAST`, so each now serves it
flagged (`last_of`), as `client_core`'s own conformance test does.

| Target | At the base `443a7f54` | After 5.1 to 5.4 | Uncached, alone |
|---|---|---|---|
| `common_frontend:ultragraphdb_common_frontend_tests` | 56 passed | 99 passed | 3.5 to 4.0 s |
| `common_frontend:property` | none | 12 passed | 4.0 to 4.6 s |
| `common_frontend:bdd` | 127 scenarios, 254 steps | 141 scenarios, 282 steps (2 retired, 16 added) | 173 s |
| `client_core:ultragraphdb_client_core_tests` | 206 passed | 213 passed | 0.4 to 0.8 s |
| `client_core:bdd` | 45 scenarios | 47 scenarios, 94 steps | 0.8 s |
| `client_tokio:ultragraphdb_client_tokio_tests` | 56 passed | 56 passed | 7.9 s |
| `client_compio:ultragraphdb_client_compio_tests` | 35 passed | 35 passed | 31.5 s |
| `common_role:ultragraphdb_common_role_tests`, `bdd` | 46 passed; 2 scenarios | the same | 0.8 s; 0.6 s |
| `frontend_tests:bdd` (integration) | 20 scenarios | 20 scenarios, 40 steps passed | 26 s |

The time rule (about 3 s, never past 10 s) is not met by `common_frontend:bdd`, 173 s: each of its 141 scenarios starts a system
group over its logs and a frontend; that target took minutes at the base too (not measured there). `property` is 4 to 4.6 s, of
which the two laws over a played group (L5.3, L5.9, 3 cases each) take most. The shells' targets were not changed in time.

## Scenarios bound

`features/client-endpoint/system_link.feature` (16, over `tests/played_stage.rs`, a played group): One turn's commits share one
Check frame; Two requests of one session in one turn take two frames; A request that ends during an outage is ended on the next
connection; A resend on another core is told the original is on its way; A Begin asks no other process; A core that starts late
begins at the latest close; A token ahead of the core's view waits and is not refused; A token carries the floor to another
connection; A stale view decides nothing; Describe never reports a version being checked; A subscription opened again reads what
its skipped closes announced; A compute process that joins later is found before a read is refused; A source past its failures is
throttled, then banned (bound at the throttle with time it is told); A retry checked in the quiet time stays undecided; A Describe
continues across frames; A Describe past the catalog's bound is refused. The other scenarios task 5.1 names stand bound already in
`features/` (sessions, reads, drops, tightenings, connections, server rules), now run through the link.

Not bound in this group: client-endpoint "The frontend listens at no role address", "A restarted frontend serves the sessions it
served before", "A storage process restarted at its address is dialled again"; schema-changes "A check outlives the frontend that
asked for it", "Dropping a predicate removes it from the predicate lists", "Dropping a vector predicate drops its vector index",
"Dropping a graph keeps the other graphs", "Dropping a tenant's everything survives a restart", "A tenant's drop leaves a tenant
sharing its run"; transactions "A partition purges an entry only after the registry's eviction"; the frontend parts of
role-binaries. Each needs the built binaries with a storage process hosting the group, which group 7's switch brings.

## Corner cases C5.1 to C5.37

Tests named by file under `tests/types/` unless named otherwise; BDD scenarios by name.

| Cases | Test |
|---|---|
| C5.1, C5.3 | `endpoint_view_tests::test_a_view_answers_once_it_holds_a_close_and_a_placement` |
| C5.2 | `endpoint_view_tests::test_a_late_core_begins_at_the_latest_close`; `system_link_tests::test_a_link_begins_at_the_latest_close`; "A core that starts late begins at the latest close" |
| C5.4 | `test_a_token_within_the_open_epoch_is_answered_at_once` |
| C5.5, C5.6 | `test_a_token_ahead_is_held_until_the_close_before_it`; "A token ahead of the core's view waits and is not refused" |
| C5.7 | `test_a_token_past_its_deadline_times_out`; `system_link_tests::test_a_token_ahead_waits_for_its_close` |
| C5.8 | `test_a_close_arrived_at_the_deadline_is_taken_first` |
| C5.9 | not tested: the one-frame path sends a token ahead to its partition (`check_one_frame`), no test names it |
| C5.10 | `test_a_reopened_subscription_at_the_close_held_applies_nothing_twice` |
| C5.11 | `test_a_reopened_subscription_reads_what_the_skipped_closes_announced`; `system_link_tests::test_a_subscription_ended_is_opened_again`; "A subscription opened again reads what its skipped closes announced" |
| C5.12 | `test_a_tenants_version_never_falls`; "A stale view decides nothing" |
| C5.13 | `test_a_schema_id_is_describe_ofs`; `system_link_tests::test_a_schema_id_is_read_once` |
| C5.14, C5.15 | "Describe never reports a version being checked" |
| C5.16 | `system_link_tests::test_defaults_and_a_catalog_are_read_across_frames` |
| C5.17 | `system_link_tests::test_a_close_request_names_partition_0`; "A token carries the floor to another connection" |
| C5.18 | `test_a_close_assigning_a_placement_wants_it_read` |
| C5.19 to C5.27 | `system_turns_tests` (each case its own test, C5.22 and C5.26 one); `system_link_tests::test_one_turns_checks_share_one_frame`; the two turn scenarios |
| C5.28 | `system_link_tests::test_a_check_is_answered_by_its_outcome` |
| C5.29 | law L5.8; "A retry checked in the quiet time stays undecided" (14) |
| C5.30 | law L5.13; no unit test at exactly the bytes one frame holds |
| C5.31, C5.32 | `system_link_tests::test_a_closed_link_refuses_and_carries_its_endeds`; law L5.9 (Register) |
| C5.33 | the same test; "A request that ends during an outage is ended on the next connection" |
| C5.34, C5.35 | law L5.9 |
| C5.36 | law L5.11; "A source past its failures is throttled, then banned" (thresholds 3 and 2, not 50 and 20) |
| C5.37 | `auth_throttle_tests` (the ban's end, the /64), law L5.12; a Verify whose link is lost answering Error 17: no test |

## Laws L5.1 to L5.13

All under `tests/property/`, one `property` target (12 tests).

| Law | Test |
|---|---|
| L5.1 | `endpoint_view_property_tests::test_the_view_never_goes_back` |
| L5.2 | `endpoint_view_property_tests::test_a_held_begin_agrees_with_its_model` |
| L5.3 | `system_link_property_tests::test_a_begin_asks_no_other_process` (phase 4) |
| L5.4 | `endpoint_view_property_tests::test_a_schema_id_is_a_function_of_the_bytes` |
| L5.5, L5.6 | `system_turns_property_tests::test_a_turn_agrees_with_its_model`, a Hegel state machine |
| L5.7 | `recall_order_property_tests::test_a_request_is_routed_exactly_when_its_check_passed` (the outcome's refusal; the routing by the turn scenarios) |
| L5.8 | `recall_order_property_tests::test_a_recalls_answer_follows_its_order` |
| L5.9 | `system_link_property_tests::test_a_closed_link_refuses_what_needs_the_group` (phase 4; the Ended carried by C5.33's tests) |
| L5.10 | `relay_authenticator_property_tests::test_the_relay_authenticates_as_the_server_does` |
| L5.11 | `auth_throttle_property_tests::test_the_throttle_agrees_with_its_model` |
| L5.12 | `auth_throttle_property_tests::test_a_source_is_an_ipv4_address_or_an_ipv6_64` |
| L5.13 | `declare_bound_property_tests::test_a_declare_is_refused_exactly_past_its_frame` |

## Phase 5: fuzzing

No decoder of bytes was added: the link reads every frame through `ultragraphdb_protocol_wire`'s views (`CheckView`,
`EpochCloseView`, `CatalogFrames`, `DefaultsFrames`, `DescribeFrames` among them), whose fuzz targets group 1 wrote, and the relay
reads SCRAM messages through `ScramRelay`, whose readers group 1b fuzzes. No fuzz target was added.

## The gates

`make build` passes. `bazel test //... --test_tag_filters=-integration_test`: 154 targets, all pass (on the tree after 5.1 to
5.4). Run again after the last edits of phase 5 (the Cargo fixes, the `common_role` test): `make build` passes; 153 of 154 pass,
and `common_system:ultragraphdb_common_system_tests` fails one test, `system_group_catalog_tests::
test_a_silent_partition_withdraws_a_tightening_at_the_wait` ("still waiting one tick before the wait ends"), in the crate whose
catalog code the concurrent performance change edits; this group changed no file of it. `frontend_tests:bdd` (integration, run once): 20 scenarios, 40 steps, all pass, the binary hosting the group under the
profiles' settings and its cores dialling it.

## Task 5.5: done 2026-11-16

Approved by the user and applied by the coordinating agent, the files deleted in the working tree (not with `git rm`, which would
have staged them beside another commit):

- Removed whole: `src/types/joined_roles/mod.rs`, `src/types/join_fan_out.rs`, `src/types/core_joins.rs`,
  `tests/types/joined_roles_tests.rs`, `tests/types/join_fan_out_tests.rs`, with their declarations in `src/types/mod.rs`,
  `src/lib.rs` and `tests/types/mod.rs`.
- `Cargo.toml`: `ultragraphdb_traits_log` and `ultragraphdb_transactions_log` leave `[dependencies]`;
  `ultragraphdb_common_system`, `ultragraphdb_cluster_metadata` and `ultragraphdb_cluster_sequencer` move to
  `[dev-dependencies]`, for the group the tests run.
- The Join channel of task 3.2: `SystemShared::new` takes the cluster and the owner alone, and `SystemSession` registers a Join
  with the owner and tells no one else (design D9). Its call sites in `common_system`'s tests, `client_session_tests.rs` and the
  storage service follow.
- `tests/endpoint_stage.rs`: the role listener serves `SystemSession` with no `JoinedRoles`; `partition_with` and
  `compute_with`, and the restarted stage's re-joining partition, read Members through the stage's link after their Join
  (`Stage::read_members`), as a frontend reads it at its start until every partition is listed.
- The binary: `Hosted`, `host_the_group`, `owner_start`, `required`, `accept_roles`, the join wiring and the group's outcome in
  the main loop are gone; `cluster.system_address` is required (`common_role::system_address`). `ultragraphdb_cluster_metadata`,
  `ultragraphdb_cluster_sequencer`, `ultragraphdb_common_system` and `futures` leave its manifest.
- `AGENTS.md`: the `common_system`, `common_frontend`, `frontend` and `futures` rows.

Evidence: `bazel build` of the frontend binary, storage, `common_frontend` and `common_system` passes; their seven non-integration
test targets pass. A profile naming only `cluster.frontend_address` now has no host for the group, so the integration targets
starting the frontend binary fail until task 7.4 rewrites the profiles (design D2, D12).

## Found wanting

1. **A version past the cap cannot be built by one transaction.** metadata-group, "A catalog version past its cap is refused
   at its Commit" (`features/metadata-group/tenants.feature`): "a transaction's declarations build a catalog version of
   16,777,217 bytes". `PROTOCOL.md` §11.13, *A new version*: "A Declare is one frame: a Write that takes a transaction's
   declarations past what one Declare frame holds within the role connection's `max_frame` is refused with code 6", and design
   D13: "one transaction adds at most a frame of declarations to the version it builds on". Implemented: the scenario's
   version builds on one of 16,002,000 bytes declared in the group itself, and the transaction's one Write adds 896,000; the
   assertion (code 7, `detail` 16,777,216, the current version unchanged) stands. The scenario's text is the user's to decide.
2. **A Begin that asks no other process, and a tenant read at its first use.** client-endpoint, "A Begin asks no other process"
   against §11.13, *The frontend's reads*: a tenant's current version comes "from one Catalog read when a tenant is first used".
   A Catalog at a Begin's first use of a tenant is a frame to the system group. Implemented: a core uses a tenant first when its
   placement names it, and reads each placed tenant's current version with the placement; a tenant the placement does not list
   is refused by the gate before any read.
3. **Two scenarios of earlier stages contradict this stage's text**, and were retired from `common_frontend`'s features: "A token
   above the open epoch is refused" (§4.8 now holds the Begin and sends the write to its partition) and "A Describe past the
   client's frame is refused" (§8.3, *Continuation*). `ultragraphdb/ultragraphdb_tests/features/transactions/tokens.feature`
   holds the first too, over the binaries; it is not this group's crate, and will fail at group 7's gate until it takes the new
   text.
4. **The golden `describe_reply` frame is not flagged `LAST`.** §11.2: "Every reply that ends its stream carries `LAST`: … a
   Describe's last frame"; §8.3. `ultragraphdb_protocol_wire/testdata/golden/bodies/describe_reply.frame` holds flags 0 (as do
   `register` and `prepare_reply`, which no client reads by `LAST`). The client tests flag it where they serve it; regenerating
   the golden frame is the wire's.
5. **A placement version.** An EpochClose names a placement version a close assigns (§13.4); a Placement reply names its log
   `position` and epochs, no version. Implemented: the view holds, with the map it read, the version the last close named, and
   reads Placement again when a close names another (C5.18).
6. **Which partition's `detail` answers code 13.** §11.5: "code 13 when any answered 13, `detail` that partition's last". With two
   answering 13 the text names no one. Implemented: the first in the order the answers arrived.
7. **A Recall ends with no Ended.** §11.5: a check of kind 1 "marks nothing"; the owner's in-process recall marked one, which an
   Ended released. Implemented: a Recall's checks are of kind 1 and send no Ended.
8. **Readiness, D10.** "the group answers, the view holds a placement and a close, Members lists every partition and one
   answers". Implemented: ready once the view answers and Members lists every partition of its placement; that a partition
   answers is not awaited.
9. **A Begin naming a token ahead waits in its request.** §4.8 holds it; the session's `answer` awaits the close, so the
   connection reads no further frame meanwhile, within the Begin's deadline (other connections go on). Not a rule of the text;
   recorded as this implementation's limit.
10. **`futures` without `std`.** The workspace takes `futures` with no default features, and `common_system` (and
    `common_frontend`, before this group) name `futures::channel`, which needs `std`: a Cargo build of a package that does not
    enable it otherwise, the frontend binary's, fails in `common_system` (`E0432`, `futures::channel`), which is what stopped
    `scripts/mutants.sh ultragraphdb_frontend` in its baseline. `common_frontend` now declares `features = ["std"]`;
    `common_system`'s manifest is another group's.

## Phase 5: mutation testing, not complete

`scripts/mutants.sh <crate> --diff 443a7f54`, one run at a time under the script's lock:

| Crate | Runs counted | Outcome |
|---|---|---|
| `ultragraphdb_client_core` | 1 | 13 mutants: 9 caught, 4 unviable, 0 missed |
| `ultragraphdb_common_role` | 2 | run 1: 12 mutants, 3 caught, 7 unviable, 2 missed; a test added (`serve_tests::test_a_connection_authenticates_by_the_authenticator_it_is_served_with`); run 2, `--re 'machine_of\|serve_connection_with'`: 6 mutants, 1 caught, 4 unviable, 1 missed |
| `ultragraphdb_protocol_transport` | 1 | 1 mutant, unviable |
| `ultragraphdb_common_frontend` | 2 | no mutant tested: both runs stopped in the unmutated baseline (below) |
| `ultragraphdb_frontend` | 1 | no mutant tested: the baseline's Cargo build failed in `ultragraphdb_common_system` (Found wanting 10) |

Open survivor, for the stage's final verification: `common_role` `src/utils/serve.rs:169`, `replace == with != in
Server::machine_of`, which gives a client endpoint's connection a role connection's idle limit and the reverse. The code is the
idle-limit choice that stood in `Server::machine` at the base and moved with the refactor; telling the two limits apart needs a
connection held idle past the client's limit, a real-time wait the suite does not take.

`common_frontend`'s two runs: the first found 458 mutants and stopped at Cargo's baseline build, which denies `unused_results` in
test targets where Bazel's do not (`tests/played_stage.rs`, `tests/property/types/endpoint_view_property_tests.rs`), and warned
of a field this group left unread (`OneFrameFacts::token`); each was fixed. The second found 460 mutants, built, and stopped at
its baseline's tests: 4 of 141 scenarios failed under `cargo test` ("A client past its bound stops being read until its
partition reads again", "A claim whose home's connection closes under it is unavailable", "A one-frame write and a Drop whose
partition closes under them are unavailable", "A first read moved by a stale map is resent"), each at an `until` of the stage
that did not come to hold within its 5 s, while the same scenarios pass under Bazel, alone four times and in the full target.
Not root-caused in this run: the third and last run is left for after that, since it would stop at the same baseline.
