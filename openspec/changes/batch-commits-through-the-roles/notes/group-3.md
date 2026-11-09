# Group 3: the session reads on

## 3.1 and 3.2 The session hands every request on; the core pushes every answer

`StorageSession::answer` checks the frame's order on its connection (`ReceivedFrame::check_order`, header only), copies it into a
`ParkedFrame` counted against the connection's `ParkedBytes`, drops the `ReceivedFrame` so the connection's one pooled buffer is
free, hands the copy to the core in `CoreMessage::Request` and answers `Answer::Open`. A frame out of order is refused at once; a core
gone, as the partition stopped. The `done` channel, the session's `looked_at` queue and `ending`, which patched the order between the
two channels, are gone, and so are `CoreMessage::Opened`, the core's map of each connection's `ParkedBytes` and its `copied`: the
session counts what it hands on, and a parked request keeps the copy it was handed. `ParkedFrame` is public, as tests and sessions
build the messages the core takes.

The core answers every request through the connection's push channel, in its order: a reply as `Push::Frame`, a stream's end with
no frame as `Push::End`, and a refusal as `Push::Refuse`. A pushed Error frame would not reach a stream the connection never opened,
since `serve_connection` drops a pushed frame for a stream it does not hold open; the first integration run hung in
`ultragraphdb_storage_tests` on "A Write on a stream no Bind opened is refused" for that reason. `Push::Refuse` goes through the
machine's `refuse`, as `Answer::Refuse` did, on the stream or on the connection's own queue; a refusal crossing the peer's Cancel
reaches it late, and the frontend's role link drops a frame on a stream that ended. `push_error` pushes `Push::Refuse`. The core's
stop refuses, with code 17, each request still waiting in its inbox, at most a turn's worth, as it refuses parked ones; a dropped
core future leaves them to their connection's close, as its documentation states.

`PROTOCOL.md` is unchanged since 1.1; no frame changes.

### The tests moved with the message

The tests that played a session or a core through `done` read answers from the pushes instead:

- `played_answers.rs` (new): a connection a test plays, which hands a request and then a probe, an Abort on a stream far above any a
  test numbers, which the core refuses at once. The core takes both in one turn, so what it pushes on the request's stream before
  the probe's refusal is the answer it gave on taking it, and nothing there is `Answer::Open`; what else comes is kept for later
  reads. The 134 call sites of `Driver::ask` and `Direct::request` stay as they were; `answered(sent)` moved onto the connection.
- The played-core session tests: the played core pushes both the stream's end and the crossed refusal, and `serve` drops the second;
  "counts what it hands its core" replaces "announces what it counts parked"; "a request out of order is refused at once" is new;
  "a refusal gives way only to its own stream's end" (the removed `ending`) and "a request its core drops is refused as stopped" (no
  `done` left to drop) are gone, the second's case now the core's stop, tested in 3.5.
- `ultragraphdb_compute_tests`' tap held a Scan until the Cancel behind it and let both go together, relying on the partition taking
  one request at a time ("the Scan has run once compute's Cancel has gone on after it"). The partition reads on now, so the Cancel
  reached the core in the Scan's turn and the span was never recorded: "A span recorded after a Cancel only widens the commit's check"
  failed. The tap now holds the frame behind the released Scan until the partition has replied on the Scan's stream, bounded as its
  pause is, so the scenario's order is enforced rather than assumed.

`serve_tests`: `test_a_pushed_refusal_reaches_a_stream_never_opened` (new) fails with `serve` dropping the refusal and passes with it
sent.

## 3.3 `role connection phases`

`RoleConnectionPhases` (`ultragraphdb_common_storage`): a `PhaseWindow<1>` shared by a partition's sessions and its core, on the
core's clock (`micros_since`). A session given it (`StorageSession::with_phases`) keeps each stream's requests' read times in order
and records the oldest at the stream's next push; a stream's end, a refusal and a Cancel forget the rest, so the map holds at most the
requests the connection holds handed on. The core logs `role connection phases` (requests, read-to-answer p50, p99 and mean) beside
`commit phases`, whose `commits` and `log_syncs` give commits per sync. The storage role builds one per partition before it listens.
Tests: the window and its clones, an answer before its read counting as no time, and the session counting a stream's first answer
only, a cancelled stream and an ended one counting nothing more.

## 3.4 The burst test

`test_a_burst_of_answers_leaves_one_role_connection`: 1,000 DirectWrites sent on one role connection while the log's sync is held,
answered in one round once it is let go, and read back one frame at a time through `RoleClient`, whose pool of one buffer is the
frontend's role link's. The whole frontend endpoint is not built in process: its relay to the client is measured in group 6 through
`one-frame write phases` beside `role connection phases` (design D10). `-c opt`, this Mac (Apple silicon), the log on the APFS disk:

| Answers | Log syncs | Release to first answer | First to last | Answers read a second |
|---|---|---|---|---|
| 1,000 | 2 (the held commit's round and one for the 1,000) | 34.3 ms | 28.5 ms | 35,108 |

One connection's reader takes about 28 µs an answer, above one partition's in-process rate (12.5: 11,881 to 14,235 relations a second
at one partition, 35,984 over 3 s) but within a factor of three of it; group 7 makes the core faster, and the reader is then the next
place a burst waits. No bound is asserted until the owner sets one.

## 3.5 A real sync held

`PartitionCore`, `CoreParts` and `StorageHost` take the log's storage as a type parameter, `S: DurableStorage = CompioStorage`, so
the storage role is unchanged. The tests open the rig's log over `GatedStorage` (`gated_storage.rs`, new), which syncs a file only
once the test lets its `SyncGate` go; `syncing` starts a commit on a connection of its own and waits until its sync waits at the gate.
Every scenario of 1.2 and 1.3 runs with that sync in flight; the inbox tap only counts, and its hold is gone. New:
`test_a_stop_refuses_the_requests_waiting_for_the_core`, a request sent during the held sync, the core then stopped, refused with
code 17. `with_core_then` gave `fill_open_epoch` its setup of a full epoch, to stay within the 70-line limit.

## 3.6 One round per intake (design D12)

With a real sync held, the ten commits of the one-round scenario took three rounds: the held commit's, then one for the first of the
ten, closed at once because no round was in flight when the core took it after the sync, then one for the other nine. The owner chose
one round per intake: `Input::Hold` and `Input::Release` in the commit step (`CommitStep::release`), `Pipeline::hold_rounds` and
`release_rounds`, `PartitionHost`'s, and `PartitionCore::run` holding around each turn's intake. The scenario counts two rounds and two
syncs; with the core's hold taken out, three (the proof run). Commit step tests: commits handed over under a hold share one round; a
release with nothing waiting closes none, and a lone commit after it closes one at once; a release while a round syncs waits for that
sync. The commit-rounds requirement states it, with the scenario "Commits a turn takes together share one round".

## 3.7 The suites

All on this Mac at the tree of this group, 2026-11-09:

- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all` clean.
- `make test` (OrbStack running, so the supervisor's Docker image tests ran): exit 0; doc 21 of 21 targets, unit 79 of 79, BDD 37 of
  37, integration 7 of 7 (`ultragraphdb_storage_tests`, `ultragraphdb_compute_tests`, `ultragraphdb_frontend_tests`,
  `ultragraphdb_supervisor_tests` with `image_bdd`, `ultragraphdb_config_tests`, `ultragraphdb_tests`).
- The five role batteries built for `aarch64-unknown-linux-gnu` and run under Apple's `container` (1.7.1) in a Wolfi container as
  user 65532, `/tmp` a named volume (ext4, since `O_DIRECT` refuses tmpfs), 8 CPUs and 8 GiB:

| Battery | Scenarios | Time |
|---|---|---|
| `ultragraphdb_storage_tests` | 98 of 98 | 54 s |
| `ultragraphdb_frontend_tests` | 20 of 20 | 11 s |
| `ultragraphdb_compute_tests` | 88 of 88 | 21 s |
| `ultragraphdb_supervisor_tests` | 28 of 28 | 11 s |
| `ultragraphdb_config_tests` | 44 of 44 | 17 s |

`container` has no seccomp option, so these run without the shipped profile; the Kubernetes deployment exercises it (task 5.4). The
first run failed 19 storage and 25 supervisor scenarios with `Permission denied`: a `container` named volume mounts root-owned with
mode 755 where Docker's copies the image's `/tmp` (1777), and the tests make their Unix sockets' directories in `/tmp`. The runner
gives the volume mode 1777 before each battery; nothing in the repository changed for it.
