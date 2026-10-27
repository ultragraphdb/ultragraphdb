# Group 9: bodies, the bus and the conformance suite

## 9.1 Scenarios

- `ultragraphdb_protocol_wire/features/wire-protocol/wire_protocol_bodies.feature`, verbatim: "An offset past the body
  is refused", "A stale statement is refused", "Describe answers from the data dictionary", "A repeated term is sent
  once per batch", "Every truncation is refused cleanly". The steps are in `tests/bodies_steps.rs`, included by the
  existing `tests/bdd.rs`. The two Describe-backed scenarios run against the real data dictionary: catalog entries
  written into a simulated store (`Catalog::entry`), `DataDictionary::describe` over a `QuadStore`, and the answer
  carried through a Describe frame. The stale scenario takes its schema ids from two such answers, before and after
  `age` is declared.
- `ultragraphdb_protocol_bus/features/wire-protocol/conversations.feature`, verbatim: "Credit stops a stream", "Cancel
  ends a stream"; and three conversations the conformance suite asks for that the spec gives no scenario: "A handshake
  agrees on a protocol", "Streams are answered in whatever order their results are ready", "Credit holds over split
  and delayed frames". They run a client and a server `Connection` over the `SimNetwork`, the server's frames
  scheduled by `Outbound`. The bus has its own BDD target now (`tests/bdd.rs`, Bazel `:bdd`).
- Not here, and why: "A refused request keeps the connection" and "A large result streams" need the server's dispatch
  (group 10, as group 2 recorded); the conformance suite's "session retries" conversation needs sessions, which are
  group 7's and 10's. "Every truncation is refused cleanly" passes on frame-level checks alone, since a plain
  truncation fails `LengthMismatch` or `ShortHeader` before any body is read; the unit suite therefore also truncates
  every golden body with the frame rebuilt around it (`body_view_tests::test_every_truncation_of_every_body_is_refused`),
  which reaches every body view.

## 9.2 Phases 1 to 3

### Surface

- `ultragraphdb_error_protocol`: 14 body-structure variants appended to `ProtocolErrorEnum`: `WrongCommand`,
  `BodyTooShort`, `SectionOutOfBounds`, `SectionOffset`, `PaddingNotZero`, `TrailingBytes`, `SectionLength`,
  `EntryOutOfBounds`, `CountMismatch`, `NotUtf8`, `IndexOutOfRange`, `BadTerm`, `FieldValue`, and `Record { index,
  cause }`, which wraps a write-set record's refusal with the record it names. A constructor per variant, `Display`.
- `ultragraphdb_protocol_wire`: the traits `BodyView<'a>` (`from_frame`, `to_body`) and `BodyBuilder`
  (`command_header`, `body`, provided `frame(header)`); per command an owned body and its view: `Hello`, `Welcome`
  (`answer` negotiates a Hello), `Register`, `Begin`, `Commit` (`OperationResult`), `Abort`, `Read`, `Write`
  (a `Changeset`), `Operation` (`CommitOperation`, `Link`), `Prepare`, `Execute`, `Describe` (`schema_id`),
  `ResultBatch` (`ResultTerm`) and `ResultView` (`ResultTermView`), `Credit`, `Cancel`, `ErrorReply`, `RoundBody`
  (`Decision`) and `RoundView` (`transactions_overlapping`); `WriteSetRecord` and `WriteSetRecordView`; the shared
  `StringList`, `KeyValues`, `RecordEntry`, `QuadPattern<T>`, `PatternSlot<T>`, `ErrorCode`, `ConflictKind`,
  `ColumnType`; `StatementCache` with `StatementError`; the constants `FLAG_LAST` and `ABSENT`;
  `Header::with_command`. New dependencies: `ultragraphdb_core_mapping`, `ultragraphdb_core_terms`, `oxrdf` (see
  Decisions).
- `ultragraphdb_protocol_bus`: `BusError`; `BufferPool` and `PooledBuffer`; `Connection<S>` over any compio
  `AsyncRead + AsyncWrite` (`read_frame`, `write_frame`, `hello`, `welcome`); `ReceivedFrame`; `Outbound`; the
  simulated network `SimNetwork`, `SimStream` (compio's `AsyncRead`/`AsyncWrite`), `NetworkFaults`, `NetworkStats`.

### The layouts

Every layout is stated in the owned type's docs, as a native client would implement it. In short: fixed fields in the
128-byte command header, unused bytes zero; the body a table of `offset` u32 and `len` u32 per section, section 0 at
the table's end and each later one at the end of the one before rounded up to 8, zero padding, nothing after the last
section. Strings are two sections (u32 ends, UTF-8 text); key-value tables are entries of `key_len`, `value_len`
(`u32::MAX` for a removal), key, value, padding to 8. Changes and declarations are 16-byte records over an args
section (`utils/changes.rs` tables the kinds).

### Independent sources

- Golden frames: `testdata/golden/bodies/generate_bodies.py` lays out 21 frames (every command; two each for
  Operation, Prepare, Describe and Error) byte by byte from the layout tables, with Python's `xxhash` 3.6.0 and
  `hashlib`'s BLAKE2b-128, and prints each frame's size and checksums, which `tests/utils/golden_bodies.rs` copies. The
  Describe answer's schema id is the generator's XXH3-64 of the declarations-only body. Every builder is asserted to
  rebuild its golden frame byte for byte, and every view to read the generator's values.
- Malformed frames are rebuilt around a changed body by `reframe` in the tests, with `xxhash-rust` directly, and the
  sections are split and joined by a test-side layout (`split_sections`, `join_sections`), not the crate's.
- Term encodings in the Result golden are written from `ultragraphdb_core_terms`' tag table (UID, small string, big
  string by BLAKE2b-128); the test reads them through `EncodedTerm::view`.

### Corner cases, enumerated before the suite

| Case | Test |
|---|---|
| Every truncation of every golden body, frame rebuilt | `body_view_tests::test_every_truncation_of_every_body_is_refused` |
| Every bit flip of every body and of every command header | `body_view_tests::test_every_bit_flip_of_*` |
| Every unused command-header byte of every command | `body_view_tests::test_each_unused_command_header_byte_must_be_zero` |
| A body shorter than its table; an offset and a length past the body; misaligned, overlapping, out-of-order and gapped sections; nonzero padding; one byte past the end | `body_view_tests::test_each_layout_rule_is_refused_naming_its_section` (9 cases, each met by at least one body) |
| Every golden relabelled as every other command | `body_view_tests::test_a_frame_relabelled_as_every_other_command_never_panics` |
| Empty bodies (Register, Begin, Abort, Credit, Cancel), an empty changeset, a Describe request of seven empty sections | the golden tests, `write_tests::test_an_empty_changeset_is_a_valid_write` |
| Strings: empty string, multi-byte characters, a character split by an end, ends backwards, past the text, short of it, a ragged ends section | `string_list_tests` |
| Key-value entries: empty key and value, removal, an entry or its header past the section, nonzero padding | `key_values_tests` |
| Patterns: every slot kind, default graph off the graph position, parameters out of order or repeated, a parameter in a Read, bound terms out of order or unbound | `quad_pattern_tests` |
| Protocol ranges at 1, equal, backwards, above the server; `max_frame` 4,095, 4,096, 1 MiB, 1 MiB + 1; zero streams; zero credit | `hello_view_tests`, `welcome_tests`, `welcome_view_tests`, `credit_view_tests` |
| Result: zero columns, `u32::MAX` rows and columns (no overflow), vectors, typed literals by UID, an unbound cell, the first offset not zero, an encoding running into the next | `result_batch_tests`, `result_view_tests` |
| Round: no transactions; damaged bytes before, inside, across and after records; record offsets out of place, out of bounds; trailing bytes | `round_body_tests`, `round_view_tests` |
| Statements: capacity 0, eviction, unknown id 0, stale before parameter count, zero parameters | `statement_cache_tests` |
| Bus: frames split into many reads and delayed; close before, inside the header, inside the body; a header refused before its body is read; no buffer free; a frame past the buffer | `connection_tests` |
| Coincidence: the golden values of a field are distinct from its neighbours' (stream, flags, session, request, versions); Hello's zero view and stream are covered by the other goldens | every golden |

Every new `ProtocolErrorEnum` variant is constructed through the public API and asserted: `WrongCommand`
(`body_view_tests::test_a_view_of_another_command_is_refused`), `BodyTooShort`, `SectionOutOfBounds`,
`SectionOffset`, `PaddingNotZero`, `TrailingBytes` (`test_each_layout_rule_is_refused_naming_its_section`),
`SectionLength` (`commit_view_tests`), `EntryOutOfBounds` (`key_values_tests`), `CountMismatch`
(`string_list_tests`, `result_view_tests`, `round_view_tests`), `NotUtf8` (`hello_view_tests`, `string_list_tests`),
`IndexOutOfRange` (`write_view_tests`, `result_view_tests`), `BadTerm` (`write_view_tests`, `operation_view_tests`,
`describe_view_tests`, `result_view_tests`), `FieldValue` (most views), `Record`
(`round_view_tests::test_a_bad_record_is_named_by_its_index`). `StatementErrorEnum`: all four variants in
`statement_cache_tests`. `BusErrorEnum`: every variant but `Io` through the API in `connection_tests`, `outbound_tests` and
`buffer_pool_tests`. `Io` is built by its constructor only: neither the simulated stream nor loopback TCP fails on
command, and a fault-injecting stream for it would test the mapping of one `?`.

### Phase 2: the failing run

`cargo test -p <crate> --no-fail-fast` against the surface:

- `ultragraphdb_error_protocol`: 10 tests, 3 failed (the two constructor tests now listing the new variants, and the
  new `Display` test), 7 passed (the existing tests of the existing variants).
- `ultragraphdb_protocol_wire` unit: 181 tests, 112 failed, 69 passed: group 2's 66 and 3 new ones that pin derived
  equality of the enums `PatternSlot`, `Decision` and `CommitOperation` (declared surface, as group 2's constants
  tests). `bdd`: 14 scenarios, 4 failed, 10 passed: group 2's 9, and "Every truncation is refused cleanly", vacuous at
  this point for the reason in 9.1.
- `ultragraphdb_protocol_bus` unit: 32 tests, 32 failed. `bdd`: 5 scenarios, 5 failed.
- Every failure was an `unimplemented!()` panic in `src/`; no compile error, no panic elsewhere.

### Phase 3: the audit

The throwaway implementation was written against the suite. Before the audit, running it showed four test
expectations wrong against the documented check order, and they were corrected rather than the code changed. In
`round_view_tests`, bounds come before the offset rule, as for sections, so the record-offset case moves to 224, an
in-bounds offset. The out-of-bounds record names its end (6,232) against the records section (5,352). Trailing bytes
are body offsets (5,440, 5,441).

The test helper's canonical rebuild now covers Describe as well as Write, Operation and Result, since a builder
interns strings. In the Describe steps the store is opened before the declarations are written, because
`QuadStore::open` refuses a store without its contract version. The bus steps became async, because cucumber
already runs inside an executor.

68 defects, planted one at a time by a throwaway script that restored each file after its crate's suites ran,
across the classes of the protocol:

| Class | Defects |
|---|---|
| Off by one | section bound; string start; result-zeros start; error-code numbers; result cell |
| Flipped comparison or boundary | string ends `<=`; protocol range `<=`; `max_frame` exclusive; stream limit `>`; link direction `> 2`; overlap `<=` |
| Constant changed | alignment dropped; strings pushed not interned; result terms not deduplicated; version step halved; credit not spent; duplicate once; split never splits |
| Guard removed | section padding; trailing bytes; ends monotone; char boundary; entry padding; unused command-header bytes; default-graph place; parameter order; term order; result-types count; stale; parameter type; args start; subject reference; unknown flags; declaration order; unbound cell; first term offset; decisions count; record numbering; conflict-point value; delta of another kind; results unmetered; last frame closes; header verified before body; buffer capacity; Welcome range; room of the read buffer |
| Early return | Welcome never refuses |
| Order | parameter count `max(1)`; delay overtakes |
| Plausible neighbour | builder without padding; removal read as empty; answer ignores the client's max; mode swapped; created inverted; too-large retryable; slot kinds swapped; evict newest; statement-id halves swapped; cardinality lost; schema id over the data; cells row by row; link direction inverted; empty value present; overlap names records; turn never moves; cancel keeps the stream; truncation read as close; buffer not returned |

Results:

- 63 rejected by a test whose subject is the defect.
- 4 survived or were caught only incidentally (by the bit-flip test), and a test was added for each, then rerun and
  rejected:
  - term order: `quad_pattern_tests::test_bound_terms_follow_their_slots_and_are_all_bound`;
  - parameter count `max(1)`: `statement_cache_tests::test_a_statement_without_parameters_executes_with_none`;
  - declaration order:
    `describe_view_tests::test_declarations_come_predicates_first_then_entity_types_then_relations`;
  - first term offset: `result_view_tests::test_the_first_term_starts_its_section`.
- "Delay overtakes" survived and is equivalent. A link delivers from the front of its queue only, so a message never
  overtakes one sent before it, and the `max` against the last due step was redundant. The line was removed and the
  doc states the property.
- "Room of the read buffer" (a simulated read losing what does not fit) was first caught only as a hang: the lost
  bytes leave a read waiting forever, and the failure showed only through the runner's 600 s timeout. The bus tests now
  drive the simulated network with `tests/utils/ready.rs` rather than `block_on`. It polls a future at most 10,000
  times and fails if the future is still pending; a simulated read advances the network itself, so no test needs a
  wake-up from outside. Planted again, the defect fails 13 tests, `sim_stream_tests::test_a_read_takes_at_most_what_its_buffer_holds`
  among them, and the whole run takes 23 s with the build. The helper has its own test (`ready_tests`), and the bus's
  BDD steps include it by path.
- 25 runs (defects 26 to 50) first failed to compile because another agent's crates were mid-edit. They were rerun
  once the workspace built again, and the counts above are from the reruns.

Input variety: the goldens give neighbouring fields distinct values (session 9, request 7, version 1,000; streams 5,
7, 11, 13, 15, 17; two results with distinct UIDs and flags; terms of four encodings). The Hello golden's zero view and
replica are covered by group 2's Read golden.

## 9.3 to 9.5 Phase 4

The throwaway implementation, reviewed after the audit, is the implementation, as the tasks' header allows.

- Bodies and views read every field with `from_le_bytes` over `get(..)`, with no `unsafe` and no indexing that can
  panic. Every view checks once: the command, the command header, the section table, then its contents. Values are
  read in place afterwards. Request terms and declaration IRIs are parsed only in `to_body`.
- The Round body names, by `RoundView::transactions_overlapping`, the transactions whose write-set records overlap a
  damaged byte range of a body that need not pass its checks. Group 3 left this open for the WAL's interior-corruption
  report.
- The bus reads a frame's header into a pooled buffer, verifies it for the connection's cluster, refuses a frame
  larger than the buffer, then reads the body into the same buffer. `Outbound` meters only Result frames by credit
  and closes a stream once its `FLAG_LAST` frame leaves; streams take turns by id. The simulated network applies
  drop, duplicate, delay (in order) and split from one SplitMix64 stream. A `SimStream` read advances the clock while
  a message is in flight on its link, so ordinary async code runs on it with a trivial executor. The simulated read
  copies through compio's own reader of a byte slice, because a fixed-size compio `Slice` refuses
  `extend_from_slice`.
- A compio TCP loopback test runs the handshake and a two-chunk frame (`connection_tests::test_a_connection_runs_over_compio_tcp`);
  it passes under Bazel's sandbox.

Final counts, equal under both build systems:

| Crate | Cargo | Bazel |
|---|---|---|
| `ultragraphdb_error_protocol` | unit 10 | `:ultragraphdb_error_protocol_tests` 10 |
| `ultragraphdb_protocol_wire` | unit 185, `bdd` 14 scenarios (28 steps) | `:ultragraphdb_protocol_wire_tests` 185, `:bdd` 14 scenarios |
| `ultragraphdb_protocol_bus` | unit 33, `bdd` 5 scenarios (10 steps) | `:ultragraphdb_protocol_bus_tests` 33, `:bdd` 5 scenarios |

Doc tests: 0 in each, every `doc_test` target passes. `cargo clippy --all-targets -- -Wclippy::all` of the three
crates and of the two fuzz packages is clean.

## 9.6 Phase 5

Mutation testing and the fuzz runs are deferred to the stage's close (11.8, 11.7), as decided on 2026-10-27; the
audit above is phase 3's.

Fuzz targets, each built with `cargo +nightly fuzz build --fuzz-dir fuzz/<crate>`, none run:

| Package | Target | Property | Seeds |
|---|---|---|---|
| `fuzz/ultragraphdb_protocol_wire` | `body` | any frame through its command's view; an accepted body rebuilds its frame, byte for byte or, for Write, Operation, Describe and Result, to a fixed point | the 21 golden frames, `testdata/fuzz_seeds/body/` |
| | `round` | a Round body: decision and record counts agree, each record reads alone, the body rebuilds; the naming of damaged transactions never panics and names only the round's | the golden round's body, `fuzz_seeds/round/` |
| | `write_set_record` | any bytes as a record: refused, or encoded again to the same bytes | the golden round's two records, `fuzz_seeds/write_set_record/` |
| | `quad_pattern` | quad patterns with parameters, Prepare then Execute through the statement cache: a statement binds exactly its parameters | golden Prepare and Execute bodies, `fuzz_seeds/quad_pattern/` |
| | `body_fields` | structure-aware: every kind of body built from `arbitrary` fields reads back what was built; then its body mutated, the frame rebuilt, never a panic | the golden frames, `fuzz_seeds/body_fields/` |
| `fuzz/ultragraphdb_protocol_bus` (new) | `framing` | any bytes as a peer's stream, split by the simulated network: frames are read whole and in order, or closed, truncated or refused; every buffer returns to its pool | `ultragraphdb_protocol_bus/testdata/fuzz_seeds/framing/` |

`fuzz/Cargo.toml` pins `ultragraphdb_core_mapping`, `ultragraphdb_core_terms`, `ultragraphdb_protocol_bus`, `oxrdf`
and `compio` at the root workspace's versions. The client crate's decoders are group 10's.

## Decisions taken here, for review

- **Requests carry RDF terms as N-Triples text.** These are the terms of a Read or Prepare pattern, an Execute's
  parameters, a Write's changes and an Operation's arguments, one string per term, parsed with oxrdf
  (`Term::from_str`, written with `Display`). A client cannot use `ultragraphdb_core_terms`' encoding, which names IRIs
  and custom datatypes by UIDs only the server's dictionary knows. N-Triples is specified by the W3C and parsed by RDF
  libraries in every popular language. Results keep `core_terms`' encoding, as the spec requires, with up to two texts
  per term: an IRI or lexical form, and an annotation (a hashed language tag or a datatype IRI held by UID).
- **The wire crate depends on `ultragraphdb_core_mapping` and `oxrdf`**, beside `core_terms`, which the design's table
  lists alone. A Write carries the mapping crate's `Changeset`, a Describe its declarations, and an Operation decodes
  to oxrdf types. Neither dependency adds an external crate: oxrdf already comes in through `core_terms`.
- **One encoding per body.** Section offsets must be exactly the previous end rounded up to 8, padding zero, nothing
  after the last section. Overlapping, out-of-order, misaligned and gapped sections are all `SectionOffset`, naming
  the offset expected. Unused command-header bytes must be zero; they are reported as `ReservedNotZero` at the
  header's offset. Patterns bind terms in slot order. Describe's declarations come predicates first, then entity types,
  then relations.
- **Fixed fields go in the command header.** The body is sections only; the write-set record alone has a fixed part
  (32 bytes: namespace, read version, session, request). A reply is a frame of the request's own command on its stream,
  with the reply's fields filled: Register (session), Begin (read version), Commit (commit version, operation
  results), Prepare (statement, schema, result types) and Describe (the answer). Hello is answered by Welcome, and a
  refusal by Error.
- **Error codes 1 to 17** (`ErrorCode`, tabled in its docs), retryable exactly for Conflict, TooOld, PastLimit and
  Busy, as the transactions spec lists. A protocol refusal is one code, Protocol, with the error's text: per-rule
  codes would only serve a client that sent a malformed frame. `ConflictKind` 1 to 6 is a conflict's `detail`.
- **Schema id** = XXH3-64 of the Describe body built from the declarations alone, so it changes with the catalog and
  not with the data, and needs no version kept anywhere. A catalog changed and then changed back yields the same id
  again, and a statement prepared against it is valid again.
- **Statements.** `StatementCache` is pure session state in the wire crate, so the server and the client share it.
  - Parameters are numbered in order of appearance.
  - Prepare declares their types, and a node slot takes only a node.
  - A statement's result types are one per unbound slot.
  - Ids start at 1 and are never reused; past capacity the oldest statement is evicted.
  - Execute carries no schema id: a statement is stale when the id it was prepared against is not the namespace's
    current one.
- **Describe** answers with labels, relationship types, property keys and the catalog's declarations. Shapes and
  statistics are left out.
- **Flow control.** Credit meters Result frames only; replies need none and keep their place in their stream's queue.
  A Credit of zero frames is refused, while a Welcome's initial credit may be zero. A stream closes once a frame
  flagged `FLAG_LAST` is sent.
- **The simulated network is self-driving.** A read advances the clock while its link has a message in flight. Each
  write is one message, which can be dropped, duplicated, delayed (keeping order) or split into several reads.

## Answered on 2026-10-27

- Describe carries no shapes or statistics in stage 5.
- A protocol refusal stays one error code, Protocol, with the rule's text.
- The other decisions above stand, except the two the user changed:
  1. **Request terms use the result batches' term encoding**: `ultragraphdb_core_terms`' encoding and the text section,
     read in place, with no N-Triples parser in the wire crate.
  2. **The wire crate depends only on `ultragraphdb_error_protocol`, `ultragraphdb_core_terms` and `xxhash-rust`.**
     Write, Describe and Operation get layouts of their own, and their conversion to `Changeset`, declarations and
     oxrdf types is the server session's (task 10.2).

## Blocked: request terms in `core_terms`' encoding (2026-10-27)

The change was to be made only if every term kind a request needs is expressible in `ultragraphdb_core_terms`'
encoding without a server-side UID. It is not:

| Term kind | `core_terms`' encoding | Expressible by a client? |
|---|---|---|
| IRI | tag `0x01` + the UID, big-endian (`EncodedTerm::uid`) | no: only the dictionary knows the UID, and no tag names an IRI by its text |
| Blank node | the same tag `0x01` + UID; the encoding does not tell a blank node from an IRI | no |
| Literal of a datatype outside `KnownDatatype` | `SMALL_TYPED`/`BIG_TYPED`: the datatype's UID, then the lexical form or its hash | no: the datatype is named by UID |
| Triple term (RDF 1.2; `Change` objects may hold one) | no tag | no encoding at all |
| `xsd:string`, language strings (any direction), known datatypes, vectors | inline, by hash (BLAKE2b-128 of the text), by value, or by the known datatype's code | yes; by value needs oxsdatatypes' byte layouts, and a non-canonical form takes the known-code form |

Result batches avoid the problem because the server fills in the UID and adds the text. A request can't do that,
since the text is the only name the client has. Making the change needs an encoding decision this group can't take
alone. One option is new request-only tags in `core_terms` for an IRI by text, a blank node by label and a typed
literal by datatype IRI, each pointing at the text section. Another is a term kind of the wire's own, used only in
requests. Either one also has to cover triple terms or rule them out of requests. Item 2 is held with it: Write's and
Operation's own layouts are made of request terms, and Write's declarations share their record types with Describe's,
so splitting the rework would change the same layouts twice. The user chose the wire's own term kind; the next section
records it.

## Request terms in a table of the wire's own (decided 2026-10-27)

**The encoding.** A request body that names RDF terms carries a term table of two sections, records and text, beside
its own sections (`TermTable`, `TermTableView`). One 24-byte record per distinct term:

| Record offset | Field | Type |
|---|---|---|
| 0 | `kind` | u8: 1 IRI, 2 blank node, 3 literal, 4 language-tagged literal, 5 triple term |
| 1 | `direction` | u8: 0 none, 1 left to right, 2 right to left; nonzero only for kind 4 |
| 2 | reserved | 6 bytes, zero |
| 8 | `first` | u32: the first text's offset in the text section; a triple term's subject index |
| 12 | `first_len` | u32: the first text's length; a triple term's predicate index |
| 16 | `second` | u32: the second text's offset, where the first ends; a triple term's object index |
| 20 | `second_len` | u32: the second text's length; zero for a triple term |

The first text is an IRI, a blank node's label scoped to the request, or a literal's lexical form; the second a
literal's datatype IRI (empty for `xsd:string`, which is never stated) or a language tag (never empty). Texts follow
each other in table order with no gap and fill the text section, which is checked for UTF-8 once, every boundary on a
character boundary; no term appears twice. A triple term names three earlier entries, so the table has no cycle, and
its depth, computed in the same pass as the indices, is at most `MAX_TERM_DEPTH` (32), refused past it with
`TermTooDeep`; a repeated entry is refused with `DuplicateTerm`. One encoding per term, so an accepted body re-encodes
to its bytes. Every term a Read or Prepare pattern, an Execute's parameters, a Write's changes and an Operation's
arguments name is an index into the table; declarations name their IRIs, tokenizers and options in a list of names.

**Dependencies.** `ultragraphdb_protocol_wire` depends on `ultragraphdb_error_protocol`, `ultragraphdb_core_terms` and
`xxhash-rust` alone; `oxrdf` and `ultragraphdb_core_mapping` are dev-dependencies of its tests. `src/utils/terms.rs`,
the N-Triples parser, is removed. Converting Write, Describe and Operation bodies and request terms to `Changeset`,
declarations and oxrdf types is task 10.2's, iteratively or within the nesting bound.

**Phase 2.** With the surface in place and the suite rewritten: `ultragraphdb_protocol_wire` 66 of 200 unit tests and
10 of 21 scenarios failed, `ultragraphdb_error_protocol` 2 of 10 (the new variants).

**Phase 3.** 30 defects planted one at a time in the tree, each restored after its run, the crates' suites run under
Cargo (`audit_terms*.py` in the session's scratchpad). 25 were rejected at first by a test aimed at the rule. Four were
not:

| Defect | Why it survived | Closed by |
|---|---|---|
| a text's end bound one past the section (`end > limit + 1`) | refused anyway by the texts-fill-the-section check, as `CountMismatch` instead of `IndexOutOfRange` naming the entry | `test_a_text_one_byte_past_its_section_names_the_entry` |
| a term argument bounded by the larger of the term and name counts | the golden Write holds 17 terms and 15 names, so the larger is the term count | `test_a_term_reference_is_bounded_by_the_terms_not_the_names` (3 terms, 5 names) |
| a Write's term table read at the wrong section's offset | the offset is used only to name a reserved byte's position, which no test checked for a Write | `test_a_reserved_term_byte_is_named_at_its_body_offset` |
| "second offset written before the first text" | the plant was a no-op (`part.len() * 0`) | replaced by a real defect, a text appended before its offset is recorded, rejected by 27 tests |

Planted again, each of the four is rejected by its test. The agent was stopped during the audit, after four defects,
with the fifth (the `xsd:string` guard) still planted; it was restored and the audit finished from there.

**Final counts, Cargo and Bazel equal:** `ultragraphdb_error_protocol` 10; `ultragraphdb_protocol_wire` 203 unit tests,
21 scenarios; `ultragraphdb_protocol_bus` 33 unit tests, 5 scenarios. The fuzz targets that read request bodies (`body`,
`body_fields`, `quad_pattern`) are updated, `term_table` is new, seeds include a table at the maximum depth; all build,
none was run (11.7).

## Noticed outside this group

- `make fix` reports warnings in other groups' tests: `ultragraphdb_store_posting/tests/utils/posting_workload.rs:382`
  (a complex type) and `ultragraphdb_store_apply/tests/types/changeset_core/changeset_core_tests.rs:91` (the disallowed
  `std::thread::scope`).
- `AGENTS.md`'s crate table does not list the stage-5 crates yet (task 13.1). No external dependency was added here,
  so its dependency table needs no change from this group.
