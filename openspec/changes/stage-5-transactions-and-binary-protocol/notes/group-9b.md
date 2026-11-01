# Group 9b: the Rust client, the transport renamed and split

## Phase 2: the failing run

`cargo test -p ultragraphdb_client_core --no-fail-fast` against the surface (every `ClientCore` method
`unimplemented!()`), 2026-10-28 14:14: 26 tests, 22 failed, 4 passed. The 4 are `ClientError`'s own tests
(constructors, `retryable` over every `ErrorCode`, `is_stale` and `closes_connection`, `Display`), whose type is data
and was written whole. Every failure was an `unimplemented!()` panic in `src/types/client_core/mod.rs`; no compile
error, no panic elsewhere.

The run came after the transport's rename and split below; the client's surface is the one listed under 9b.2.

## 9b.7 The rename

`ultragraphdb_protocol_bus` became `ultragraphdb_protocol_transport` (`git mv`), with its fuzz package
(`fuzz/ultragraphdb_protocol_transport`, package `ultragraphdb_protocol_transport_fuzz`), `BusError` →
`TransportError`, `BusErrorEnum` → `TransportErrorEnum`, the module and test files `bus_error*` → `transport_error*`;
the root `Cargo.toml`, `fuzz/Cargo.toml`, `ultragraphdb_server_session`'s manifest, the BUILD targets, design.md's and
the design note's crate tables, tasks.md, proposal.md and AGENTS.md's compio row follow. Before and after, Cargo: 33
unit tests and 5 scenarios, all passing. Historical notes (group-9.md) keep the old name.

## 9b.8 The transport split, sans-I/O

- `ultragraphdb_protocol_transport` has no runtime dependency (compio removed from its manifest). New pure machines:
  `Framer` with `FrameStep` (pull-style: `want()` is the length the frame's pooled buffer must reach; `advance` verifies
  the header for the cluster and the buffer's capacity, then asks for the frame's `size`; `closed(received)` is
  `Closed` between frames and `Truncated` inside one; `finish` hands over the `ReceivedFrame`), `ClientHandshake`
  (`hello_frame`, `receive`), `ServerHandshake` (`receive` → `HandshakeAnswer`: the frame to send and the outcome).
  `PooledBuffer::take`/`restore` are public, so a shell reads into the buffer the frame stays in. `SimStream` is the
  model alone: `send`, `shutdown`, `poll_chunk(cx, room)`. `BufferPool` is now `Send + Sync` (`Arc<Mutex<_>>`, one
  uncontended lock per buffer taken and returned; a poisoned lock is recovered, no panic), because the tokio client's
  futures must be `Send` and the core holds received frames.
- `ultragraphdb_protocol_transport_compio` (new): `Connection` (the old one, now reading as the `Framer` asks and running
  the handshake machines; `write_frame` hands the frame's vector back; `shutdown`), `CompioSimStream`.
- Tests: the old suite's 33 tests are all kept. In the pure crate (38): errors 2, buffer pool 2, faults 1, stats 1,
  outbound 7, pooled buffer 2 (1 kept, rewritten on the framer, 1 new), received frame 1 (rewritten on the framer),
  simulated network 5 and simulated stream 2 (rewritten on the pure API), framer 7, client handshake 4, server
  handshake 3, answer 1 (16 new). In the compio crate (13 + 5 scenarios): connection 10, `ready` 1, the compio
  adapter 2 (the old simulated-stream tests through compio), and the 5 conversations. 38 + 13 = 51 = 33 kept + 2
  adapter copies + 16 new. The framing fuzz target drives the pure framer.
- These machines were extracted from code the 33 tests already covered; the 16 new tests were written against the
  extracted surface, not seen failing first. The client's audit below exercises the framer and the handshake through
  the core.

## 9b.2, 9b.3, 9b.9 The client core

`ultragraphdb_client_core` (renamed from `ultragraphdb_client` with `git mv`). Public surface: `ClientCore`,
`ClientConfig`, `Session`, `Transaction`, `Statement`, `Committed`, `Pending`, `Reply`, `ResultStream`, `BatchPoll`,
`ClientError(pub ClientErrorEnum)` (18 variants), and behind `test-utils` the sans-I/O `ScriptedServer` and
`ServerScript`.

- Sans-I/O: `connect(now)` queues Hello; `transmit()` gives frames to write; `receive(frame, now)` takes whole frames;
  `disconnected(error, now)`; `next_deadline()` / `handle_timeout(now)`; `needs_connection()` when a commit waits for a
  new connection. The core never reads a clock.
- Streams from 1 per connection, never reused; at most the Welcome's `max_streams`; a request past the Welcome's
  `max_frame` refused before it is sent.
- Replies (Register, Begin, Commit, Prepare, Describe) are decoded through their views when they arrive; result
  batches stay in their pooled buffers and are read in place by the caller's function given the `ResultView`
  (`next_batch(stream, now, |view| ...)`), so a batch is validated once and nothing is copied. Credit: a result stream
  starts with the Welcome's initial credit, spends one per Result frame, and when spent with nothing arrived the core
  grants `ClientConfig::credit` frames; `grant` grants more. `cancel` ends the stream.
- Commits: one in flight per session; the request number is the session's last committed + 1. Lost with its connection
  the commit is resent on the next connection with the same session and request number, on a new stream; `SessionBusy`
  is resent after `retry_delay`; at most `commit_attempts` tries (losses and busy answers counted), then
  `RetriesExhausted` and the session in doubt until `resolve`. A refusal of a commit or of its retry is its answer
  and keeps the request number.
- Errors are retryable exactly for the wire's retryable codes (`ClientError::retryable` = `Refused` with
  `ErrorCode::retryable`).

### Phase 3: the audit

Script in the session's scratchpad (`audit_client.py`), its own target directory, the source tree hashed before and
compared after (equal). 20 defects planted one at a time, each restored in a `finally`; none broken.

| Defect | Result |
|---|---|
| retry carries the next request number | rejected (lost-reply, busy, in-doubt tests) |
| retries unbounded | rejected (bounded retries) |
| session advances on a refusal | rejected (refused commit, fault workload) |
| busy never resent | rejected (busy session) |
| commits not resent after a reconnect | rejected (lost reply, busy) |
| duplicates not dropped | **survived** → `test_a_duplicated_result_frame_is_read_once`, re-planted, rejected |
| credit never granted | rejected |
| credit not spent | rejected |
| `FLAG_LAST` ignored | rejected |
| cancel sends nothing | rejected |
| read-only may write | rejected |
| stream limit off by one | rejected |
| `max_frame` unchecked | rejected |
| timeout exclusive | rejected |
| a commit reply's request unchecked | **survived** → `test_a_commit_reply_of_another_request_is_a_mismatch`, re-planted, rejected |
| in doubt not marked | rejected |
| an Error on stream 0 ignored | rejected |
| a refusal on a transaction's stream dropped | **survived** → `test_a_refused_write_is_the_answer_of_the_next_request`, re-planted, rejected |
| a handshake failure kept | rejected |
| a second commit while one is in flight | rejected |

Before implementing, four test expectations were corrected against the design rather than the code: the busy test
begins its second transaction before committing the first (the first commit's reply can arrive while the second
begins); the exhausted-retries test expects one Commit at the server, since the blackhole also loses the reconnects'
Welcomes; the fault workload waits for "ready or disconnected"; `StreamBusy` and `NotInDoubt` were added as variants.

The shells: 6 defects (no reconnect for a retry, frames not flushed before a wait, a received frame dropped; in each
shell), all rejected (`audit_shells.py`).

## 9b.10, 9b.11 The shells

- `ultragraphdb_client_tokio`: `Client<C: Connect>` with async `connect`, `register`, `resume_session`, `begin`,
  `write`, `operation`, `abort`, `read`, `execute`, `next_batch`, `cancel`, `commit`, `resolve`, `prepare`, `describe`,
  `close`; `TcpConnector` (Nagle off), `TokioSimStream`. Frames are read with `read_buf` into the pooled buffer's spare
  capacity, as the framer asks: no zeroing, no copy. The time given to the core is tokio's clock. Over TCP every future
  is `Send` (checked by a test). `tokio = "=1.54.1"` (it was `"1"`, resolving to 1.54.1 already), features
  `net`, `io-util`, `time`, and `rt` in tests; its tree is `mio`, `socket2`, `bytes`, `pin-project-lite`, `libc`
  (bindings only): pure Rust, no C toolchain.
- `ultragraphdb_client_compio`: the same surface over the transport's compio `Connection`; `TcpConnector`.
- A read cut by a deadline may stand inside a frame, so after a timeout a shell gives the connection up if the core
  still holds it (a busy commit's resend then goes on a new connection). Correct, at the cost of a reconnect per busy
  answer.

## Final counts, Cargo and Bazel equal

| Crate | Cargo | Bazel |
|---|---|---|
| `ultragraphdb_protocol_transport` | unit 38 | `:ultragraphdb_protocol_transport_tests` 38 |
| `ultragraphdb_protocol_transport_compio` | unit 13, `bdd` 5 scenarios | 13, `:bdd` 5 scenarios |
| `ultragraphdb_client_core` | unit 33, `bdd` 5 scenarios (10 steps) | 33, `:bdd` 5 scenarios |
| `ultragraphdb_client_tokio` | unit 4 | 4 |
| `ultragraphdb_client_compio` | unit 3 | 3 |

Doc tests: 0 each, every `doc_test` passes. Clippy (`--all-targets --all-features`) clean on the five crates.
`cargo build --workspace` passes.

## 9b.1 Scenarios

`ultragraphdb_client_core/features/wire-protocol/rust_client.feature`, verbatim: "The client retries a lost reply
through its session", and the client's side of "Credit stops a stream", "Cancel ends a stream", "A stale statement is
refused", "A repeated term is sent once per batch". Steps in `tests/bdd.rs`, driving the core against the scripted
server with the unit tests' harness, included by path; Bazel `:bdd`. 5 scenarios, 10 steps, under Cargo and Bazel.
Written after the core's implementation, from the audited unit tests' conversations, so not seen failing first. "Describe
answers from the data dictionary" needs the data dictionary (the wire's own BDD runs it); the client's Describe is
covered by `test_prepare_describe_and_a_stale_statement`.

## 9b.5 Fuzzing

`fuzz/ultragraphdb_client_core`, target `reply`: any bytes framed as a shell frames them and received by a ready client
waiting on stream 0, Register, Begin, Prepare, Describe, Commit and result streams on streams 1 to 20 (covering the
golden frames' streams); never a panic, a refused frame leaves the client disconnected. Seeds: the nine golden reply
frames in `ultragraphdb_client/ultragraphdb_client_core/testdata/fuzz_seeds/reply/`. Built with
`cargo +nightly fuzz build --fuzz-dir fuzz/ultragraphdb_client_core`; not run (11.7). The transport's `framing` target
builds too.

## 9b.4 The conformance suite against the client

`tests/types/client_core/client_core_conformance_tests.rs`, reading the wire crate's golden frames (Bazel: the wire's
new public filegroup `:golden_bodies`, a `data` of the core's unit target):

- every request the client sends is its golden frame, byte for byte with only the stream moved (header re-checksummed
  with xxhash-rust directly): hello, write, operation, operation_counter, operation_deep, abort, read, credit, cancel,
  prepare, describe, execute (12);
- every golden reply is read as its values: welcome, register, begin, prepare_reply, describe_reply, commit (version,
  request, results), result (columns, rows, cells column by column, unbound, resume, flagged last), error (Conflict,
  detail 3, retryable) and error_operation (OperationFailed, the value found);
- every single-bit flip of every byte of eight golden replies (2,000+ frames) is refused: by the framer for a header
  flip, otherwise by the core with exactly the `ProtocolError` the wire's `FrameView::validate` gives, never read as a
  value; every truncation of each is `Truncated` or `Closed` at the framer.

### Under the simulator's faults, and through both shells (2026-11-01)

The expected outcome of every case comes from the connection's rule (`PROTOCOL.md` §2.5 step 13: each frame numbered one
above the last is taken, the first that is not is refused with `SequenceOrder` and the connection given up) applied to
the arrivals the network itself recorded (its counters read around each send), and, for a damaged frame, from the
transport's framer and the wire's `FrameView::validate` run on the bytes: never from the client.

**The core** (`client_core_conformance_faults_tests.rs`, 4 tests). Each golden reply crosses a `SimNetwork` link, a probe
behind it (the same frame on stream 1,000, which nothing opened, so it is dropped once taken), and the client's framer
reads what arrives as a shell does, the bytes it asks for and no more. Five fault classes: drop 35 %, duplicate 35 %,
delay 60 % up to 4 steps, split 70 %, and the four together; eight seeds per golden frame and class (32 for the
Welcome).

- Replies: register, begin, prepare_reply, describe_reply, commit, error, error_operation, drop_reply, result. Taken at
  its place, a reply is read as its golden values at once; a refused one leaves a plain request and a read failed as
  `ConnectionLost`, the `SequenceOrder` rule its cause, a commit or a drop waiting for the next connection
  (`needs_connection`); both frames lost, the request and the read time out at the deadline (`TimedOut`; the commit
  waits again).
- The Welcome (welcome_no_auth), and RFC 7677's Authenticate exchange (welcome, authenticate_server_first,
  authenticate_server_final, each crossing in turn): the client answers with authenticate_client_first and
  authenticate_client_final byte for byte (user "user", password "pencil", nonce "rOprNGfwEbeRWgbNEkqO") and ends as
  principal 7 with the golden expiry. These and the Drop request (`test_the_drop_the_client_sends_is_its_golden_frame`)
  were not in the suite before; the Hello is checked at every connect.
- What happened, all classes summed per test (delivered / refused / lost): replies under drop 45 / 18 / 9, duplicate
  72 / 0 / 0 (36 copies refused after their frame), delay 72 / 0 / 0 (54 delayed), split 72 / 0 / 0 (90 splits), all
  four 54 / 9 / 9; the Welcome and the exchange alike (`--nocapture` prints them).

**The shells** (`ultragraphdb_client_tokio` and `ultragraphdb_client_compio`: `tests/types/client/client_conformance_tests.rs`,
4 tests each, and `tests/utils/golden.rs`, a golden server: each frame of the client's checked against the golden frame
its step names but for stream and sequence, answered with its step's frames moved to the request's stream and numbered,
a step's frames in one write; checksums by xxhash-rust directly).

- One conversation, over a plain network and a split and delayed one under three seeds: every request the shell writes
  is golden (hello, prepare, describe, read, credit, read, cancel, execute, write, operation, operation_counter,
  operation_deep, abort, drop: 14), every golden reply is read as its values (welcome_no_auth, register, prepare_reply,
  describe_reply, begin, commit, error, error_operation, drop_reply, result six times: four not flagged last spend the
  Welcome's credit of four and bring the golden Credit).
- The Authenticate exchange through the shell, as above.
- A reply the server sends twice is read once, its copy refused, so the next request fails as `ConnectionLost` (its
  cause `SequenceOrder { expected: 3, found: 2 }`) and the one after goes on a new connection; a reply never sent times its request out (`TimedOut`); three connections.
- Every single-bit flip of every byte and every truncation of the nine golden replies, served over a split and delayed
  network and the connection closed after (6,544 cases a shell): never a value, and exactly the answer the framer and
  the wire predict: a plain request `ConnectionLost`, its cause the framer's refusal (`Transport`; none for a close
  between frames) or the frame's rule (`Protocol`); a commit or a drop, allowed one attempt, `RetriesExhausted {
  session, request 1, attempts 1 }`; a read `Protocol(the rule)` of its own frame, or `ConnectionLost` with the
  framer's refusal. No damaged frame passed both.

Written after the implementation, so not seen failing first; its teeth, 17 defects planted one at a time in a copy of
the tree under the session's scratchpad (`plant.py`, each file restored in a `finally`, the copy's sources hashed
before and after, equal), the conformance suites run under Bazel:

| Defect | Result |
|---|---|
| D1 the core takes frames out of sequence (`check_order` skipped) | rejected (the three fault tests) |
| D2 the sequence rule tolerates a gap (transport) | rejected (the three fault tests) |
| D3 a frame of a stream never opened gives the connection up | rejected (the three fault tests) |
| D4 a lost connection leaves a plain request waiting | rejected (replies) |
| D5 a lost commit fails instead of waiting to be sent again | rejected (replies) |
| D6 the timeout is exclusive | rejected (the three fault tests) |
| D7 a read lost with its connection reads as done | rejected (replies) |
| S1 tokio: a cut frame reads as a close between frames | rejected (damaged replies) |
| S2 tokio: a framer refusal does not give the core's connection up | rejected (damaged replies) |
| S3 tokio: a Drop's reply is a mismatch | rejected (conversation) |
| S4 tokio: reads past what the framer asks | **survived** → the golden server writes a step's frames in one write; re-planted, rejected (conversation, repeated reply) |
| S5 tokio: cancel sends nothing | rejected (conversation) |
| S6 compio: a cut frame reads as a close between frames (transport's compio connection) | rejected (damaged replies) |
| S7 compio: a framer refusal does not give the core's connection up | rejected (damaged replies) |
| S8 compio: a Drop's reply is a mismatch | rejected (conversation) |
| S9 compio: cancel sends nothing | rejected (conversation) |
| S10 compio: reads past what the framer asks | rejected (conversation, repeated reply) |

S4 survived because the simulator delivers each write as its own message and splits never merge two, so a read never
meets a second frame's bytes; TCP coalesces writes. The golden server now sends a step's frames as one write.

Outside the tests: `drop_command` added to both shells (the core's Drop had no shell method, so the golden Drop could
not cross a shell); the shells' dev-dependencies gain `ultragraphdb_error_protocol`, `ultragraphdb_tool_bdd` and
`xxhash-rust` (their `Cargo.lock` entries edited to match), their unit targets the wire's `:golden_bodies` as `data`;
the conformance file's helpers (`golden`, `restream`, `numbered`, `stream_of`, `view`) are `pub` for the new file.

Bazel: core unit 82 (+4), `:bdd` 13 scenarios; tokio 9 (+4); compio 8 (+4); all pass. The shells' damaged-reply tests
take about 13 to 30 s each on a loaded machine.

Open questions, as raised (1 and 2 resolved below; 3 moved to the backlog by the coordinator):

1. Through a shell, a frame the core refuses by a rule reaches the caller as `ConnectionLost`; the rule stays in the
   shell's private `failure`. The framer's refusals do reach the caller (`Transport`). Should a request's answer carry
   the rule?
2. A read whose connection is given up answers `StreamFinished`, as `next_batch` documents, not the connection's
   failure, so the shell's caller cannot tell a lost read from a finished transaction's. Intended?
3. The shells have no `grant`: Credit goes out only when a stream's credit is spent. Should they wrap `ClientCore::grant`?

### Questions 1 and 2 resolved (2026-11-01, approved by the user)

- `ClientErrorEnum::ConnectionLost { cause: Option<Box<ClientError>> }`, constructor `ClientError::ConnectionLost(cause)`:
  the cause is what gave the connection up, the framer's refusal or a frame cut short or an I/O failure (`Transport`,
  from `disconnected`), the rule a frame broke (`Protocol`), the server's closing Error (`Closed`), the handshake's
  failure (`Transport`); `None` for a plain close (`TransportError::Closed`) and for `close()`. It is the error's
  `source()` and follows its text (`the request's connection was lost: <cause>`). The core builds it where it gives the
  connection up, so both shells' callers get it with no shell change. Requests waiting on a lost connection now answer
  `ConnectionLost` where they answered `Transport(error)` before; `closes_connection` is true for `ConnectionLost`. A
  timeout still answers `TimedOut`.
- A read cut short by a lost connection, its last frame not arrived, answers what the connection's requests answered
  (`ConnectionLost` with its cause, or `TimedOut`), once, even after the next connection opened; it is not retried
  (`PROTOCOL.md` §11.4). A read whose last frame had arrived is kept and read to its end, then `Done`. `StreamFinished`
  stays for a read whose stream or transaction ended, or whose loss was answered; `cancel` forgets a lost or kept read.
  `next_batch`'s doc says so.
- Tests, each seen failing first (run of 2026-11-01: core 14 failed of 94, tokio 3 of 15, compio 3 of 12, each a wrong
  value: `Transport` or `StreamFinished` or a cause missing): `client_core_lost_tests.rs` (11: a plain loss, a framer
  refusal, a truncation, a rule, the server's closing Error, each the cause of a waiting request's and a cut-short
  read's loss; the loss answered after the next connection; a timeout; a read whose last frame arrived read to its end;
  an ended and a cancelled read `StreamFinished`; the cause as `source()`), the error tests (Display pinned, `source()`,
  the constructor, `closes_connection`), and in each shell `test_a_read_cut_short_through_the_shell_fails_as_lost_with_its_cause`
  (two batches then the server closes: `ConnectionLost(None)`; the second batch's header refused: `ConnectionLost(Transport(the
  refusal))`; its body broken: `Protocol(the rule)` for the read itself; a Register's reply broken: `ConnectionLost(Protocol(the
  rule))`) and `test_a_read_whose_transaction_ended_still_answers_stream_finished`; the conformance suites' expectations
  follow.
- Teeth, 8 defects planted in the scratch copy (`plant2.py`), all rejected: the rule dropped as the cause; the transport
  error dropped; a cut-short read reported finished; a read whose last frame arrived dropped; the loss keyed by the
  current connection; the text dropping the cause; the cause not the source; `ConnectionLost` not closing.
- Bazel: core unit 94, `:bdd` 13 scenarios; tokio 15; compio 12 (counts include agent M2's tests); all pass. No target
  outside `ultragraphdb_client` depends on the client core.

## Decisions taken here, for review

- **A transaction is its stream.** Begin, Read, Execute, Write, Operation, Commit and Abort of a transaction go on its
  stream, since no body names a transaction otherwise. So `Cancel` of a transaction's result ends the transaction, and
  a refusal (an Error frame, flagged last) ends it too.
- **Write, Operation and Abort have no reply**, as group 9's notes list replies only for Register, Begin, Commit,
  Prepare and Describe. A refusal of one arrives as an Error on the transaction's stream and is the answer of its next
  request.
- **Register goes on a stream of its own**, not stream 0: a request takes a stream id, and `Outbound` refuses stream 0.
- **A retried commit is a Commit frame on a new stream with no transaction.** The server answers it from its session
  record if it was applied; the scripted server answers `Refused` (detail: the request) when it was not and there is no
  transaction, `SessionBusy` while the original is still in the commit step. The client reads any refusal of a retry as
  "not applied" and keeps the request number.
- **A frame identical to the last of its stream is a duplicate** and is dropped (the simulator duplicates; TCP never
  does). This presumes a server never sends two identical frames in a row on a stream; consecutive Result frames differ
  by their resume key.
- **Request numbers**: the session's next request is its last committed + 1, so an aborted or refused commit's number
  is used again by the next transaction.
- **`BufferPool` is `Send + Sync`**, for tokio.
- **The scripted server lives in `src/utils_test/` behind `test-utils`** so the core and both shells test against one
  script; Bazel turns the feature on for every consumer, as AGENTS.md says of features.

## Open questions for the user

1. A retried commit whose original never reached the commit step: which code does the server answer with? The
   scripted server uses `Refused`; a code of its own ("not committed, transaction gone") would let the client tell it
   from a catalog refusal.
2. Should a transaction's reads take streams of their own, so cancelling a read does not end the transaction? That
   needs a transaction field in Read and Execute: a wire change.
3. The busy-commit resend reconnects in the shells (a cut read cannot be resumed): acceptable, or should the shells
   keep a frame in progress across a deadline (cancel-safe reads in tokio; not possible for compio's owned buffers)?

## For group 10

- Answer a retried Commit (same session and request, new stream, no transaction) from the session table; answer
  `SessionBusy` while that commit is in the commit step.
- Keep a transaction's stream open across its requests: `Outbound` closes a stream after its `FLAG_LAST` frame, so a
  Result stream's last frame or a reply flagged last would end the transaction's stream in the server's scheduler.
- Error frames end their stream (flag last), as the golden Error frames do.
- The client sends Credit only when its credit is spent and nothing has arrived; the server's initial credit per
  stream is the Welcome's.
