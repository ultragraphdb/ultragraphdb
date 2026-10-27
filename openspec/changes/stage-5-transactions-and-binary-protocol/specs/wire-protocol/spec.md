## ADDED Requirements

### Requirement: One header for every frame
Every frame SHALL begin with a 256-byte little-endian header: `checksum` u64 at 0, `checksum_body` u64 at 8,
`nonce_reserved` u128 at 16, `cluster` u64 at 32, `size` u32 at 40, `view` u32 at 44, `release` u32 at 48, `protocol`
u16 at 52, `command` u16 at 54, `stream` u64 at 56, `replica` u8 at 64, `flags` u8 at 65, 62 reserved bytes at 66 and a
128-byte command header at 128. Client frames, log records and, from stage 10, replica frames SHALL share it.

#### Scenario: A golden header round-trips
- **WHEN** the golden Hello frame's header is read
- **THEN** each field holds its golden value, and writing the fields back yields the same 256 bytes

### Requirement: XXH3-64 checksums
`checksum` SHALL be XXH3-64, as `doc/xxhash_spec.md` of the xxHash repository specifies it, with seed 0, of header
bytes 8 to 256; every checksum SHALL be stored as a little-endian u64, not in the specification's big-endian display
form.

#### Scenario: A checksum matches the specification's reference
- **WHEN** a header's checksum is computed by this crate and by the xxHash reference implementation over the same bytes
- **THEN** both give the same 64-bit value

### Requirement: Chunked body checksums
A body SHALL be checked in chunks that end on 4,096-byte boundaries counted from the frame's start: the first chunk is
the body's first 3,840 bytes and every later chunk 4,096 bytes, the last possibly shorter. Each chunk's leaf SHALL be
XXH3-64 seeded with the chunk's index. A body of more than one chunk SHALL end with its table of leaves, the chunks
covering the bytes before the table, and `checksum_body` SHALL be XXH3-64 of the table with seed 0; a body of at most
one chunk, an empty one included, SHALL carry no table, and `checksum_body` SHALL be its leaf. A body whose length
cannot be split into chunks and their table SHALL be refused. A failed chunk SHALL be reported with its index and byte
range.

#### Scenario: A corrupted chunk is named
- **WHEN** one byte of chunk 3 of a 20 KiB body is flipped
- **THEN** the frame is refused with an error naming chunk 3 and its byte range

#### Scenario: Swapped chunks fail
- **WHEN** chunks 1 and 2 of a body are swapped
- **THEN** the frame is refused naming chunk 1

#### Scenario: A small body carries no table
- **WHEN** a frame with a 100-byte body is built
- **THEN** its body is 100 bytes, and `checksum_body` is the leaf of that one chunk

#### Scenario: A body length with no valid split is refused
- **WHEN** a frame arrives whose body is 3,841 bytes, one byte past the first chunk with no room for a table
- **THEN** it is refused with an error naming the body's length

### Requirement: Frames are validated before use and never panic
A receiver SHALL check, in order: the length; the header checksum; the command known, the reserved bytes zero, the
cluster its own and `size` at most 1 MiB; the body's chunk table and chunks; the body's structure; then the command's
own rules. A failure before the command's own rules SHALL be a `ProtocolError` naming the rule, answered with an error
frame where the connection can still carry one and followed by closing the connection. A refusal by the command's own
rules (a stale statement, an abort, a refused write) SHALL be an error reply on the request's stream, and the
connection SHALL stay open. No input SHALL panic.

#### Scenario: A frame of another cluster is refused
- **WHEN** a frame whose `cluster` is not the server's arrives
- **THEN** the server answers with an error naming the cluster and closes the connection

#### Scenario: An unknown command is refused, not fatal
- **WHEN** a frame with a valid checksum names an unknown command
- **THEN** the connection is closed with an error naming the command, and the server keeps serving other connections

#### Scenario: A refused request keeps the connection
- **WHEN** a client executes a stale statement on one stream while another stream reads
- **THEN** the first stream receives an error reply, and the connection and the other stream continue

### Requirement: Frames are at most 1 MiB
A frame SHALL be at most 1 MiB. A result larger than a frame SHALL be sent as a stream of frames, the last flagged; a
write set larger than a frame SHALL be refused with an error naming the limit.

#### Scenario: A large result streams
- **WHEN** a read returns 5 MiB of results
- **THEN** it arrives as several frames on its stream, the last one flagged last

### Requirement: Bodies are read in place
A body SHALL be a fixed part followed by sections located by u32 offsets and lengths, each aligned to 8 bytes. A receiver
SHALL check every offset, length and count once, and UTF-8 where a section is text; afterwards every value SHALL be read
through a view borrowing the receive buffer.

#### Scenario: An offset past the body is refused
- **WHEN** a body's section offset points past its end
- **THEN** the frame is refused with an error naming the section

### Requirement: Commands keep their numbers
A command's number SHALL never be reused for another meaning; a retired command's number SHALL stay reserved, and ranges
SHALL be reserved for stage 10's replica commands.

#### Scenario: A retired number is refused
- **WHEN** a frame carries a retired command's number
- **THEN** it is refused as an unknown command

### Requirement: Handshake, streams, credit and cancellation
A client SHALL be configured with the cluster identity it connects to. A connection SHALL begin with Hello, carrying
that identity in its header and the client's protocol range and release, answered by Welcome with the chosen protocol,
the maximum frame, the streams allowed in flight and the initial credit. Each request SHALL take a stream id, and the
server SHALL answer streams independently, in whatever order their results are ready. A result stream SHALL send
frames only while the client holds credit, which `Credit` frames grant. `Cancel` SHALL end a stream, after which no
further frame of it is sent.

#### Scenario: Credit stops a stream
- **WHEN** a client grants credit for two frames of a result of ten
- **THEN** the server sends two frames and waits until more credit arrives

#### Scenario: Cancel ends a stream
- **WHEN** a client cancels a stream mid-result
- **THEN** no further frame of that stream arrives, and other streams continue

### Requirement: Data requests, prepared statements and describe
In stage 5 a read SHALL be a quad pattern with each position bound or unbound, a limit and a resume key; a write SHALL be
a changeset; a commit-time operation SHALL be its own request. A statement SHALL be prepared once per session from a
quad pattern with parameter slots, answered with a statement id, its parameter and result column types and a schema id,
and executed with parameters bound by position; after a catalog change its schema id SHALL no longer match, and an
execution SHALL be refused as stale. Describe SHALL answer from the data dictionary.

#### Scenario: A stale statement is refused
- **WHEN** a statement is prepared, the catalog of its namespace changes, and the statement is executed
- **THEN** the execution is refused as stale, and preparing it again succeeds

#### Scenario: Describe answers from the data dictionary
- **WHEN** `name` is declared a property with a `term` index and a client sends Describe for the namespace
- **THEN** the answer lists `name` as a property with its `term` index, as the data dictionary reports it

### Requirement: Result batches are columnar and dictionary-encoded
A result batch SHALL hold, per column, a u32 cell per row indexing the batch's term table, `u32::MAX` for unbound; the
term table SHALL hold each distinct term of the batch once in `ultragraphdb_core_terms`' encoding, with a text section
for IRIs and for literals the store holds by hash.

#### Scenario: A repeated term is sent once per batch
- **WHEN** a batch of 1,000 rows names the same IRI in every row
- **THEN** its term table holds that IRI once

### Requirement: Requests name terms by their text
A request body that carries RDF terms SHALL carry them in a term table of its own, one entry per distinct term, each a
kind and offsets into the body's text section: an IRI by its text, a blank node by its label scoped to the request, a
literal by its lexical form with its datatype's IRI (none for `xsd:string`) or its language tag and base direction, and
a triple term by the indices of three earlier entries of the same table. Every term the body names SHALL be an index
into that table. A receiver SHALL refuse an entry that repeats an earlier one, a triple term naming an entry at or
after its own, and a triple term nested deeper than 32. A client names a term only by its text; result batches keep
`ultragraphdb_core_terms`' encoding, which names IRIs by the store's UIDs.

#### Scenario: A request names an IRI by its text
- **WHEN** a Read binds its subject to the IRI `http://ex.org/a`
- **THEN** the view reads an IRI entry whose text is `http://ex.org/a`, borrowed from the frame

#### Scenario: A request names a blank node by its label
- **WHEN** a Write inserts a statement whose subject is the blank node labelled `b1`
- **THEN** the view reads a blank-node entry labelled `b1`

#### Scenario: A request names a typed literal by its datatype's IRI
- **WHEN** an Execute binds the literal `35` of datatype `http://www.w3.org/2001/XMLSchema#integer`
- **THEN** the view reads a literal entry of lexical form `35` and that datatype's IRI

#### Scenario: A request names a language-tagged literal
- **WHEN** an Operation's value is `chat` tagged `fr`
- **THEN** the view reads a literal entry of lexical form `chat` and language tag `fr`

#### Scenario: A request names a triple term by its parts
- **WHEN** a Write's object is the triple term of `http://ex.org/a`, `http://ex.org/knows` and `http://ex.org/b`
- **THEN** the view reads a triple-term entry naming three earlier entries, those IRIs

#### Scenario: A forward index is refused
- **WHEN** a triple term's entry names an entry at or after its own index
- **THEN** the frame is refused with an error naming the entry and the index

#### Scenario: A triple term nested deeper than the maximum is refused
- **WHEN** a request's term table holds a triple term nested 33 deep
- **THEN** the frame is refused with an error naming the entry and its depth

### Requirement: The conformance suite
The protocol SHALL be covered by golden frames for every command; a malformed case for every validation rule, every
truncation of every golden frame, bit flips in header and body, offsets past the end, overlapping and out of order, and
corrupted, swapped and missing chunks and tables; conversations for the handshake, multiplexed streams, credit
exhaustion, cancellation and session retries as Gherkin scenarios; and round-trip property tests of every builder
through its view. The suite SHALL run against the client and the server, and in the simulator under network faults.

#### Scenario: Every truncation is refused cleanly
- **WHEN** each golden frame is truncated at every length below its size and sent
- **THEN** each truncation yields a `ProtocolError` and none panics

### Requirement: The Rust client
The Rust client SHALL speak the protocol: handshake, sessions, transactions, data requests, prepared statements,
commit-time operations and result streams read through views. It SHALL retry a commit only through its session, so a
retry never commits twice.

#### Scenario: The client retries a lost reply through its session
- **WHEN** the connection drops after a commit is sent and the client reconnects and retries
- **THEN** the commit is applied once and the client receives its reply
