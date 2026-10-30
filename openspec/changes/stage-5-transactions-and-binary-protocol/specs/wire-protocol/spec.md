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
cluster its own, the header's flags, view and replica, and `size` at most 1 MiB and the `max_frame` it announced; the
body's chunk table and chunks; the frame's sequence and protocol; the body's structure; then the command's own rules,
as `PROTOCOL.md` §2.5 orders them. A failure of a frame-level rule SHALL be a connection error: a `ProtocolError` naming
the rule, answered with an error frame on stream 0 and followed by closing the connection. A failure of a body's rules
SHALL be a stream error: an error reply on the request's stream, ending that stream, and the connection SHALL stay
open (`PROTOCOL.md` §2.6). A refusal by the command's own rules (a stale statement, an abort, a refused write) SHALL be
an error reply on the request's stream, and the connection SHALL stay open. No input SHALL panic.

#### Scenario: A frame of another cluster is refused
- **WHEN** a frame whose `cluster` is not the server's arrives
- **THEN** the server answers with an error naming the cluster and closes the connection

#### Scenario: An unknown command is refused, not fatal
- **WHEN** a frame with a valid checksum names an unknown command
- **THEN** the connection is closed with an error naming the command, and the server keeps serving other connections

#### Scenario: A refused request keeps the connection
- **WHEN** a client executes a stale statement on one stream while another stream reads
- **THEN** the first stream receives an error reply, and the connection and the other stream continue

#### Scenario: A malformed body keeps the connection
- **WHEN** a Write whose term table repeats an entry arrives on a transaction's stream while another stream reads
- **THEN** the transaction's stream receives an error of code 1 naming the rule, its transaction ends, and the
  connection and the other stream continue

#### Scenario: A request on stream 0 closes the connection
- **WHEN** a client sends Begin on stream 0
- **THEN** the server answers an error of code 1 on stream 0 and closes the connection

### Requirement: Frames are at most 1 MiB
A frame SHALL be at most 1 MiB, and at most the `max_frame` its receiver announced: the client in its Hello, the server
in its Welcome, each at least 524,288 bytes. A result larger than a frame SHALL be sent as a stream of frames, the last
flagged. A transaction's write set SHALL be at most the Welcome's `max_write_set`, which one Round frame holds, and a
larger one SHALL be refused with an error naming the limit.

#### Scenario: A large result streams
- **WHEN** a read returns 5 MiB of results
- **THEN** it arrives as several frames on its stream, the last one flagged last

#### Scenario: A frame past the receiver's announced maximum is refused
- **WHEN** a client sends a frame larger than the Welcome's `max_frame`
- **THEN** the server refuses it as too large, answers on stream 0 and closes the connection

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
that identity in its header and the client's protocol range, release and largest frame, answered by Welcome with the
chosen protocol, whether the connection authenticates and by which mechanisms, the maximum frame, the streams allowed in
flight, the initial credit, the largest write set and a transaction's time limit. Each request SHALL take a stream id,
and the server SHALL answer streams independently, in whatever order their results are ready. Every reply that ends its
stream SHALL carry the flag last, and Begin's reply SHALL not: a transaction's stream stays open until its Commit's
reply, its Abort or an error. A transaction's reads SHALL each take a stream of their own, naming the transaction, and
run beside each other at its snapshot; when the transaction ends, its reads still open SHALL end with it. A result
stream SHALL send frames only while the client holds credit, which `Credit` frames grant, starting with the initial
credit for each read. `Cancel` SHALL end a stream, after which no further frame of it is sent: on a read's stream it
ends the read alone, on a transaction's stream the transaction.

#### Scenario: Credit stops a stream
- **WHEN** a client grants credit for two frames of a result of ten
- **THEN** the server sends two frames and waits until more credit arrives

#### Scenario: Cancel ends a stream
- **WHEN** a client cancels a stream mid-result
- **THEN** no further frame of that stream arrives, and other streams continue

#### Scenario: A transaction's reads run on streams of their own
- **WHEN** a transaction runs two reads at once, each on a stream of its own naming the transaction
- **THEN** both stream their results at the transaction's snapshot, each with its own credit, and the transaction then
  commits on its own stream

#### Scenario: Cancelling a read keeps its transaction
- **WHEN** a client cancels one of its transaction's reads mid-result and then commits the transaction
- **THEN** no further frame of the read arrives, and the commit succeeds

#### Scenario: A transaction's end ends its reads
- **WHEN** a transaction commits while one of its reads has frames left to send
- **THEN** the read's stream sends no further frame, and the commit's reply arrives

### Requirement: Data requests, prepared statements and describe
In stage 5 a read SHALL be a quad pattern with each position bound or unbound, a limit and a resume key, naming its
transaction and asking, when it wants them, each row's statement ID; a write SHALL be a changeset, in its transaction's
namespace; a commit-time operation SHALL be its own request. A statement SHALL be prepared once per connection, which
holds it, from a quad pattern with parameter slots, answered with a statement id, its parameter and result column types
and a schema id, and executed with parameters bound by position; after a catalog change its schema id SHALL no longer
match, and an execution SHALL be refused as stale; a literal bound at a slot that takes a node SHALL be refused. A
resume key SHALL continue its own request only, and rows SHALL come in the store's key order. Describe SHALL answer
from the data dictionary, as far as the connection's principal may read it.

#### Scenario: A stale statement is refused
- **WHEN** a statement is prepared, the catalog of its namespace changes, and the statement is executed
- **THEN** the execution is refused as stale, and preparing it again succeeds

#### Scenario: Describe answers from the data dictionary
- **WHEN** `name` is declared a property with a `term` index and a client sends Describe for the namespace
- **THEN** the answer lists `name` as a property with its `term` index, as the data dictionary reports it

#### Scenario: A statement belongs to its connection
- **WHEN** a statement prepared on one connection is executed on another
- **THEN** the execution is refused with code 10, UnknownStatement

#### Scenario: A literal at a node slot is refused
- **WHEN** a statement's subject parameter is bound to a literal
- **THEN** the execution is refused with code 11, `detail` the statement

#### Scenario: A statement ID names the statement a read returned
- **WHEN** a read asks for statement IDs, and a Write deletes by the ID of one row it returned
- **THEN** that row's statement is removed, and every other statement remains

### Requirement: Result batches are columnar and dictionary-encoded
A result batch SHALL hold, per column, a u32 cell per row indexing the batch's term table, `u32::MAX` for unbound; the
term table SHALL hold each distinct term of the batch once in `ultragraphdb_core_terms`' encoding, with a text section
for IRIs and for literals the store holds by hash, and with a blank node told from an IRI by its own tag, `0x02`, its
text its skolem IRI; and, when the request asked, each row's statement ID. A literal SHALL be encoded by value only when
its payload holds the value exactly. A read that matches nothing SHALL answer one batch flagged last, with no rows.

#### Scenario: A repeated term is sent once per batch
- **WHEN** a batch of 1,000 rows names the same IRI in every row
- **THEN** its term table holds that IRI once

#### Scenario: A blank node is told from an IRI and named again
- **WHEN** a read returns a blank node, and a later request names the term's text as an IRI
- **THEN** the term carries tag `0x02` and its skolem IRI, and the later request reads the same node

#### Scenario: A read matching nothing answers one empty batch
- **WHEN** a read's pattern matches no statement
- **THEN** one batch arrives, flagged last, with no rows and a column per unbound slot

#### Scenario: A decimal the payload cannot hold is kept by its lexical form
- **WHEN** a literal `"0.1234567890123456789"^^xsd:decimal`, of 19 fraction digits, is written and read back
- **THEN** its term carries tag `0x30` and its lexical form unchanged

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

#### Scenario: A relative IRI is refused
- **WHEN** a Write names the IRI `people/alice`
- **THEN** the Write is refused with code 7, `detail` the term's index in its term table

#### Scenario: A skolem IRI no blank node holds is refused
- **WHEN** a request names an IRI under the namespace's `.well-known/genid/` base that the dictionary does not hold as a
  blank node's skolem IRI
- **THEN** the request is refused with code 7, `detail` the term's index

#### Scenario: A triple term nested deeper than the maximum is refused
- **WHEN** a request's term table holds a triple term nested 33 deep
- **THEN** the frame is refused with an error naming the entry and its depth

### Requirement: Connections authenticate
A server requiring authentication SHALL say so in its Welcome and list the SASL mechanisms it accepts. The client SHALL
authenticate by an Authenticate exchange on stream 0 before its first request, and the server's final Authenticate
SHALL name the principal the connection acts for and when its credential expires. A failed exchange SHALL be refused
with code 19, saying nothing of what failed, and a request before the exchange completes, or an exchange not complete
within the authentication timeout, with code 20; each SHALL close the connection. Before its credential expires, a
connection SHALL be able to authenticate again on stream 0 as the same principal while its other streams go on; a
credential that expires first SHALL close the connection with code 22, aborting its open transactions and keeping its
sessions. A server running without authentication SHALL say so in its Welcome, and its connections SHALL act for the
anonymous principal without an exchange (`ultragraphdb_protocol/SECURITY.md`).

#### Scenario: A client authenticates with SCRAM-SHA-256
- **WHEN** a server requiring authentication lists `SCRAM-SHA-256` and a client completes the exchange with the
  principal's password
- **THEN** the server's final Authenticate names the principal, and the client's requests are served

#### Scenario: A wrong password is refused without saying why
- **WHEN** a client's SCRAM proof does not verify, once for a principal that exists and once for a name no principal
  holds
- **THEN** each is refused with code 19 on stream 0 and the connection closed, and the two Error frames are the same
  bytes

#### Scenario: A request before authentication closes the connection
- **WHEN** a client sends Begin after Welcome and before the final Authenticate
- **THEN** the server answers code 20 on stream 0 and closes the connection

#### Scenario: An unfinished authentication times out
- **WHEN** a client sends Hello and then nothing for longer than the authentication timeout
- **THEN** the server answers code 20 on stream 0 and closes the connection

#### Scenario: A credential is renewed on the open connection
- **WHEN** a connection whose credential expires runs a new exchange as the same principal before the expiry, while one
  of its reads streams
- **THEN** the read continues, and the connection serves requests past the first expiry

#### Scenario: Renewing as another principal is refused
- **WHEN** a connection's new exchange ends as another principal
- **THEN** the server answers code 19 on stream 0 and closes the connection

#### Scenario: An expired credential closes the connection
- **WHEN** a connection's credential expires before a new exchange completes, with a transaction open
- **THEN** the server answers code 22 on stream 0 and closes the connection, the transaction aborts, and the
  principal's sessions serve its next connection

#### Scenario: A server without authentication says so
- **WHEN** a client connects to a server configured without authentication
- **THEN** the Welcome says no authentication is required and lists no mechanism, and the client's requests are served
  without an exchange

### Requirement: Frames are numbered
Each side SHALL number the frames it sends on a connection, the header's `sequence`, from 1 and one more for each
frame, and a receiver SHALL refuse a frame whose sequence is not one above the previous frame's from the same side, as a
connection error. A log frame's sequence SHALL be its round's position.

#### Scenario: A repeated frame closes the connection
- **WHEN** the network delivers one of the client's frames twice
- **THEN** the server refuses the second as out of sequence and closes the connection, and the client reconnects and
  retries its commit through its session, which commits once

### Requirement: Drops are commands
A client SHALL drop a predicate, a graph, a namespace's data, a namespace or every namespace by a Drop request on a
stream of its own, committed through a session as a commit is and answered with the drop's commit version. The commit
step SHALL decide a drop as the last transaction of its round, and its write-set record SHALL name the drop, so
recovery redoes it. A transaction that read or wrote what a drop removes, below the drop's version, SHALL abort with a
conflict of kind Drop.

#### Scenario: A graph drop removes the graph
- **WHEN** a client drops graph `g` of a namespace holding statements in `g` and in `h`
- **THEN** the reply carries the drop's commit version, no statement of `g` remains, and every statement of `h` remains

#### Scenario: A namespace's data drop keeps its catalog
- **WHEN** a client drops a namespace's data after declaring `name` a property
- **THEN** no statement of the namespace remains, and Describe still lists `name`

#### Scenario: A drop retried through its session applies once
- **WHEN** a Drop's reply is lost, and the client sends it again with the same session and request number
- **THEN** the second answer is the stored reply, and the drop is applied once

#### Scenario: A transaction writing into a dropped graph aborts
- **WHEN** a transaction reads before a drop of graph `g` commits and then writes into `g`
- **THEN** its commit is refused with a conflict of kind Drop

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
The Rust client SHALL speak the protocol: handshake, authentication and its renewal, sessions, transactions, data
requests on streams of their own, prepared statements, commit-time operations, drops and result streams read through
views. It SHALL retry a commit only through its session, so a retry never commits twice: a commit refused as not
committed (code 18) runs its transaction again, a busy session (code 14) is retried after a delay, and a retry refused
as evicted (code 12) reports an unknown outcome and is not retried.

#### Scenario: The client retries a lost reply through its session
- **WHEN** the connection drops after a commit is sent and the client reconnects and retries
- **THEN** the commit is applied once and the client receives its reply

#### Scenario: A retry of a commit that never arrived runs the transaction again
- **WHEN** the connection drops before a commit reaches the commit step, and the client retries it through its session
- **THEN** the retry is refused with code 18, the client runs its transaction again, and it commits once

#### Scenario: A retry of an evicted session reports an unknown outcome
- **WHEN** the client retries a commit whose session the table evicted
- **THEN** the retry is refused with code 12, and the client reports the commit's outcome as unknown without retrying
