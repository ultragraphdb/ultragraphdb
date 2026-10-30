# Group 9c: the protocol block

The specification's open points, and the decisions that resolved them, kept here as `PROTOCOL.md` §13 held them before
task 9c.4 wrote every resolution into the specification. The point numbers are the ones every other document cites.
The security design they led to is `openspec/notes/security.md`, and its architecture `ultragraphdb_protocol/SECURITY.md`.

## The open points of `PROTOCOL.md` §13, as triaged and decided

### 13.1 Left to the server, which does not exist yet

`ultragraphdb_server_session` and `ultragraphdb_server` hold no code. The connection rules of section 11 come from
the transport crate's machines (`ServerHandshake`, `Outbound`), the Rust client core and the design; the server's
answers below have no code to read.

1. Which frames besides the last Result carry `LAST`? The `Outbound` scheduler closes a stream after any frame flagged
   `LAST`, the golden Error frames carry it, and the golden Commit, Begin and other replies do not. Does every reply that
   ends a stream (Register, Commit, Prepare, Describe, Error) carry `LAST`?
2. What does the server answer to a request on stream 0, or on a stream already open (`StreamZero`, `StreamOpen` of
   the transport)? An Error of which code, on which stream, and does the connection stay open?
3. Does the server refuse a frame larger than its Welcome's `max_frame`, and with which code? The framer refuses only a
   frame past its receive buffer's capacity.
4. In which order does the server list a namespace's declarations in a Describe answer? The schema id hashes them in
   that order, so a client recomputing it depends on it.
5. Which RDF syntax rules does the server apply to a request's terms (IRI syntax, language-tag syntax, a literal's
   lexical form against its datatype, an IRI or blank node at the predicate slot)? The wire's views check only the
   layout of section 4.4.
6. Which N-Triples serialization does an OperationFailed Error's value use: escapes, `xsd:string` stated or not,
   directional language tags?
7. Does the server's Hello answer close the connection after code 16 (ProtocolVersion)? `ServerHandshake` returns the
   refusal and the Error frame; the closing is the shell's.
8. How long may a transaction live (code 4's limit), how many sessions and statements a server holds, and what
   `initial_credit` and `max_streams` it offers? These are configuration; no default is in the code.
9. What does the server send for a Result with `columns` 0, and for a read that matches nothing: one empty batch flagged
   `LAST`?

### 13.2 Left to a Rust crate's internals

10. A result term with tag `0x01` names an IRI or a blank node by a UID, and its `text` is the IRI's text or the
    label; nothing in the batch tells a client which. How does a client tell an IRI from a blank node?
11. A by-value date or time with timezone offset −1 minute (`-00:01`) encodes `Z` as `0xFFFF`, the marker for "no
    timezone": oxsdatatypes 0.2.4's own layout, which this protocol adopts. Is the collision accepted, or does the
    protocol take its own marker?
12. The by-value lexical forms of section 10.5 are oxsdatatypes 0.2.4's `Display` output, which the encoder compares
    against. A future oxsdatatypes that formats differently changes which literals are encoded by value. Does the
    protocol pin these forms as its own?
13. The Result view accepts a term encoding whose inline text is not UTF-8 or whose known-datatype code (tags `0x30`,
    `0x31`) is not in section 10.4; `ultragraphdb_core_terms` refuses such a term only when a reader opens it. Should
    the batch's check refuse it?
14. Describe's preorder number in an entity type's `extra`: preorder of which tree, counted from 0 or 1?
15. The write-set record's key and value contents, the posting format of list deltas and the commit record are the
    store's (`ultragraphdb_store_*`); the log alone reads them, and this file states their framing only.
16. The Round frame's header fields other than the command header (`stream`, `flags`, `release`, `protocol`) are the
    log writer's; the golden Round has them 0, `release` `0x00050000` and `protocol` 1.

### 13.3 Ambiguous in the code

17. No receiver checks the header's `protocol`, `view`, `replica` or `flags` bits 1 to 7. Should a server refuse a
    frame whose `protocol` is not the Welcome's?
18. `release` is an opaque u32 everywhere; the golden frames use `0x00050000`. Does it have a structure (major, minor)?
19. The Error frame's `stream` of a protocol error: `ErrorReply` documents stream 0 for the connection's last frame, and
    the transport machines send handshake refusals on stream 0. Does a protocol error found in a body of stream n go on
    stream 0 or n?
20. The receivers enforce one encoding for the request term table's text order but not its entry order, and not for
    Write's names, Describe's strings or a Result batch's term and string order (section 4.7). Is the sender's
    convention part of the protocol, so a receiver may refuse other orders?
21. A Commit request may carry results, and a Prepare request result types; the views accept them. Should a server
    refuse them?
22. A list delta (write-set section 0) may hold a removal (`value_len` `ABSENT`); the view accepts it and the owned
    record reads it as an empty delta.

### 13.4 Documents and code disagree

23. `ultragraphdb_protocol_wire/src/lib.rs` says "Terms a client sends are RDF terms in N-Triples text". The code
    sends them in the term table of section 4.4, by text without N-Triples syntax; only the Error's value is N-Triples.
24. The design (`zero-copy-binary-protocol.md`, "Multiplexing") lists the Welcome's limits as "in-flight streams per
    connection, frames per stream"; the Welcome carries `max_frame` in bytes, `max_streams` and `initial_credit`.
25. The design's validation order reads "command known, reserved bytes zero, `size` within the maximum"; the code also
    checks the cluster between the reserved bytes and `size`, as the wire-protocol spec states, and `size ≥ 256` and
    the received length before the chunks.
26. The design's Round body lists "position, parent, first version, decisions, the admitted write-set records"; the
    code adds `incarnation` and `transactions` to the command header.
27. The design says a client reads a result cell "as `core_terms`' existing `TermView<'a>`"; a native client has no
    `TermView`, and section 10 states the bytes instead.

### 13.5 Commands not yet in the protocol

28. No command drops anything: a predicate, a graph, a namespace's data, a namespace or every namespace. The node's
    drop of a predicate is a commit step proposal whose record carries operation 1 (section 12.2), reached today only
    from code; the other four drops have no proposal and no operation code yet. Which drops does the protocol offer,
    as which command, answered how, and which operation code does each take in the write-set record?

### 13.6 Carried from the stage's notes

29. A retried Commit (same session and request, a new stream, no transaction) whose original never reached the commit
    step: which code answers it? The scripted server answers 7 (Refused), which a client cannot tell from a catalog
    refusal.
30. Reads share their transaction's stream, and section 11.2 contradicts itself there: a stream ends when its last
    frame (`LAST`) arrives, yet a transaction's stream stays open across its Reads, each ending with a `LAST` Result;
    `Outbound` closes a stream after any `LAST` frame. Credit is per stream, and nothing says whether a second Read on
    the same stream starts again at `initial_credit`. Cancelling a Read ends its transaction. Should a transaction's
    Reads and Executes take streams of their own, naming their transaction (a wire change to Read and Execute)?
31. A busy commit's resend reconnects in the Rust shells, since a cut read cannot be resumed. Acceptable, or should the
    shells keep a frame in progress across a deadline (cancel-safe in tokio, not possible for compio's owned buffers)?
32. Every frame is to carry a unique nonce (approved 2026-10-28), but section 2.1 reserves the 16 bytes as zero, and
    nothing states how a nonce is chosen, checked, or carried by the log's Round frames.
33. A counter add's result carries no value: `uid` and `created` are 0. A client cannot read the counter's new value
    without a second transaction.
34. The superblock keeps a checkpoint's position and checksum, not its round's version, so recovery cannot tell a store
    that fell behind its checkpoint from one the checkpoint covers (section 12.2).

### 13.7 Found by a critical reading

The reading asked of each section its purpose, its concepts, its assumptions and its consequences, against clarity,
accuracy, precision, consistency, completeness and significance.

35. Concepts: a statement is "prepared per session" (section 11.7), yet Prepare and Execute carry no session, and a
    session is a commit's exactly-once record, not a connection. Whose cache holds a statement: the connection's?
36. Consistency: the error table marks 14 (SessionBusy) not retryable, while section 11.5 has the client retry it
    after a delay.
37. Precision: may a Commit carry session 0, and what does it promise then? The commit step treats session 0 as no
    session; section 6.3 says every commit goes through a session.
38. Consequences: a retried Commit refused with 12 (SessionEvicted) leaves its outcome unknown. Nothing tells a client
    what to conclude.
39. Consequences: Register is not idempotent. A lost reply leaves a session nobody uses, which can evict a live one from
    a full table.
40. Consistency: Read and Write state a namespace, and so does the Begin of their transaction, whose write-set record
    holds one namespace. A request naming another is unspecified.
41. Completeness: a client cannot name a stored blank node again. A request's blank-node label is scoped to the
    request, and a result names a blank node by a UID no request can carry, so a blank node read cannot be read from,
    written about or deleted by its identity (with point 10).
42. Completeness: Write deletes a statement and sets or deletes its properties by statement ID (change records 4, 6
    and 7 without bit 0), but no read returns a statement's ID.
43. Assumptions: a transaction's write set must fit one Round frame, so a transaction writes at most about 1 MiB, and a
    client learns the limit only from a refused commit (6, TooLarge). Is the ceiling intended, and how does a bulk load
    proceed?
44. Assumptions: only the server states a largest frame. A client cannot bound the Result frames it receives, and the
    server's `max_frame` floor of 4,096 bytes is below one vector term of the largest dimension (262,143 bytes).
45. Consequences: every refusal of a body's rules is code 1 (Protocol), which closes the connection, so one malformed
    request ends every transaction multiplexed on it (with point 19).
46. Precision: a Cancel of a Commit's stream, or a connection lost while a commit is in the commit step: does the commit
    still happen, and what becomes of a connection's open transactions?
47. Precision: a resume key is "bytes the server gave". Is it valid in another transaction or request, what does the
    server do with one a client altered, and in which order are rows returned?
48. Completeness: Execute checks the parameters' count, not that a literal is not bound at a slot declared a node (column
    type 1).
49. Accuracy: section 10.2 encodes a by-value datatype by value whenever its lexical form is canonical, but a canonical
    decimal with more than 18 fraction digits, an integer beyond i64, or a duration beyond its payload has no by-value
    payload.
50. Consistency: a boolean payload is read as true for any nonzero byte (section 10.5), against section 1's booleans and
    the one-encoding rule.
51. Consistency: the protocol has no escape grammar (section 1), yet an OperationFailed value is N-Triples text (point 6),
    which a native client must parse.
52. Precision: `columns × rows` is a product of two u32s; a receiver computing it in 32 bits overflows.
53. Significance: the protocol authenticates nobody and encrypts nothing. `cluster` is configuration, not a secret, and
    XXH3 detects accidents, not tampering. A session is a commit version, so any connection may commit through, or read
    the stored reply of, any session it guesses.
54. Clarity: three byte orders meet in one Result (little-endian frames, big-endian UIDs and by-value payloads,
    little-endian vectors), and section 1 does not say why.
55. Completeness: no rule closes an idle connection or detects a half-open one, which holds its transactions until their
    time limit.

### 13.8 Triage (2026-10-30)

**A — an obvious or small fix, with the recommendation: approved 2026-10-30, every row as written.**

| Point | Recommendation |
|---|---|
| 1 | `LAST` marks the end of a request's reply; the replies that end their stream (Register, Commit, Prepare, Describe, Error) carry it. Settled with point 30 |
| 2 | A request on stream 0, or a new request on an open stream, is a connection error: Error 1 on stream 0, then close |
| 3 | A frame over the Welcome's `max_frame` is refused as `FrameTooLarge`: Error 1 on stream 0, then close |
| 4 | Declarations in catalog key order: kind (8, 9, 10), then UID ascending; the schema id hashes that order |
| 5 | IRIs absolute per RFC 3987; language tags well-formed per BCP 47; an IRI at the predicate slot; the known datatypes' lexical forms checked; a refusal is 7 (Refused) naming the term |
| 6 | Superseded by point 51 |
| 7 | The server closes the connection after 16 (ProtocolVersion) |
| 8 | Defaults: read-write transaction limit 10 s, as decided for bounded transactions; 4,096 sessions, 256 statements per connection, `max_streams` 256, `initial_credit` 4 |
| 9 | One batch flagged `LAST`, `rows` 0, `columns` the unbound slots |
| 11 | A date or time whose offset is −00:01 is not encoded by value; it takes tag `0x30` |
| 12 | Section 10.5's forms are the protocol's own, normative; golden vectors pin them against an oxsdatatypes upgrade |
| 13 | The batch check refuses a non-UTF-8 inline text and an unknown datatype code as `BadTerm` |
| 14 | State the code's rule: preorder of the type hierarchy, roots and children in record order, from 0 |
| 15 | No change: server-internal, versioned by the mapping contract |
| 16 | A Round frame's header: `stream` 0, `flags` 0, `protocol` 1, `release` the writer's, nonce per point 32 |
| 17 | The server refuses a `protocol` other than the Welcome's, a nonzero `view` or `replica` from a client, and flag bits 1 to 7 |
| 18 | `release` stays opaque: logged, never compared |
| 19, 45 | A frame-level refusal (section 2.5 steps 1 to 11) is a connection error: Error 1 on stream 0, then close. A body-level refusal (steps 12, 13) is a stream error: Error 1 on the request's stream, and the connection stays |
| 20 | The orders stay the sender's convention; a receiver refuses only what section 4.7 enforces |
| 21 | Refuse results in a Commit request and result types in a Prepare request (`FieldValue`) |
| 22 | The record view refuses a list delta's removal (`FieldValue` `"list delta"`) |
| 23–27 | Update the design documents to the code |
| 29 | A code of its own, 18 NotCommitted, retryable: the request is not in the table and no transaction is open, so the client runs its transaction again |
| 31 | Accept the reconnect |
| 32 | Per connection and direction, a u128 counter from 1; a receiver refuses a nonce not one above the previous one (`NonceOrder`); this replaces section 11.2's duplicate check by checksum. A Round frame's nonce is its position |
| 33 | Kind 4's result carries the counter's new value in `uid`, as an i64 in two's complement; the commit step computes it in commit order |
| 34 | A checkpoint records the last commit version it covers, passed to `trim_through` and kept in the superblock; recovery refuses a store below it, naming both |
| 35 | Statements belong to the connection; section 11.7 says so |
| 36 | 14 (SessionBusy) is retryable, after a delay |
| 37 | Session 0 commits without exactly-once: a lost reply leaves the outcome unknown |
| 38 | The client reports the outcome as unknown and does not retry |
| 39 | Accept: an orphan session ages out of the table; a client registers again |
| 40 | Read and Write drop their `namespace`; their transaction's Begin names it |
| 44 | Hello carries the client's `max_frame`, and both floors rise to 524,288 bytes, above the largest single term |
| 46 | A Cancel or a lost connection never undoes a decided commit; a connection's open transactions abort when it closes; the client learns a commit's outcome by retrying it through its session |
| 47 | A resume key is valid in its own transaction and pattern only; the server refuses one outside the pattern's keys (Error 1 on the stream); rows come in the store's key order |
| 48 | Execute refuses a literal at a node slot: 11 (Parameters), `detail` the statement |
| 49 | By value only when the value also fits the payload exactly; otherwise tag `0x30` |
| 50 | A boolean payload of 2 or more is refused as `BadTerm` |
| 51 | The value found is a one-entry request term table (section 4.4), not N-Triples |
| 52 | Compute in 64 bits; a count past the cells' section is `CountMismatch` |
| 54 | One sentence: frames are little-endian; a term is the store's sortable key encoding, sent as stored |
| 55 | TCP keepalive, and a configured idle limit after which the server closes the connection; no Ping command |

**B — complex, or a major rework: reviewed 2026-10-30.**

| Point | Why | Decision |
|---|---|---|
| 10, 41 | Node identity: telling an IRI from a blank node, and naming a stored blank node again | Decided: a result-only tag `0x02` for a blank node, set from the dictionary's skolem flag, the store keeping `0x01` for both; its `text` its skolem IRI (`…/.well-known/genid/{uid}`, RDF 1.1 §3.5, which the store already mints), which a request names as an IRI (kind 1); a request IRI under the genid base the dictionary does not hold as a skolem is refused; UIDs stay internal |
| 28 | Drops as commands | Decided: one Drop command whose kind names the target, answered as a Commit; each drop a commit step proposal with an operation code of its own (4 to 7); graph and namespace drops carry their key ranges as the predicate drop does |
| 30 | Streams of a transaction | Decided: Reads and Executes take streams of their own and name their transaction |
| 42 | Whether a statement ID is a public identity | Decided: public, the property-graph edge identity; a read returns it |
| 43 | The size of a transaction | Decided: transactions stay bounded, by one Round frame and the 10-second limit, the bound stated in Welcome; a bulk loader writing outside the commit step takes a design of its own |
| 53 | Security | Decided: authentication, authorization and encryption are designed now, RBAC supported from the ground up though added later; TLS and other advanced encryption are implemented later |

## 9c.6 The wire and the term encoding

### Phase 1: the surface

`ultragraphdb_protocol_wire`, every new or changed behaviour `unimplemented!()`:

- The header's `sequence` (`Header::with_sequence`, `HeaderView::sequence`, replacing `nonce_reserved`);
  `HeaderView::verify(cluster, max_frame)` for a server's or the log's frame and `verify_client` for a client's;
  `FrameView::validate` and `validate_client` likewise; `MAX_FRAME_FLOOR` and the defaults of section 11.9.
- Commands 18 Authenticate and 19 Drop: `Authenticate`, `AuthenticateView`, `DropCommand` (named for its command as
  `ErrorReply` is, beside the standard `Drop` trait), `DropCommandView`, `DropKind`.
- Hello's `max_frame`; Welcome's mechanisms (`auth` follows from them), `max_write_set` and `transaction_ms`, built from
  the defaults with `with_*` setters.
- Read's and Execute's `transaction` and statement-ID flag; Write without its namespace.
- `CommitView::check_request`, `PrepareView::check_request` and `check_reply`, the rules of point 21 a view of a "both"
  command cannot hold alone.
- Result section 7, the statement IDs (`ResultBatch::with_statement_ids`, `push_row_with_id`,
  `ResultView::statement_id`); tag `0x02` (`ResultTerm::blank_node`, `is_blank_node`).
- Error codes 18 to 22 and `Action`, PermissionDenied's detail; the OperationFailed value as a one-entry term table
  (`ErrorReply::with_value(RequestTerm)`).
- The write-set record's `principal`; record operations 4 to 7 and `RecordOperation::is_drop`; the counter's value
  (`OperationResult::counter`, `counter_value`); `StatementError::NotANode` (point 48).

`ultragraphdb_core_terms`: `EncodedTerm::check`, the whole-encoding rules for bytes from a peer (points 13, 50).

Found in the work and written into `PROTOCOL.md` first: section 4.1's fixed part of the write-set record is 40 bytes,
not 32; section 12.2 refuses an argument on operations 5 to 7 (`FieldValue` `"argument"`); section 6.5 refuses a
drop's target that is not an IRI (`FieldValue` `"drop term"`). The 9c.5 scenario "A decimal the payload cannot hold is
kept by its lexical form" named tag `0x30`; its lexical form is 21 bytes, so the tag is `0x31`, and the spec says so.

### Phase 2: the failing run

The golden frames regenerated by both independent generators: `generate_frames.c` (the sequence in every header, the
Read frame's flags `LAST`, the only flag), `generate_bodies.py` (every changed body; new frames `welcome_no_auth`, the
four messages of RFC 7677's SCRAM-SHA-256 exchange, `drop` and `drop_reply`; a blank node and statement IDs in
`result`; `principal` 7 in the Round's first record). The regenerated `hello.frame` reproduces section 2.4's worked
example, checksum `0x13CBA202760D70B3`, and the schema id is section 11.7's `0xFE70341D112B65A9`.
`ultragraphdb_core_terms/testdata/by_value/` holds 114 by-value vectors computed from section 10.5 alone by
`generate_by_value.py` (point 12).

2026-10-30 13:47, `cargo test --no-fail-fast` against the surface:

| Crate | Unit tests | Scenarios |
|---|---|---|
| `ultragraphdb_protocol_wire` | 254: 71 passed, 183 failed | 23: 1 passed, 22 failed |
| `ultragraphdb_core_terms` | 26: 22 passed, 4 failed | 6 passed |

Every wire failure was an `unimplemented!()` panic but one, a test's own arithmetic on the record's old size, fixed.
The four core_terms failures were the stubbed check (2), the −00:01 vectors and test (point 11), and one more: the
vector `"--02-29Z"^^xsd:gMonthDay`. oxsdatatypes 0.2.4 refuses February 29 of a gMonthDay ("29 is not a valid day of
2"), which XML Schema 1.1's day-of-month constraint takes; the encoder holds it by value as February 28's payload and
one day, which reads back as `--02-29`. Group 10's term rules (point 5) must not take oxsdatatypes' refusal of it.

### Phase 3: the audit

Script in the session's scratchpad (`audit_wire.py`), its own target directory, the two crates' source trees hashed
before and compared after (equal). 45 defects planted one at a time in the implementation, each restored in a
`finally`; every one rejected by the suite, none survived:

| Area | Defects |
|---|---|
| Header | the sequence not encoded; the flags not checked; flag bit 1 allowed; a client's view, a client's replica free; the announced maximum ignored; a maximum above 1 MiB taken; the reserved bytes from 67 |
| Hello, Welcome | Hello's floor unchecked; Hello's `max_frame` unwritten; Welcome `auth` 2 taken; mechanisms not checked against `auth`; the limits swapped in the builder; `max_write_set` read at 24 |
| Authenticate | a principal outside a final message; a final message at an even step; a mechanism after step 0; the final flag unwritten |
| Drop | every namespace naming one; a target of any kind; the terms uncounted |
| Requests | Read's flags unchecked; Execute's transaction written as its statement; Write's namespace bytes free; a Commit request with results; a Prepare answer without types; a literal at a node slot |
| Errors, results | NotCommitted not retryable; an Error value of two entries; statement IDs uncounted; a blank node under the IRI's tag; a blank node unseen; the lenient term check; a batch keyed by the store's encoding; the counter's value read unsigned; command 19 read as 18 |
| Write-set record | a list delta's removal held; the principal unwritten; an argument on operation 5; a graph drop not a drop |
| Term encoding | a boolean of 2 whole; a known datatype's code unchecked; a language tag's text unchecked; −00:01 by value; February 29 kept by its lexical form |

The throwaway implementation that passed the audited suite is kept as the implementation (phase 4 of these two
crates), as the protocol allows. After it: `ultragraphdb_protocol_wire` 254 unit tests and 23 scenarios,
`ultragraphdb_core_terms` 27 unit tests and 6 scenarios, all passing under Cargo.

## 9c.6 The transport and SCRAM-SHA-256

### Phase 1: the surface

- Wire: `Header::stamp_sequence`, a frame numbered as it leaves, after it was built.
- `ultragraphdb_protocol_transport`: `Framer::new(cluster, max_frame)` for a server's frames and `Framer::from_client`
  for a client's, `set_protocol`; `ReceivedFrame::view` checks the sequence and the protocol after the chunks (§2.5
  steps 13, 14); `Outbound` sends every frame of the connection and numbers it as it leaves (`push_connection` for
  stream 0 and the refusals of streams never opened, `sent`), and ties reads to their transaction (`open_transaction`,
  `open_read`, `is_transaction`), a transaction's end ending them. New: the SASL traits `ClientMechanism`,
  `ServerMechanism` and `Authenticator`; `Credential`, `MechanismStep`; `ClientAuthentication` with
  `AuthenticationStep`; `ServerConnection`, a server's connection as one sans-I/O machine (the handshake, the exchange
  and re-authentication, `AUTHENTICATION_TIMEOUT`, the credential's expiry, `IDLE_LIMIT`, §11.2's stream rules),
  handing requests to the session as `Inbound`; `TransportError::Authentication`.
- `ultragraphdb_protocol_scram` (new): `ScramVerifier`, `ScramClient` (the transport's `ClientMechanism`),
  `ScramServer`, `ScramExchange` and `ScramAuthenticator` (its `Authenticator`) over a `Credentials` store,
  `ScramError`. Dependencies, pure Rust, pinned in the workspace: `sha2` 0.11, `hmac` 0.13, `pbkdf2` 0.13 (its `hmac`
  feature), `subtle` 2.6, `base64` 0.22, `stringprep` 0.1. The lockfile resolved with `cargo metadata`, nothing built.
- `ultragraphdb_protocol_transport_compio`: `Connection::new(stream, framer)`, the caller's side chosen by its framer;
  `framer_mut`.

A client learns from the final Authenticate its principal and expiry alone, so `AuthenticationStep::Done` carries those
two, while a server's `Credential` adds how long it lasts, which the authenticator, reading the wall clock, gives. An
idle connection closes with no frame (§11.4 names none): `Inbound::Closing`'s code is optional.

### Phase 2: the failing run

The suites written against the surface: the transport's tests of numbering, the connection's queue, reads and their
transaction, the framer's limit and client rules, the sequence and protocol checks, the client's exchange, and the
server connection (21 tests over a two-round test mechanism); SCRAM's tests over RFC 7677's exchange, its stored and
server keys computed by Python's `hashlib` and `hmac`, which reproduce the RFC's proof and signature; the
conversations rebuilt on `ServerConnection` with 13 of 9c.5's scenarios. "An expired credential closes the
connection" ends with the transaction aborting and the principal's sessions serving its next connection, the server
session's work: it is bound with 10.1, against the node.

2026-10-30 15:50, `bazel test //ultragraphdb_protocol/...` against the surface:

| Target | Result |
|---|---|
| `ultragraphdb_protocol_wire` unit | 255: 254 passed, 1 failed |
| `ultragraphdb_protocol_wire` bdd | 23 passed |
| `ultragraphdb_protocol_transport` unit | 73: 25 passed, 48 failed |
| `ultragraphdb_protocol_scram` unit | 20: 2 passed, 18 failed |
| `ultragraphdb_protocol_transport_compio` unit | 13: 3 passed, 10 failed |
| `ultragraphdb_protocol_transport_compio` bdd | 18 scenarios failed |

Every failure an `unimplemented!()` panic of the surface; the passing ones are data (errors, constants, credentials)
and the tests the surface leaves unchanged.

### Phase 3: the audit

Script in the session's scratchpad (`audit_transport.py`), each defect planted in place, run by `bazel test` over the
transport's, the compio shell's and SCRAM's targets (test failures, exit 3, are rejections; a build failure is
reported apart), restored in a `finally`, the two crates' source trees hashed before and compared after (equal). 45
defects:

| Area | Defects |
|---|---|
| Framer, frame checks | a client's rules unchecked; the announced maximum ignored; the expected sequence not advanced; the sequence unchecked; the protocol unchecked; Hello and Welcome not exempt |
| Outbound | the connection's frames sent last; frames unnumbered; numbering from 0; reads outliving their transaction; a read of no transaction; a read without its credit |
| Client exchange | the step unchecked; answering at the wrong step; the server's final unchecked |
| Server connection | requests before authentication; renewal as another principal; an unknown mechanism taken; the expiry, the idle limit, the authentication timeout ignored; stream 0 taking requests; a new request on an open stream; the stream limit closing; a read opened plain; a Write without its transaction; an Abort keeping its stream; a transaction's cancel unnamed; a refusal not last; a closing connection reading on; a refusal saying why; a final message unflagged |
| SCRAM | the password unprepared; a verifier below the floor; the server's nonce, its signature, its count below the floor unchecked; the name unescaped; channel binding and an authorization identity taken; the proof unchecked; the final nonce unchecked; one salt for every unknown name; an unknown name authenticated; any mechanism taken |

The first run rejected 41. One plant did not build and was corrected; three survived, and the suite was strengthened
for two:

- A new request on an open stream: the helper that opens a stream closes on `StreamOpen` too, so the explicit rule
  differs only for a request whose body breaks a rule, which it must close before reading (§2.6's order). Added: a
  malformed Read on an open stream closes the connection.
- A refusal saying why: both failures the suite compared failed alike. Added: a wrong password and a message out of
  step give the same frame.
- Stream 0 taking requests, the corrected plant, survived next: Begin on stream 0 is refused by the open helper as
  well. Added: Credit, Cancel and Write on stream 0 each close the connection.

Re-run, all three rejected. One survivor stands, equivalent: an unknown name authenticated needs a proof verifying
against the stand-in verifier, whose stored key is an HMAC of the server's secret, so a client would need a SHA-256
preimage of it. The check stays as defence in depth.

The implementation that passed the audited suite is kept (phase 4 of this slice).

After it, `bazel test`: the wire 255 unit tests and 23 scenarios, the transport 76, SCRAM 20, the compio shell 13 and
18 scenarios; every non-integration target of the repository, 118, passing on macOS. The compio, transport and SCRAM
targets, with the compio client's, passing on Linux 7.2 (arm64) in an OrbStack machine with io_uring enabled, where
compio runs its io_uring driver, 9 of 9.
