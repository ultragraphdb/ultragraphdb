# Stage 5.9, group 0, task 0.7: one reader rule in `ultragraphdb_common_role`

Base commit `64887405`. Blocker B4 of `feasibility.md`; backlog item 23, "One reader rule".

`PROTOCOL.md` section 2.5 step 3 and section 2.6 state how the side that dialled reads the frames that answer a
request. The rule was written three times: `RoleClient::request` with `arrived`, storage's `catalog_frame` with
`next_taken`, and storage's `subscribed`. It is now `RoleClient::next_answer`, and the three callers go through it.

## The three copies compared

| Case | `RoleClient::request` / `arrived` | storage `next_taken` / `catalog_frame` | storage `subscribed` | Decided by the specification |
|---|---|---|---|---|
| Frame on the request's stream, a command the request is answered by | the reply | taken into the version | read as a close | The answer (2.5 step 3). All three agreed |
| Late frame: stream nonzero, below the request's | checked whole by `view()`, dropped, the next read in the same loop | the same | returned as `Late`; the caller's loop looked at `stopping` before the next read | Checked whole and dropped (2.5 step 3, 11.2). The one method loops; the stop is not looked at after a late frame, which changes nothing a test or a process sees, since the read that follows waits as the one before did |
| Late frame failing a frame-level rule | refused, `shutdown()` | refused; closed by the caller dropping the client | the same as the catalog's | Refused, connection closed (2.6). One close, in the method |
| Peer's Error on the request's stream | `TransportError::refused` | `TransportError::refused` | code 1 the subscription's end (`Ended`); any other code `refused`, joined again | The peer's refusal; the stream ends, the connection stays open. The method gives `Answered::Refused`, and the caller decides what a code means |
| Peer's Error on stream 0 | `TransportError::refused`, not told apart from the stream's | the same | `refused`, joined again, whatever the code | About the connection (2.6). The method gives `Answered::ConnectionRefused`; only the subscription needed the two told apart |
| Error whose body does not read | refused, `shutdown()` | refused; closed by the drop | the same | A frame this side refuses: closed (2.6, 13) |
| Frame on stream 0 that is no Error | `CommandNotServed`, `shutdown()` | `CommandNotServed`; closed by the drop | the same | Refused and closed (2.5 step 3, 2.6) |
| Frame on a stream above the request's | `CommandNotServed`, `shutdown()` | `CommandNotServed`; closed by the drop | the same | Refused and closed |
| Command the request is not answered by | `CommandNotServed`, `shutdown()` | the same rule; closed by the drop | the same | Refused and closed |
| **Frame whose header breaks a rule, found by the framer inside `read_frame` (steps 1 to 8)** | **the error returned, the connection left open while the client lives** | closed by the caller dropping the client | closed by `drop(client)` before the rejoin | **A client refusing a frame closes the connection (2.6). A defect of `request`'s copy, fixed test first; see below** |
| Which stream is the request's | `self.stream`, the last opened | passed in by the caller, always `client.last_stream()` | the same | One fact held once: the method reads `self.stream`, and no caller passes a stream |
| Which command the request is answered by | `B::COMMAND` | `Command::Catalog`, written in the function | `Command::Subscribe`, written in the function | A parameter, `request: Command` |
| The frame's view | validated in `arrived`, again by the caller | validated once, the view handed on | validated once | The method returns the frame, as `request` always did, so storage's two readers now validate a frame twice. Control plane on both paths: a catalog version at a start or a close that announces one, and one close per epoch |

How the connection closes was the widest difference: `request` shut the connection down itself, and storage relied on
every caller dropping the client. Every caller did. The method owns the close now, so the next caller cannot leave it
out. A frame whose body breaks a rule of step 16 is still the caller's to refuse, by dropping the client, since only
the caller reads the body.

### The defect, fixed test first

`RoleClient::request` returned a `read_frame` error with `?`. A frame refused by the connection's framer, by its
header (steps 1 to 8: its checksum, its cluster, its size), came back as `TransportError::protocol` with the
connection still open for as long as the caller held the client. Section 2.6: "A client refusing a frame from the
server closes the connection." The refusals found later, by `view()`, did close it.

`test_a_request_answered_by_a_header_breaking_a_rule_closes_the_connection` answers a request with another
cluster's frame. Against the unchanged `request` it failed `"a request answered by another cluster's frame: the
client kept its connection"`; it passes now. `next_answer` shuts the connection down on every error, a failed read
included: a connection whose read failed is given up either way, and one branch fewer stands between a refusal and
its close.

No caller in the repository kept a client past such an error, so no process behaved wrongly.

## The method

```rust
// ultragraphdb_common_role/src/types/role_client/role_client_answer.rs
impl RoleClient {
    pub async fn next_answer(&mut self, request: Command) -> Result<Answered, TransportError>;
}

// ultragraphdb_common_role/src/types/answered.rs
#[derive(Debug)]
pub enum Answered {
    Frame(ReceivedFrame),
    Refused { code: ErrorCode, message: String },
    ConnectionRefused { code: ErrorCode, message: String },
}

impl Answered {
    pub fn frame(self) -> Result<ReceivedFrame, TransportError>;
}
```

`next_answer` reads the next frame answering `request`, a request sent on the last stream the client opened:

- `Answered::Frame`: a frame on that stream, of a command `request` is answered by. Its `FLAG_LAST` says whether
  the peer ended the stream with it.
- `Answered::Refused`: the peer's Error on that stream. The stream ends, the connection stays open.
- `Answered::ConnectionRefused`: the peer's Error on stream 0. With code 1 it is a connection error and the peer
  closes; the method closes nothing.
- `Err`: the read failed, or the client refused a frame; `TransportError::protocol` names the rule. The method has
  shut the connection down before it returns.
- A late frame is checked whole and dropped inside the method, and the next frame read.

Why this shape:

- **One method serves the three uses.** A request of one reply calls it once (`request` is `send`, then
  `next_answer(B::COMMAND).await?.frame()`). A reply in several frames calls it once a frame, with its credit between
  the calls (the catalog). A long-lived stream calls it in its loop (the subscription).
- **No stream parameter.** A `RoleClient` takes its earlier streams as ended, so the request's stream is the last it
  opened. Storage passed `client.last_stream()` at both sites; a parameter could only be passed wrong.
- **The request is a `Command` parameter**, since a stream may carry more than the request that opened it (a Write on
  a Bind's stream is answered by Held), and the client does not record what it sent.
- **The peer's Error is in `Ok`**, in two variants the caller matches exhaustively. The peer refusing a request is an
  answer, and the connection lives on; `Err` is left to mean the connection is given up. Storage's subscription needs
  the two variants apart: Error 1 on its stream ends the subscription for good, Error 1 on stream 0 is a connection
  to dial again.
- **`Answered::frame`** folds both refusals into `TransportError::refused`, which is what `request` has always
  returned and what the catalog read wants. One conversion, in the crate, used by both.
- **Code and message, no `ErrorReply`.** The three copies read exactly these two. An owned `ErrorReply` needs
  `ErrorReplyView::to_body`, which parses the reply's value and can fail, for a field no reader here uses.
- **The stream's end is the frame's flag**, read by the caller as `taken` reads it. The client keeps no record of
  ended streams beyond "below the last".

Static dispatch, no trait object, no macro; `Answered` is a small type in a file of its own, the method a file beside
`role_client_split.rs`, the private classification `arrived` beside it.

## The callers

- `RoleClient::request`: `self.next_answer(B::COMMAND).await?.frame()`. The private `Arrived` and the old `arrived`
  are gone.
- Storage `catalog_read`: each frame after the first is `client.next_answer(Command::Catalog).await?.frame()?`.
  `next_taken` and `catalog_frame` are gone.
- Storage `deliver_closes`: `client.next_answer(Command::Subscribe).await.and_then(subscribed)`. `subscribed` now
  maps an `Answered` (a close, the subscription's end by Error 1 on its stream, any other Error an error);
  `Subscribed::Late` and the `subscription` stream variable are gone.
- `service.rs`: 1,530 lines before, 1,459 after. `ultragraphdb_storage` no longer names `ProtocolError`, so its
  manifest drops `ultragraphdb_error_protocol`, and `Cargo.lock` the one line for it.

`RoleClient::next_frame` stays: tests and `common_compute` read with it.

## Test first

Corner cases, listed before the suite, each with its test in
`tests/types/role_client/role_client_answer_tests.rs`:

| Case | Test |
|---|---|
| A frame on the request's stream | `test_a_frame_on_the_requests_stream_is_the_answer` |
| A reply in several frames, the last alone flagged | `test_a_reply_in_several_frames_is_read_a_frame_a_call` |
| Late frames before the first frame and between frames; a late Error; a late frame of the reply's own command | `test_late_frames_before_and_between_a_replys_frames_are_dropped` |
| A late frame failing its checksum | `test_a_late_frame_failing_its_checksum_closes_the_connection` |
| The peer's Error on the request's stream; the connection usable after it | `test_the_peers_error_on_the_requests_stream_is_its_refusal` |
| The peer's Error on stream 0, code 1 and another code | `test_the_peers_error_on_stream_0_is_the_connections` |
| A connection error, then the connection's end | `test_a_connection_error_is_read_before_the_connections_end` |
| A frame on stream 0 that is no Error | `test_a_frame_on_stream_0_that_is_no_error_closes_the_connection` |
| A stream not opened: the very next, one far past it, an Error there | `test_a_frame_on_a_stream_not_opened_closes_the_connection` |
| A command the request is not answered by | `test_a_command_the_request_is_not_answered_by_closes_the_connection` |
| The same frame under another request named | `test_the_request_named_decides_what_answers_it` |
| An Error whose body does not read, on the request's stream and on stream 0 | `test_an_error_whose_body_does_not_read_closes_the_connection` |
| A header breaking a rule, found by the framer | `test_a_frame_whose_header_breaks_a_rule_closes_the_connection` |
| The same through `request` (the defect) | `test_a_request_answered_by_a_header_breaking_a_rule_closes_the_connection` |
| `request` failing by the peer's Error on either stream | `test_a_request_fails_by_the_peers_error_on_either_stream` |
| `Answered::frame` for a frame | `test_an_answers_frame_is_given_back_whole` |
| `Answered::frame` for each refusal (`tests/types/answered_tests.rs`) | `test_a_refusal_on_the_requests_stream_is_the_requests_failure`, `test_a_refusal_on_stream_0_is_the_requests_failure` |

Every refusal's test waits for the server to see the close while the client is still held, so the close seen is the
method's and no drop's. The wait is a channel the server's task sends on, under a timeout; no test sleeps.

The fake is `Scripted`, a `Session` served by `serve_connection`: it answers the client's first request, which ends
stream 1, leaves the second open, and pushes the test's script on it, each frame naming whatever stream its own
header says.

**Phase 1.** `Answered`, `Answered::frame` and `RoleClient::next_answer` with `unimplemented!()` bodies; the library
built.

**Phase 2.** 18 unit tests and 1 property test written against that surface. The unit target went from 51 tests to
69: `52 passed; 17 failed`. 16 failed `not implemented`, at `answered.rs:41` or `role_client_answer.rs:33`. One,
the defect's, failed `the client kept its connection` against the unchanged `request`. One passed,
`test_a_request_fails_by_the_peers_error_on_either_stream`: it pins what `request` already did, so that moving
`request` could not change it unseen. The property target: `1 passed; 1 failed`, `not implemented`.

**Phase 3.** Inputs reviewed for values that coincide. The reply's frames list 1, 2, 3 members, so a frame out of
order or twice shows. Late frames carry three commands. The two Error tests use different codes and messages. A
stream not opened is tried at the boundary, the request's stream plus one, and far past it. The unit tests' late
frames all sit on stream 1, since their client has ended one stream; the property draws 1 to 3 ended streams.

**Phase 4.** `ultragraphdb_common_role_tests`: `69 passed; 0 failed`. `property`: `2 passed`. `loom` and `bdd`
unchanged and green. `role_client_tests.rs` and `serve_tests.rs`, which hold `request`'s tests from stage 5.8, are
untouched and pass.

**The property.** `test_a_reader_is_given_the_replys_frames_alone_and_in_order`, 30 cases, each a connection of its
own: 1 to 3 ended streams; a reply of 1 to 5 frames; before each frame 0 to 3 late frames, a Held, a Members or an
Error, each on a drawn ended stream; in half the cases a frame the rule refuses in place of one of the reply's (a
Members on stream 0, a Members 1 to 4 streams ahead, a Held on the request's stream). The caller is given the
reply's frames alone, in order, up to the stray frame, then its refusal. What is due is computed from the draws.
Bite: with `Ordering::Less => Ok(None)` changed to hand the late frame out, the property failed (`a frame of stream 1
was given`), as did three unit tests; the edit was reverted.

No Loom model: the method shares nothing across threads. No fuzz target: it decodes nothing the wire crate's views do
not already decode.

## The gate

- `bazel test //ultragraphdb_common/ultragraphdb_common_role/... //ultragraphdb/ultragraphdb_storage/...
  //ultragraphdb/ultragraphdb_storage_tests/...`, one invocation: 9 targets pass. `storage_tests:bdd`: 103 scenarios,
  206 steps, all passed, the five of `lost_group.feature` among them, unchanged.
- The clippy aspect on the three packages: clean.
- `rustfmt` on the files touched: clean.
- `make build`: passes.

## Phase 5: mutation

`MUTANTS_JOBS=6 scripts/mutants.sh ultragraphdb_common_role --diff 64887405`, run 1 of 3. The unmutated baseline
passed (17 s build, 3 s test).

| Run | Mutants | Caught | Missed | Unviable | Timeouts |
|---|---|---|---|---|---|
| 1 of 3 | 10 | 4 | 0 | 4 | 2 |

| Mutant | Outcome |
|---|---|
| `role_client_answer.rs:68:19` `==` with `!=` in `arrived` (`stream == 0`) | caught |
| `role_client_answer.rs:81:32` the guard `command == Command::Error` with `true` | caught |
| `role_client_answer.rs:81:32` the same guard with `false` | caught |
| `role_client_answer.rs:81:40` `==` with `!=` in that guard | caught |
| `role_client_answer.rs:69:24` `!=` with `==` in `arrived` (`command != Command::Error` on stream 0) | timeout; fails under Bazel, below |
| `role_client_answer.rs:65:9` `arrived` with `Ok(None)` | timeout; fails under Bazel, below |
| `answered.rs:41:9` `Answered::frame` with `Ok(Default::default())` | unviable |
| `role_client/mod.rs:104:9` `RoleClient::request` with `Ok(Default::default())` | unviable |
| `role_client_answer.rs:36:9` `next_answer` with `Ok(Default::default())` | unviable |
| `role_client_answer.rs:65:9` `arrived` with `Ok(Some(Default::default()))` | unviable |

No mutant survived, so no test was added, `.cargo/mutants.toml` is unchanged, and no `--re` run was needed.

The two timeouts, each applied by hand and run under Bazel, then reverted:

- `69:24`: `ultragraphdb_common_role_tests` `62 passed; 7 failed` in 0.3 s; `property` failed in 0.02 s. Under Cargo
  the property target ran first and failed too, and Hegel then shrank the failure, which cargo-mutants cut at 20 s:
  `scripts/mutants.sh` does not set `HEGEL_PHASES` as `.bazelrc` does. A fail, reported as a timeout.
- `65:9`: every frame is dropped as late, so a read never returns. `ultragraphdb_common_role_tests`
  `41 passed; 28 failed` in 20 s, each failing test on its own bounded wait (`the test within 10s`); `property` failed
  (`the case within 10s`). A hang the suite notices.

cargo-mutants generates no mutant for the `shutdown()` on a refusal. The tests pin it: each refusal's test waits for
the server to see the close while the client is held, and the defect's test failed for want of it.

The run's output is `mutants.out/` at the repository's root, where `scripts/mutants.sh` writes it without a slot.

## Whether the method fits the other readers

Not changed, as the task says. For the groups that rework them:

- **`common_compute`, `ComputeCore::catalog` (`read_catalog`)**: fits as it stands. It is a `RoleClient` reading a
  Catalog across frames. Its credit count and its answering every refusal with code 17 stay around the call.
- **`common_compute`, `closes` with `late` (`joining.rs`)**: does not fit as it stands. It reads a connection with no
  request open, where every frame is late or refused, the last stream included. `next_answer` takes the last stream
  as the request's. It would fit if the client recorded that the last stream ended.
- **`common_compute`'s `Link`, `common_frontend`'s `RoleLink` and `SystemLink`**: do not fit. Each reads a
  `RoleReader`, the split half, with many streams open at once and a table of who holds each. `next_answer` reads one
  stream of an unsplit client. What they share with it is the classification alone (stream 0, below the next stream,
  at or above it), which could be one function over the next stream and the frame's header if those groups want it.

## Found, not changed

- `held_versions` takes `Refused` with code 7 as the end of a tenant's versions wherever it came: on a later frame
  of a version (backlog item 23 has this) and on stream 0. `Answered` now tells the two apart, should that be decided.
- `common_compute` holds two more copies of the rule on a `RoleClient`, beyond the three this task names:
  `read_catalog` and `closes`.
- `ultragraphdb_storage`'s manifest named `ultragraphdb_error_protocol`, which `AGENTS.md`'s crate table never
  listed for it. The dependency is gone with its last use.
