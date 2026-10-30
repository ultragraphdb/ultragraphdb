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

## 9c.6 The client

### Phase 1: the surface

Decided by the user before the surface (2026-10-30): the client authenticates through a generic `Login` that gives a
fresh mechanism for each exchange; the core is told the wall clock once per connection and maps the credential's
expiry onto its own time; the shells draw SCRAM's nonce from the operating system through `getrandom`.

- `ultragraphdb_protocol_transport`: the `Login` trait and `Anonymous`, the login of a client without credentials;
  `ReceivedFrame::check_order`, the frame's sequence and protocol from its header alone, so a frame a receiver keeps
  to view later is checksummed once; `Outbound::next_connection_frame`, the connection's own queue alone.
- `ultragraphdb_protocol_scram`: `ScramLogin`, the client's `Login`, a fresh `ScramClient` with the next nonce for
  each exchange; `system_nonce` and `NONCE_BYTES`, 18 bytes of the operating system's randomness in base64;
  `ScramError::Random`. `getrandom` 0.3 pinned in the workspace, already in the lockfile beneath other crates.
- `ultragraphdb_client_core`: `ClientCore<L: Login>`, `new(config, login)`; `connect(now, unix_micros)`;
  `principal`, `expires`; `welcome` from the Welcome on, through the exchange; frames numbered as they leave
  (`transmit`) and the server's taken only in their order (`receive`), the checksum dedupe gone; the exchange and its
  renewal, a margin before the expiry (`ClientConfig::with_renewal_margin`, 60 s, at most half the remaining life),
  through `next_deadline` and `handle_timeout`; reads and executes on streams of their own naming their transaction
  (`ClientError::TransactionMismatch`), ended with it by its Commit's reply, its Abort or an Error on its stream;
  `cancel` of a read alone; a retry refused as evicted is `ClientError::OutcomeUnknown` and forgets the session.
- The shells: `Client<C, L: Login>`, `new(connector, config, login, buffers)`; a framer for each connection, set to
  the Welcome's protocol; the wall clock read at each connect; `Anonymous` and `Login` re-exported.

Found in the work and fixed: a closing `ServerConnection` sent the replies queued on its streams after its closing
Error, so the Error was not its last frame, as section 2.6 has it; while closing it now sends its own queue alone.

The scripted server is replaced by `TestServer` (user's decision, 2026-10-30), the minimal server covering the core,
which grows with the node in group 10: the transport's `ServerConnection` holds the connection's rules (handshake,
SCRAM and its renewal, the frames' order, the stream rules, credit, cancel), each request takes one fixed answer, the
session table answers a retry from its record (code 18 for a request with no transaction open, code 12 for an
unknown session), and a test names its fault: `drop_after_next_commit`, `lose_next_commit`, `answer_busy_retries`,
`evict`. `ServerScript` and the withheld-commit coupling are gone.

Client drops (`DropCommand` sent and retried through a session) are left to the commit and store slice.

### Phase 2: the failing run

The suites written against the surface: the harness on `TestServer` with one framer a connection and the wall clock
following the harness's time; the core's tests of authentication (SCRAM, a wrong password, no mechanism spoken,
requests waiting for the final message, renewal while a read streams, a short credential, a new connection), the
retry rules (NotCommitted keeps the request number and the rerun commits once; an evicted session's retry is an
unknown outcome, not retried; a first commit refused as evicted), reads on streams of their own (two at once, a
cancelled read, a transaction's end ending its reads, a read naming another transaction), the frames' numbering, a
repeated frame of the server; the conformance tests with the golden reads in the transaction of stream 5 and every
golden reply numbered as the connection's next; the transport's tests of `check_order`, the connection's own queue,
a closing connection and `Anonymous`; SCRAM's of `ScramLogin`, `system_nonce` and the `Random` error; 8 of 9c.5's
scenarios bound in the client's feature: NotCommitted, evicted, a repeated frame, the three of reads on their own
streams, SCRAM authentication and renewal. The shells' tests on `TestServer`.

2026-10-30 17:01, `bazel test` against the surface:

| Target | Result |
|---|---|
| `ultragraphdb_protocol_transport` unit | 81: 67 passed, 14 failed |
| `ultragraphdb_protocol_scram` unit | 26: 20 passed, 6 failed |
| `ultragraphdb_protocol_transport_compio` unit | 13 passed |
| `ultragraphdb_protocol_transport_compio` bdd | 18: 12 passed, 6 failed |
| `ultragraphdb_client_core` unit | 48: 4 passed, 44 failed |
| `ultragraphdb_client_core` bdd | 13 scenarios failed |
| `ultragraphdb_client_tokio` unit | 4 failed |
| `ultragraphdb_client_compio` unit | 3 failed |

Every failure an `unimplemented!()` panic of the surface but one, the transport test's own: it damaged a Credit,
which has no body, so the flip broke the header; it damages an Error's chunk now. The compio conversations' 6 are the
closing connection's own queue.

### Phase 3: the audit

Script in the session's scratchpad (`audit_client9c.py`), each defect planted in place, run by `bazel test` over the
transport's, the compio transport's, SCRAM's, the client core's and both shells' targets, restored in a `finally`,
the three source trees hashed before and compared after (equal). 59 defects:

| Area | Defects |
|---|---|
| Transport | the order's sequence, its protocol, the Welcome's exemption unchecked; a view skipping the order; the connection's own queue taking the streams; a closing connection sending its streams; `Anonymous` speaking a mechanism |
| SCRAM | the offer ignored, any offer taken; one nonce reused; the login unchecked; a nonce of zeros, half drawn |
| Numbering | frames unnumbered; numbering not reset on a new connection; the server's order unchecked; a late frame refused |
| Authentication | an exchange where none is asked; up with no mechanism spoken; the first message, an answer unsent; up before the final message; the principal unrecorded; a renewal as another principal taken; stream 0's Error ignored; an exchange's error ignored; commits not resent after the exchange; the expiry kept at 0; renewal at the expiry; the margin uncapped; the wall clock ignored; a renewal never due, never started, repeated; the Welcome hidden while authenticating; requests while authenticating; the principal kept across connections |
| Reads | a read on its transaction's stream; the transaction unnamed; a read not tied to it; a read without its initial credit; a commit, an abort, a refused transaction keeping the reads; every transaction's reads ended; the last batch keeping its stream; a done read finished; a kept frame not viewed; a cancel unsent; a cancelled read kept |
| Retries | an evicted retry retried; the evicted session kept; every evicted commit unknown; a retry unmarked; NotCommitted advancing the request |
| Shells | the tokio framer kept across connections; the tokio wall clock 0; the compio protocol unset |

The first run rejected 45; 14 survived. The suite was strengthened for ten: every byte of a system nonce drawn; a
late frame of a cancelled read dropped, the connection kept; a renewal ending as another principal, and a server that
does not prove itself, giving the connection up (a test mechanism and frames of the test's own); a commit lost with
its connection retried once the next connection authenticated; the second renewal scheduled from the wall clock at
the first's end; no grant before a read's initial credit is spent; a refused transaction ending its reads; another
transaction's read going on when one transaction ends; the shells' renewal due before the expiry. Re-run, all ten
rejected. Four stand:

- A renewal repeated: clearing the renewal's deadline when it starts was dead, since the deadline is guarded while an
  exchange runs and replaced when it ends. The line is removed.
- Requests while authenticating, equivalent: every caller of the core's request framing is gated on the connection
  being up before it reaches it.
- A kept frame not viewed, equivalent: its order is checked on arrival, and its chunks are checked either way.
- The compio protocol unset, equivalent while protocol 1 is the only one: a framer left at 1 reads every frame.

The implementation that passed the audited suite is kept (phase 4 of this slice).

After it, `bazel test`: the transport 81 unit tests, SCRAM 27, the compio transport 13 and 18 scenarios, the client
core 56 and 13 scenarios, the tokio shell 5, the compio shell 4; every non-integration target of the repository, 118,
passing on macOS with group 8 beside it. The transport, compio, SCRAM and client targets passing on Linux 7.2 (arm64)
in the OrbStack machine with io_uring, 14 of 14.

## 9c.6 The commit, the store and the log

### Phase 1: the surface

- `ultragraphdb_store_apply`: `DropTarget`, what a drop decided in a round removes by UID (a predicate, a graph, a
  namespace's data, a namespace, every namespace), held by `Executed::dropped` in place of the predicate's UID;
  `DropRequest`, what a Drop command names; `prepare_drop(namespace, DropRequest)` in place of
  `prepare_drop_predicate`, each kind's key ranges written, applied by the round at its version
  (`ChangesetCore::drop_at`); `prepare_replayed_drop(namespace, DropTarget)`; a counter add answers its new value,
  an i64 in two's complement (point 33).
- `ultragraphdb_transactions_txn`: `Prepared::drop(applier, namespace, DropRequest, session, request, principal,
  buffers)` in place of `drop_predicate`, a drop through a session; `WriteSetBuilder::with_principal` and
  `Transaction::with_principal`, the record's `principal`; operations 4 to 7 named in section 10; `MAX_WRITE_SET`,
  1,040,384 bytes, the default bound of a write set, the Welcome's (B5), in place of the 1 MiB frame.
- `ultragraphdb_transactions_commit`: a session's owner, the principal that registered it, kept in its entry (the
  entry's fixed part 32 bytes, the owner after the version) and checked before anything else, `PermissionDenied`
  (code 21, `detail` 2, write) for another principal; `Sessions::retry` and `Pipeline::retry`, a retried Commit with
  no transaction open answered from the table or refused as `NotCommitted` (code 18, `detail` the request,
  retryable); `Input::Register { ticket, principal }`, `Pipeline::register(ticket, principal, now)`, a registration's
  record naming its owner; replay redoing drops 4 to 7 and registering each session for its owner;
  `RecoveryError::BehindCheckpoint`, a store below the version the log's checkpoint covers, naming both.
- `ultragraphdb_traits_log`: `OrderedLog::trim_through(position, version)`, the checkpoint's last commit version
  with it, and `checkpoint_version`; `ultragraphdb_transactions_log`: the superblock's format 2, 72 bytes, the
  version at offset 48 (point 34).
- `ultragraphdb_client_core`: `ClientCore::drop_command(DropCommand, session, now)`, a drop through a session, retried
  as a commit is; `Reply::Dropped`. `TestServer` answers a Drop from its session table as a commit.

### Phase 2: the failing run

The suites written against the surface: the store's tests of every kind of drop decided in a round against its
immediate drop, the oracle, of their targets and ranges, of a graph the store does not hold, and of every kind
replayed; the counter's new value; the txn tests of each drop's operation and ranges in the record, of the
principal in the record and in a transaction's, and of a drop through a session; the session table's owner, its
retry answers and the owner surviving a reload; the entry's layout with its owner; replay of every kind of drop
through a session with its owner, and of a store behind its checkpoint; the checkpoint's version in the superblock and
across a reopen; the client's drop and its retry through the session. 9 of 9c.5's scenarios bound in the commit
crate's features: the counter's new value, a retry never committed, a session's principal, the write-set bound
(the scenario "A write set over the maximum frame is refused" renamed as the spec has it), a store behind its
checkpoint, and the four of drops in `features/wire-protocol/drops.feature`.

2026-10-30 18:03, `bazel test` against the surface:

| Target | Result |
|---|---|
| `ultragraphdb_store_apply` unit | 181: 170 passed, 11 failed |
| `ultragraphdb_transactions_txn` unit | 72: 68 passed, 4 failed |
| `ultragraphdb_transactions_commit` unit | 144: 134 passed, 10 failed |
| `ultragraphdb_transactions_commit` bdd | 37 scenarios: 24 passed, 13 failed |
| `ultragraphdb_transactions_log` unit | 160: 159 passed, 1 failed |
| `ultragraphdb_client_core` unit | 58: 56 passed, 2 failed |

Every failure an `unimplemented!()` panic of the surface but three, tests asserting the values the slice changes on
purpose, updated: the default write-set bound, and a session's 48 bytes in the table's account.

### Phase 3: the audit

Script in the session's scratchpad (`audit_commit9c.py`), each defect planted in place, run by `bazel test` over the
store's unit target, every transaction crate's targets and the client core's, restored in a `finally`, the three
source trees hashed before and compared after (equal). 57 defects:

| Area | Defects |
|---|---|
| Store drops | a graph drop of another graph, without its split parts; a data drop taking the catalog, a namespace drop keeping it; every prefix without the catalog; an unknown graph a drop; a blank graph the default; a graph target naming no graph; a drop at its version of a graph, the namespace or every namespace dropping another target; a drop at the next version; a namespace drop writing no initial catalog; ranges ending at their start; a replayed drop of nothing |
| Counter | its delta answered; a negative value answered as 0 |
| Record | a graph drop, a data drop, an every-namespace drop named by another code; the principal unwritten, a drop's dropped, a drop through no session; the rewrite losing the principal; a transaction's principal unwritten; the write set bounded by the frame |
| Sessions | the owner unchecked, checked after busy, lost by a commit, registered for nobody, not drained, not loaded, unwritten in the entry, read from the version; a retry of the next request taken as committed; NotCommitted not retryable; the two codes' details |
| Step and replay | decided as principal 0; a registration's record and table naming nobody; replayed graph, namespace and every-namespace drops of another target; replay registering nobody; the checkpoint unchecked, naming the store twice |
| Log | the checkpoint's version unwritten, read from the next field, lost by a restart at the checkpoint, read as 0; a version 1 superblock taken; a trim recording none |
| Client | a drop through no session; a drop leaving its session free; a drop retried as a commit; a drop's reply of another request taken; a drop answered as a commit |

The first run rejected 45; one plant did not build and was corrected; 11 survived, and the suite was strengthened
for ten: a graph whose lists split dropped in a round, its parts with it; the store a drop is tried on holding
`rdf:type`, and a namespace's initial catalog checked after either drop, which share one helper, so the oracle
comparing them could not tell; a record written again keeping its principal; the replayed session's owner served, not
only another refused, and a statement of another namespace through the log, which only a drop of every namespace
removes; the restart at the checkpoint, reached by the torn checkpoint round, keeping its version; a drop keeping its
session busy; a drop's reply of another request a mismatch. Re-run, all rejected. One stands, equivalent: a drop
decided in a round applied at the store's next version instead of its own leaves the same store, the drop being
physical and the round's last batch setting the watermark; only a read between the two could tell.

The implementation that passed the audited suite is kept (phase 4 of this slice).

## 9c.7 Phase 4, the frame and the connection

Points 2, 3, 17, 19 and 45, 32 and 44, the handshake with authentication, reads on their own streams with credit per
request (B3, points 1, 30) and the golden frames regenerated are the wire's, the transport's and the client's slices
of 9c.6, whose implementations passed their audited suites and are kept. Point 55's idle limit is the server
connection's; TCP keepalive is the server's socket, group 10's. Done here:

- Point 16, found in the work: the log wrote a Round frame's header with sequence 0 and no release, where `PROTOCOL.md`
  §12 numbers it by its round's position and names the release of the node that wrote it. `encode_round` takes the
  release (`WalConfig::with_release`, 0 by default) and numbers the header by the position; a scan refuses a round
  whose header names another sequence as damage. A round the log writes is now the golden `round.frame` byte for
  byte, which a test checks, so stage 10's replication carries frames a receiver checks as every other frame.
- Point 7 through the server connection: a Hello above the server's protocols is answered with code 16 on stream 0,
  `detail` the server's highest, and the connection closes.

After 9c.6 and 9c.7, `bazel build //...` and every non-integration target of the repository, 118, passing on macOS;
the protocol, client and log targets, 20 of 20, on Linux 7.2 (arm64) in the OrbStack machine with io_uring.

## 9c.8 Phase 4, terms and results

Every point is the wire's or `ultragraphdb_core_terms`', implemented in 9c.6's wire slice and kept from its audited
suite, and section 1, 9.1 and 10 state each: point 11, a timezone of −00:01 kept under tag `0x30` or `0x31`; point 12,
section 10.5's forms the protocol's own, held by the 114 vectors `generate_by_value.py` computes from the section alone;
point 13, `EncodedTerm::check` refusing non-UTF-8 inline text and a known-datatype code outside section 10.4, which the
batch's view applies to each term; point 49, a value its payload does not hold exactly (an integer past i64, a decimal
of 19 fraction digits) kept by its lexical form; point 50, a boolean of 2 or more refused; point 51, the OperationFailed
value a one-entry request term table; point 52, the cell count computed in 64 bits; point 54, section 1's byte-order
row; tag `0x02` (B1) and the statement IDs of section 7 (B4). Setting `0x02` from the dictionary's skolem flag, and
refusing a request IRI under the genid base the dictionary does not hold, are the server's (10.2).

## 9c.9 Phase 4, requests, errors and sessions

Points 21, 22, 29, 33, 36, 38, 40 and 48, the session's owner, the Drop command with operations 4 to 7 and their
replay (B2) and the write-set bound (B5) are the wire's, the commit crate's and the client's, implemented in 9c.6 and
kept from its audited suites. Points 4, 5, 9, 14, 20, 35, 37, 39, 46 and 47 are stated in sections 4.4, 4.7, 6.3, 8.3,
9.1 and 11.4 to 11.8. Found and done here:

- Point 9: no test held a read that matches nothing, and `TestServer::with_results(0)` sent no frame, so such a read
  never ended. It answers one batch of no rows flagged `LAST` now, which a client test reads to its end, and a wire test
  builds and views that batch.
- Point 35: `TestServer` kept its statements across connections and `Statement` named its id "in the session". A new
  connection clears them, and a client test executes a statement of the previous connection, refused with code 10, then
  prepares again from id 1.
- Point 8: section 11.9's 256 statements per connection were in no code; `StatementCache::DEFAULT_CAPACITY` holds them,
  as `Sessions::DEFAULT_CAPACITY` holds the 4,096 sessions. The Welcome's defaults are the wire's constants.
- Point 14: the triage row reads "record order, from 0", but its recommendation is to state the code's rule, and the
  catalog numbers from 1 in UID order (`Catalog::preorder`), which section 8.3 states. Point 4's order is the same:
  `DataDictionary::describe` lists each kind's declarations by UID.

Each new test failed before its change: the empty read and the statement of the previous connection ran out of steps,
the default did not build; and a default of 255 is rejected. The client commits only through a session it registered,
which section 6.3's session 0 permits and does not require. The server's halves (point 5's term rules, point 9's empty
answer, point 46's abort of a closed connection's transactions, point 47's resume keys, the configured limits of point
8) are the session layer's, group 10.

After 9c.8 and 9c.9, `bazel build //...` and every non-integration target of the repository, 118, passing on macOS;
the protocol and client targets, 17 of 17, on Linux 7.2 (arm64) in the OrbStack machine with io_uring.

## 9c.10 Phase 4, the log

Implemented in 9c.6's commit, store and log slice and kept from its audited suite: the write-set record's `principal`
at offset 32 of its 40-byte fixed part (section 12.2), written by `WriteSetBuilder::with_principal` and kept when a
record is written again, which replay reads to register each session for its owner; the checkpoint's last commit
version passed to `OrderedLog::trim_through` and kept in the superblock's format 2 at offset 48, read back by
`checkpoint_version` across a reopen and a restart at the checkpoint; recovery refusing a store whose applied version
is below it, `RecoveryError::BehindCheckpoint` naming both (point 34). Section 12.2 states point 15: the keys' and
values' contents, the posting format of list deltas and the commit record are the store's, versioned by its mapping
contract, and the file states their framing.

## 9c.11 The transport's state machines audited

Script in the session's scratchpad (`audit_9c11.py`), each defect planted in place, alone, run by `bazel test` over
the transport's, the compio transport's, SCRAM's and every client target, restored in a `finally`, the two transport
source trees hashed before and compared after (equal). The 63 defects are new: none repeats 9c.6's plants, which
covered the checks; these take the machines' transitions and boundaries.

| Machine | Defects |
|---|---|
| `Framer` | the body read one byte short; a frame of the buffer's capacity refused, one past it taken; a short buffer looked at; a close inside a header reported clean, one between frames truncated; a close or a finished frame keeping the frame's want; the sequence expected from 0; a client's frame viewed as a server's |
| Handshakes | Hello, Welcome at sequence 0; the Welcome's protocol unchecked; the handshake's refusal unread; a first frame refused, a Hello refused, silently; a first frame read as a server's |
| `BufferPool` | a buffer never given back, given back full; one buffer fewer |
| `Outbound` | credit never spent, spent by every frame; a last frame keeping its stream; the lowest stream always first; the turn never moving; a grant replacing the credit; a cancel keeping the stream; one stream past the limit; the connection's frames last in, first out |
| `ServerConnection` | credit ignored; a cancel of no stream reported, a cancel reported keeping the stream; a frame not counted as activity; an exchange starting at step 1, forgotten between steps; a credential's life from the connection's start; an expiry closed as unauthenticated, an authentication timeout as expired; an idle close sending an Error; the deadline ignoring the expiry; no principal without authentication; the Welcome unrecorded; an authenticating connection serving; a refused Hello reading on; a client's Result taken; a read of no transaction closing the connection; the stream limit's refusal without its limit; a refusal of a stream never opened lost; Authenticate on stream 1 |
| `ClientAuthentication` | step 0 expected back; the final principal, the final expiry lost; the server's refusal read as a rule broken |
| `ReceivedFrame` | a client's frame validated as a server's; the Welcome not exempt from the protocol; the command, the flags read at the wrong offset |
| The compio `Connection` | an interrupted read failing; a close inside a frame reported clean; a frame written unflushed; a refused Hello's answer unsent; the framer not reset before a frame; a read past the frame |

The first run rejected 53; 10 survived. One was a defect of the shell, found by the audit: the compio `Connection`
hands its handshake frames to `ClientHandshake` and `ServerHandshake` as bytes, past the order check the client core
and `ServerConnection` make, so a compio client took a Welcome of any sequence, and a compio server a Hello. Section
2.5's step 13 makes each side's first frame its sequence 1, a rule of the handshake, so both machines check it now,
`SequenceOrder(1, found)`, and every shell has it; the tests' handshake frames are numbered 1. The suite was
strengthened for six more: the Welcome the server's first frame, numbered 1; a Hello naming a view or a replica
refused by `ServerHandshake` itself; a reply spending none of its stream's credit; a Cancel of a stream not open
handled with nothing sent; a command only a server sends (Result, Welcome, Error) closing the connection; an
interrupted read retried, and a frame written flushed, over a stream of the tests' own that is interrupted once and
holds writes until a flush. Re-run with two plants removing the new checks, all rejected. Three stand, equivalent:

- A client's frame viewed, or validated, as a server's: a `ReceivedFrame` comes from `Framer::finish` alone, whose
  header the framer verified by the client's rules, and the two validations differ only in those rules.
- The framer not reset before a frame: every way out of `read_frame` leaves the framer waiting for a header
  (`finish`, `closed`, the two refusals), so the reset differs only after a read dropped inside a frame, which loses
  the connection's place in the stream either way.

After 9c.10 and 9c.11, `bazel build //...` and every non-integration target of the repository, 118, passing on macOS;
the protocol and client targets, 17 of 17, on Linux 7.2 (arm64) in the OrbStack machine with io_uring.
