# Group 1: protocol and scenarios first

## 1.1 `PROTOCOL.md` §11.12

*Parked requests* opens with what a partition does with every request: it copies it out of the receive buffer as it arrives, hands
the copy to its core and reads the next frame without waiting for the core to take it, so the requests a connection carries during
a sync reach the core together and their commits share the next round. The bound counts a connection's requests copied and not yet
taken together with those parked, against `storage.held_bytes`; *stopping cannot deadlock* gains that a request waiting for its
core is released by the core's own turn, which waits on no connection. No frame, command or error changes.

## 1.2 and 1.3 The scenarios, written first

All in `ultragraphdb_common/ultragraphdb_common_storage/tests/types/storage_session_tests.rs`, on the core rig
(`partition_core/core_rig.rs`): a real `PartitionCore` over fjall and its write-ahead log, each connection a real `StorageSession`
served by `serve_connection`, dialled as a role dials it.

The rig gains a tap between the sessions and the core's inbox (`partition_core/inbox_tap.rs`). While the tap holds, the core takes
nothing, as while its turn waits on its log's sync (`PartitionHost::turn` awaits every sync); the tap counts the requests and the
closes the sessions hand on, and released it hands the core everything kept, in order. Holding the log's sync itself would need
`PartitionCore` generic over its log's storage, a change to `src/` for a test; the tap holds the core at the boundary the change is
about, with no change to `src/`. The tap matches `CoreMessage::Request { .. }` and `CoreMessage::Closed { .. }` by variant alone, so
the tests survive group 3's change to the request message. The rig also takes `held_bytes` from its setup and exposes the log's
`LogIo`, whose totals count the log's writes and syncs; its log opens in `open_log`, out of `with_core_then`, which had reached the
70-line limit.

| Scenario | Test | Against the code of `b010b64f` |
|---|---|---|
| wire-protocol "A connection is read on while its core syncs" | `test_a_connection_is_read_on_while_its_core_syncs` | fails: 1 of 10 requests reached the core's inbox within 10 s |
| wire-protocol "Requests waiting for the core count against the bound" | `test_requests_waiting_for_the_core_count_against_the_bound` | fails: 1 of 3 |
| wire-protocol "An Abort's stream ends with no frame" | `test_an_aborts_stream_ends_with_no_frame` | fails: 2 of 3 (the Bind, then the Abort) |
| commit-rounds "Commits a frontend sends during a sync share the next round" | `test_commits_a_frontend_sends_during_a_sync_share_the_next_round` | fails: 1 of 10 |
| a transaction's requests waiting for the core answered in order | `test_a_streams_requests_waiting_for_the_core_are_answered_in_order` | fails: 2 of 4 (the Bind, then the first Write) |
| a Cancel crossing a request waiting for the core | `test_a_cancel_crossing_a_request_waiting_for_the_core_ends_its_stream` | fails: 2 of 3 |
| a connection closing with a Write waiting for the core aborts it | `test_a_connection_closing_with_a_write_waiting_for_the_core_aborts_its_transaction` | fails: 0 of 1 closes (the session still waits on the Write) |

Each failure is the defect: the session waits for the core to take a request before the connection reads the next frame. The
commit-rounds test also asserts one log write and one sync for the ten commits and a version of its own for each; the bound's test
derives how many frames reach the bound from the frame's measured size (1,504 bytes for about 1,500 asked, three under 4,096).

`bazel test //ultragraphdb_common/ultragraphdb_common_storage:ultragraphdb_common_storage_tests`: 208 tests, 201 passed, the 7
above failed. `cargo clippy -p ultragraphdb_common_storage --all-targets -- -D warnings` and `cargo fmt` clean.

Kept passing where they stand, not duplicated here: a request parked on a home among a stream's requests, and a Cancel or a close
reaching a parked write (`ultragraphdb_storage_tests`, `parked_requests.feature`, task 3.5); a refusal crossing its stream's end
(`test_a_refusal_crossing_its_streams_end_gives_way_to_it`, which plays a core answering through the `done` channel and is adapted to
the one channel in group 3).
