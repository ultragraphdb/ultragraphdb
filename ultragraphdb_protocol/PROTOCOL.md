# UltraGraphDB binary protocol

This file specifies the bytes of UltraGraphDB's binary protocol, version 1: the frame, every command's body, the term
encodings, the error codes and the connection's rules. A client in any language implements the protocol from this file
and the golden frames in `ultragraphdb_protocol_wire/testdata/golden/`. The Rust crates `ultragraphdb_protocol_wire`
(the bytes) and `ultragraphdb_protocol_transport` (the connection's state machines) implement it for client and server.
`SECURITY.md` beside this file describes the security architecture the protocol carries: authentication,
authorization and encryption.

This file leads the code: group 9c of stage 5 brings the crates and the golden frames to it
(`openspec/changes/stage-5-transactions-and-binary-protocol/tasks.md`).

## Contents

1. Conventions
2. The frame
3. Commands
4. The body layout and its shared structures
5. Bodies of the connection: Hello, Welcome, Authenticate
6. Bodies of sessions, transactions and drops: Register, Begin, Commit, Abort, Drop
7. Bodies of data requests: Read, Write, Operation
8. Bodies of prepared statements and the dictionary: Prepare, Execute, Describe
9. Bodies of results and control: Result, Credit, Cancel, Error
10. The term encoding of results
11. The connection: handshake, authentication, streams, credit, cancel, sessions, statements, limits
12. Internal to the server and the log: Round and the write-set record

## 1. Conventions

| Rule | Statement |
|---|---|
| Byte order | Little-endian, except inside a result term's encoding (section 10): a term is the store's sortable key encoding, sent as stored, so its UIDs and by-value numbers are big-endian, which sorts keys as their values, and a vector's elements little-endian. Section 10 states the order per field |
| Integers | `u8`, `u16`, `u32`, `u64`, `u128` unsigned; `i16`, `i64`, `i128` two's complement. Widths in bytes: 1, 2, 4, 8, 16 |
| Offsets | A header offset counts from the frame's first byte. A command-header offset counts from the command header's first byte, the frame's byte 128. A body offset counts from the body's first byte, the frame's byte 256. A section offset counts from its section's first byte |
| Alignment | Every section starts at a body offset that is a multiple of 8. A multi-byte field needs no alignment in memory: read every field byte by byte |
| Padding | Every padding byte, reserved byte and unused command-header byte is zero; a receiver refuses a nonzero one |
| Text | UTF-8, delimited by offsets and lengths, with no terminator and no escape grammar. A string's bytes end on a character boundary |
| `ABSENT` | The u32 `0xFFFFFFFF`: no term, no string, the default graph, an unbound cell, a removal |
| Booleans | A u8 of 0 (false) or 1 (true); a receiver refuses any other value where this file says so |
| Checksums | XXH3-64 as `doc/xxhash_spec.md` of the xxHash repository specifies it, stored as a little-endian u64.  |
| One encoding per value | Where a rule below says "one encoding", a receiver refuses every other byte sequence for the same value, so re-encoding a decoded value gives the received bytes. Section 4.7 lists which layouts hold this rule and which hold it only by the sender's convention |

## 2. The frame

A frame is a 256-byte header and a body of 0 to 1,048,320 bytes. A frame is at most 1,048,576 bytes (1 MiB), header
and body. The body is the command's data followed, when the data takes more than one chunk, by its chunk table
(section 2.3).

### 2.1 The header

| Offset | Width | Field | Type | Meaning | Allowed values |
|---|---|---|---|---|---|
| 0 | 8 | `checksum` | u64 | XXH3-64, seed 0, of header bytes 8 to 255 | the computed value |
| 8 | 8 | `checksum_body` | u64 | The body's root checksum (section 2.3) | the computed value |
| 16 | 16 | `sequence` | u128 | The frame's place among the frames its sender sent on the connection: 1 for the first, one more for each after (section 11.2). A log frame's is its round's `position` | the expected value |
| 32 | 8 | `cluster` | u64 | The cluster's identity, drawn at random when the cluster is created | the receiver's cluster |
| 40 | 4 | `size` | u32 | The frame's length, header and body | 256 to 1,048,576 |
| 44 | 4 | `view` | u32 | The VSR view | 0 from a client, and on one node |
| 48 | 4 | `release` | u32 | The sender's release, opaque: logged, never compared | any |
| 52 | 2 | `protocol` | u16 | The frame format's version | 1 in Hello and Welcome; the Welcome's `protocol` after them (section 11.1) |
| 54 | 2 | `command` | u16 | What the body is (section 3) | a number of section 3 |
| 56 | 8 | `stream` | u64 | The request the frame belongs to | 0 for connection-level frames |
| 64 | 1 | `replica` | u8 | The sending replica | 0 from clients |
| 65 | 1 | `flags` | u8 | Bit 0 (`LAST`, value 1): the stream's last frame | bits 1 to 7: 0 |
| 66 | 62 | reserved | bytes | | all 0 |
| 128 | 128 | `command_header` | bytes | The command's fixed fields, laid out by each command | each command's rules |

### 2.2 The header checksum

`checksum` is XXH3-64 with seed 0 of the header's bytes 8 to 255 (248 bytes): `checksum_body`, every field after it and
the command header. A sender writes every other field first and `checksum` last.

### 2.3 The chunked body checksum

The body's data splits into chunks on the frame's 4,096-byte blocks:

| Chunk | Body bytes |
|---|---|
| 0 | 0 to 3,839: the rest of the frame's first block after the header |
| k ≥ 1 | 3,840 + 4,096 (k − 1) to 3,840 + 4,096 k − 1; the last chunk ends with the data |

A body whose data takes `D` bytes has `n` chunks: `n = 1` when `D ≤ 3,840` (an empty body is one empty chunk), else
`n = 1 + ⌈(D − 3,840) / 4,096⌉`.

| Chunks | Body | `checksum_body` |
|---|---|---|
| 1 | the data alone, `D` bytes | XXH3-64 of the data with seed 0 |
| n ≥ 2 | the data, then the chunk table: `n` leaves of 8 bytes, leaf k a u64 at data offset `D + 8k`; `D + 8n` bytes | XXH3-64 of the table's `8n` bytes with seed 0 |

Leaf k is XXH3-64 of chunk k with seed `k`. A body's length `L = size − 256` fixes its split: `L ≤ 3,840` is one chunk
of `L` bytes. For `L > 3,840` the receiver takes `n = max(2, ⌈(L + 256) / 4,104⌉)` and `D = L − 8n`; the split exists
when `D` takes exactly `n` chunks (`3,840 + 4,096 (n − 2) < D ≤ 3,840 + 4,096 (n − 1)`), and the receiver refuses the
frame otherwise. A body of 3,841 bytes, for one, has no split.

### 2.4 Worked example

`testdata/golden/bodies/hello.frame`, 279 bytes:

| Bytes | Hex | Field | Value |
|---|---|---|---|
| 0–7 | `b3 70 0d 76 02 a2 cb 13` | `checksum` | `0x13CBA202760D70B3` |
| 8–15 | `a6 d7 a5 4a ce a6 54 33` | `checksum_body` | `0x3354A6CE4AA5D7A6` |
| 16–31 | `01` then `00` × 15 | `sequence` | 1: the client's first frame |
| 32–39 | `ef cd ab 89 67 45 23 01` | `cluster` | `0x0123456789ABCDEF` |
| 40–43 | `17 01 00 00` | `size` | 279 |
| 44–47 | `00 00 00 00` | `view` | 0 |
| 48–51 | `00 00 05 00` | `release` | `0x00050000` |
| 52–53 | `01 00` | `protocol` | 1 |
| 54–55 | `01 00` | `command` | 1, Hello |
| 56–63 | `00` × 8 | `stream` | 0 |
| 64, 65 | `00`, `00` | `replica`, `flags` | 0, 0 |
| 66–127 | `00` × 62 | reserved | |
| 128–135 | `01 00 01 00 00 00 05 00` | command header: `protocol_min`, `protocol_max`, `release` | 1, 1, `0x00050000` |
| 136–139 | `00 00 10 00` | command header: `max_frame` | 1,048,576 |
| 140–255 | `00` × 116 | command header, unused | |
| 256–263 | `08 00 00 00 0f 00 00 00` | body: section 0's `offset`, `len` | 8, 15 |
| 264–278 | `75 6c 74 72 61 67 72 61 70 68 2d 74 65 73 74` | section 0 | `ultragraph-test` |

XXH3-64 (seed 0) of bytes 8 to 255 is `0x13CBA202760D70B3`, the `checksum`. The body's 23 bytes take one chunk, so
`checksum_body` is XXH3-64 (seed 0) of bytes 256 to 278: `0x3354A6CE4AA5D7A6`.

A body of two chunks, `testdata/golden/two_chunks.frame` (size 4,113): the data is 3,841 bytes, chunk 0 its bytes 0 to
3,839 and chunk 1 its byte 3,840 (`6e`). The table, body bytes 3,841 to 3,856, is
`9d c4 ec 34 58 68 8b f5 2e 3a 5a 40 b2 05 43 b5`: leaf 0 = XXH3-64(chunk 0, seed 0) = `0xF58B685834ECC49D`, leaf 1 =
XXH3-64(chunk 1, seed 1) = `0xB54305B2405A3A2E`. `checksum_body` = XXH3-64(table, seed 0) = `0x4169EE9E34DB3D5A`.

### 2.5 Validating a frame

A receiver checks a frame in this order and refuses it at the first rule broken. The name in the second column is the
rule's name in `ultragraphdb_error_protocol`'s `ProtocolErrorEnum`; the Error frame a server answers with carries the
rule's text in its message (section 9.4).

| Step | Refusal | Rule |
|---|---|---|
| 1 | `ShortHeader` | The frame holds at least 256 bytes |
| 2 | `HeaderChecksum` | `checksum` equals XXH3-64 of bytes 8 to 255 |
| 3 | `UnknownCommand` | `command` is a number of section 3 |
| 4 | `ReservedNotZero` (offset of the first nonzero byte) | Bytes 66 to 127 are zero |
| 5 | `WrongCluster` | `cluster` is the receiver's cluster |
| 6 | `FieldValue` (`"flags"`, then `"view"`, then `"replica"`) | `flags` bits 1 to 7 are zero; in a client's frame, `view` and `replica` are 0 |
| 7 | `SizeBelowHeader` | `size ≥ 256` |
| 8 | `FrameTooLarge` | `size ≤ 1,048,576`, and at most the `max_frame` the receiver announced: the server's in its Welcome, the client's in its Hello; a stream receiver also refuses a `size` past its receive buffer, naming the buffer's capacity |
| 9 | `LengthMismatch` | The bytes received equal `size` |
| 10 | `BodyLength` | The body's length has a split into chunks and table (section 2.3) |
| 11 | `Chunk` (index 0, bytes 0 to D) | One chunk: XXH3-64(data, seed 0) equals `checksum_body` |
| 11 | `ChunkTable` | Two or more chunks: XXH3-64(table, seed 0) equals `checksum_body` |
| 12 | `Chunk` (index, start, end) | Each chunk in order equals its leaf |
| 13 | `SequenceOrder` (the expected sequence, the frame's) | A connection's receiver: `sequence` is one above the previous frame's from the same sender, 1 for its first |
| 14 | `FieldValue` (`"protocol"`) | A connection's receiver: `protocol` is 1 in Hello and Welcome, and the Welcome's `protocol` in every later frame |
| 15 | `WrongCommand` | A command's view reads a frame of that command |
| 16 | the command's rules | The command header, then the body (sections 4 to 9, 12) |

A stream receiver reads the 256-byte header first, applies steps 1 to 8 to it, then reads `size − 256` more bytes and
applies the rest.

### 2.6 Connection errors and stream errors

A server answers a refusal in one of two ways:

| Refusal | Answer | The connection |
|---|---|---|
| A frame-level rule, steps 1 to 15 of section 2.5; a request on stream 0 other than Hello and Authenticate; a new request on a stream already open | Error 1 (Protocol) on stream 0, its message the rule's text | closed after the Error |
| A body's rule, step 16, or a rule of section 4 | Error 1 (Protocol) on the frame's stream, which the Error ends; a transaction's stream ends its transaction | stays open |

A client refusing a frame from the server closes the connection.

## 3. Commands

| Number | Command | Direction | Body | Answered by |
|---|---|---|---|---|
| 0 | unassigned: a zeroed header names no command | | | |
| 1 | Hello | client → server | sections | Welcome, or Error on stream 0 |
| 2 | Welcome | server → client | sections | |
| 3 | Register | both | empty | Register |
| 4 | Begin | both | empty | Begin |
| 5 | Commit | both | sections | Commit, or Error |
| 6 | Abort | client → server | empty | nothing |
| 7 | Read | client → server | sections | Result frames, or Error |
| 8 | Write | client → server | sections | nothing; a refusal is an Error |
| 9 | Operation | client → server | sections | nothing; its result comes with the Commit reply |
| 10 | Prepare | both | sections | Prepare, or Error |
| 11 | Execute | client → server | sections | Result frames, or Error |
| 12 | Describe | both | sections | Describe, or Error |
| 13 | Result | server → client | sections | Credit when the client wants more |
| 14 | Credit | client → server | empty | nothing |
| 15 | Cancel | client → server | empty | nothing |
| 16 | Error | server → client | sections | |
| 17 | Round | log and server only (section 12) | sections | |
| 18 | Authenticate | both, stream 0 | sections | Authenticate, until the server's final one; or Error on stream 0 |
| 19 | Drop | both | sections | Drop, or Error |
| 256–511 | reserved for the replica commands of stage 10 | | | |

A receiver refuses every other number, and these, as `UnknownCommand`. A retired command's number stays retired and is
refused the same way; no command is retired. "Both" means the request and its reply share the command and the layout:
the client sends it with the reply's fields zero, and the server answers with them filled.

## 4. The body layout and its shared structures

### 4.1 Sections

Every body, and the write-set record (section 12.2), is a fixed part, a section table, and the sections:

| Body offset | Contents |
|---|---|
| 0 | the fixed part, `F` bytes, a multiple of 8. Every command body has `F = 0`; the write-set record has `F = 32` |
| F | the section table: for each of the command's `N` sections, `offset` u32 then `len` u32, both body offsets and lengths |
| F + 8N | the sections, in order |

Section 0 starts at `align8(F + 8N)`. Section i ≥ 1 starts at `align8(end of section i − 1)`, where `align8(x)` rounds
up to a multiple of 8. The bytes between a section's predecessor's end and its start are zero. The body ends where its
last section ends; no padding follows the last section. A section of length 0 still has its offset. A body with
`N = 0` sections is empty.

This layout has one encoding: the table's offsets follow from the lengths.

A receiver checks, in order:

| Step | Refusal | Rule |
|---|---|---|
| 1 | `BodyTooShort` | The body holds at least `F + 8N` bytes |
| 2, for each section i in order | `SectionOutOfBounds` | `offset + len ≤` the body's length |
| | `SectionOffset` | `offset` equals the expected start |
| | `PaddingNotZero` (body offset) | The padding before the section is zero |
| 3 | `TrailingBytes` | The last section ends at the body's end |

A section holding records of `u` bytes, u32s (`u = 4`) among them, has a length that is a multiple of `u`, refused as
`SectionLength` otherwise. A section holding text is UTF-8, refused as `NotUtf8` at its first invalid byte.

### 4.2 The command header

A command's fixed fields sit in the frame's command header, the 128 bytes at frame offset 128, at the offsets each
command's table gives. Every byte outside those fields is zero, refused as `ReservedNotZero` naming the frame offset
(128 + the command-header offset) of the first nonzero byte. A receiver checks the unused bytes before the fields'
values.

### 4.3 String list

Two sections: `ends`, one u32 per string, the end offset of string i within `text`; then `text`, the strings' UTF-8
bytes back to back. String i is `text[ends[i−1] .. ends[i]]`, with `ends[−1] = 0`. An empty list is two empty sections.

| Step | Refusal | Rule |
|---|---|---|
| 1 | `SectionLength` (the ends' section) | `ends` is a whole number of u32s |
| 2 | `IndexOutOfRange` (the ends' section, the entry) | Each end is at least the one before and at most `text`'s length |
| 3 | `CountMismatch` (`"string text"`) | The last end, or 0 for no strings, equals `text`'s length |
| 4 | `NotUtf8` (the text's section) | `text` is UTF-8 |
| 5 | `NotUtf8` (the text's section, the end) | Every end falls on a character boundary |

A sender interns: each distinct string appears once, at its first use, in the order the body's layout uses them
(section 4.7).

### 4.4 Request term table

Every RDF term a request names (Read, Write, Operation, Prepare, Execute, Drop) is a u32 index into the request's
term table, as is the value an OperationFailed Error reports (section 9.4):
two sections, `records` and `text`.

`records` holds one 24-byte record per term, in table order:

| Record offset | Width | Field | Type | Meaning |
|---|---|---|---|---|
| 0 | 1 | `kind` | u8 | 1 IRI, 2 blank node, 3 literal, 4 language-tagged literal, 5 triple term |
| 1 | 1 | `direction` | u8 | 0 none, 1 left to right (`ltr`), 2 right to left (`rtl`); nonzero only for kind 4 |
| 2 | 6 | reserved | bytes | 0 |
| 8 | 4 | `first` | u32 | kinds 1–4: the first text's offset in `text`; kind 5: the subject's index |
| 12 | 4 | `first_len` | u32 | kinds 1–4: the first text's length; kind 5: the predicate's index |
| 16 | 4 | `second` | u32 | kinds 1–4: the second text's offset, equal to `first + first_len`; kind 5: the object's index |
| 20 | 4 | `second_len` | u32 | kinds 1–4: the second text's length; kind 5: 0 |

| Kind | First text | Second text |
|---|---|---|
| 1 IRI | the IRI | empty |
| 2 blank node | the label, scoped to the request | empty |
| 3 literal | the lexical form | the datatype's IRI; empty for `http://www.w3.org/2001/XMLSchema#string`, which a table never states |
| 4 language-tagged literal | the lexical form | the language tag, never empty |
| 5 triple term | (indices instead) | |

`text` holds every text term's first and then second text, in table order, back to back: the first record's first text
starts at 0 and each text starts where the one before ends. A triple term holds no text and leaves the position
unchanged. A triple term names three entries before its own. A term that is not a triple term has depth 0; a triple
term has depth one more than the deepest of its three parts, at most 32.

The table has one encoding: texts follow each other with no gap, `xsd:string` is never stated, and no entry repeats
another. Two text entries are equal when kind, direction, first and second texts are equal; two triple terms when their
three indices are equal.

A receiver checks, entry by entry in table order:

| Step | Refusal | Rule |
|---|---|---|
| 0 | `SectionLength` (records' section, unit 24) | `records` is a whole number of 24-byte records; checked before the first entry |
| 1 | `FieldValue` (`"term kind"`) | `kind` is 1 to 5 |
| 2 | `FieldValue` (`"direction"`) | `direction ≤ 2`, and nonzero only for kind 4 |
| 3 | `PaddingNotZero` (body offset) | bytes 2 to 7 are zero |
| kind 5: 4 | `IndexOutOfRange` (records' section, the entry, the index, limit = the entry's own index) | subject, predicate and object, in that order, each below the entry's own index |
| kind 5: 5 | `FieldValue` (`"triple term"`) | `second_len` is 0 |
| kind 5: 6 | `TermTooDeep` | the depth is at most 32 |
| kinds 1–4: 4 | `FieldValue` (`"term text offset"`) | `first` equals the end of the texts before |
| kinds 1–4: 5 | `IndexOutOfRange` (text's section, the entry, the end, the text's length) | `first + first_len ≤` the text's length |
| kinds 1–4: 6 | `FieldValue` (`"term text offset"`) | `second = first + first_len` |
| kinds 1–4: 7 | `IndexOutOfRange` (text's section) | `second + second_len ≤` the text's length |
| kinds 1–4: 8 | `BadTerm` (records' section, the entry) | kinds 1, 2: `second_len = 0`; kind 3: the second text is not `http://www.w3.org/2001/XMLSchema#string`; kind 4: `second_len > 0` |
| 9 | `DuplicateTerm` (the entry, the earlier entry) | no earlier entry is equal |

Then, once for the table:

| Step | Refusal | Rule |
|---|---|---|
| 10 | `CountMismatch` (`"term text"`) | the texts end at the text section's end |
| 11 | `NotUtf8` (text's section, first invalid byte) | `text` is UTF-8 |
| 12 | `NotUtf8` (text's section, the boundary) | every text's start and end fall on character boundaries |

The layout's rules above are the receiver's. The server then checks each term's content, and refuses a term breaking
one with Error 7 (Refused) on the request's stream, `detail` the term's index and the message naming the rule:

| Term | Rule |
|---|---|
| IRI | an absolute IRI (RFC 3987); one under a namespace's skolem base, `…/.well-known/genid/`, only when the namespace's dictionary holds it as a blank node's skolem IRI (section 10.2) |
| Language tag | well-formed (BCP 47) |
| Literal of a known datatype (section 10.4) | its lexical form valid for its datatype |
| The predicate slot of a pattern or a change | an IRI |

### 4.5 Quad pattern

A pattern is 32 bytes, four slots of 8 in the order graph, subject, predicate, object. A slot is `kind` u32 then
`value` u32:

| Kind | Slot | Value |
|---|---|---|
| 0 | unbound | 0 |
| 1 | the default graph; at the graph slot only | 0 |
| 2 | bound to a term | the term's index in the body's term table |
| 3 | a statement's parameter; Prepare only | the parameter's number: 0 for the pattern's first parameter slot, 1 for the next |

A receiver checks the section's length, then each slot in order:

| Step | Refusal | Rule |
|---|---|---|
| 0 | `CountMismatch` (`"pattern"`) | the section is 32 bytes |
| 1 | `FieldValue` (`"slot value"`) | kind 0 or 1 has value 0 |
| 2 | `FieldValue` (`"default graph"`, the slot) | kind 1 sits at slot 0 |
| 3 | `IndexOutOfRange` (the pattern's section, the slot, the value, the table's length) | kind 2's value is below the term table's length |
| 4 | `FieldValue` (`"parameter slot"`, the slot) | kind 3 appears only where the body allows parameters |
| 5 | `FieldValue` (`"parameter"`) | kind 3's value equals the number of parameter slots before it |
| 6 | `FieldValue` (`"slot kind"`) | the kind is 0 to 3 |

A pattern has one encoding.

### 4.6 Change records

Write (section 7.2) and Describe (section 8.3) carry changes and declarations as records over an args section. A
record is 16 bytes:

| Record offset | Width | Field | Type |
|---|---|---|---|
| 0 | 1 | `kind` | u8 |
| 1 | 1 | `flags` | u8 |
| 2 | 2 | zero | 0 |
| 4 | 4 | `args_start` | u32: the index of the record's first arg in the args section |
| 8 | 4 | `args_len` | u32: the record's arg count |
| 12 | 4 | `extra` | u32: 0, except a Describe entity type's preorder number |

The args section is u32s. Record i's args follow record i − 1's: `args_start` of record 0 is 0, of record i the sum of
the `args_len` before, and the records consume every arg. A term arg is a term-table index; a name arg is an index into
the body's string list; `ABSENT` is written where the table says so.

| Kind | Record | Flags | Args |
|---|---|---|---|
| 1 | Insert a statement | 0 | graph (term or `ABSENT` for the default graph), subject, predicate, object: 4 terms |
| 2 | Insert an edge with properties | 0 | graph, subject, predicate, object, then a property term and its value term per pair: 4 + 2p |
| 3 | Delete a statement | 0 | graph, subject, predicate, object |
| 4 | Delete a statement by its ID | 0 | graph, the ID's low u32, the ID's high u32 (ID = low + 2³² × high) |
| 5 | Delete every statement of a subject and predicate | 0 | graph, subject, predicate |
| 6 | Set a statement's property | bit 0: by triple | the statement, then the property's predicate term, then its value term |
| 7 | Delete a statement's property | bit 0: by triple | the statement, then the predicate term, then the value term or `ABSENT` for every value |
| 8 | Declare a predicate | bits 0–1: the kind (0 edge, 1 property, 2 either); bit 2: single-valued; bit 3: unique; bit 4: count index; bit 5: vector index | names: the predicate; the datatype or `ABSENT`; the tokenizer count t, then t tokenizer names; the domain count d, then d domain names; with bit 5, the factory name, the option count o, then o pairs of option name and value name |
| 9 | Declare an entity type | bit 0: abstract | names: the type's IRI; the parent's IRI or `ABSENT` |
| 10 | Declare a relation | 0 | names: the relation's IRI, then each role's IRI: 1 + r |

"The statement" of kinds 6 and 7 is, with bit 0 clear, graph, the ID's low u32 and high u32 (3 args); with bit 0 set,
graph, subject, predicate and object (4 terms).

A receiver checks the records' section is a whole number of 16-byte records (`SectionLength`), then each record in
order:

| Step | Refusal | Rule |
|---|---|---|
| 1 | `FieldValue` (`"change kind"`) | `kind` is 1 to 10 in a Write, 8 to 10 in a Describe |
| 2 | `FieldValue` (`"declaration order"`) | Describe only: `kind` is at least the record before's |
| 3 | `FieldValue` (`"flags"`) | only the kind's flag bits are set; for kind 8, bits 0–1 are not 3 |
| 4 | `FieldValue` (`"change record"`) | bytes 2–3 are zero |
| 5 | `FieldValue` (`"extra"`) | `extra` is 0, except for kind 9 in a Describe |
| 6 | `FieldValue` (`"args_start"`) | `args_start` equals the args consumed so far |
| 7 | `CountMismatch` (`"args"`) | `args_start + args_len ≤` the args' count |
| 8 | `CountMismatch` (`"change args"`) | the arg count fits the kind: exactly 4 (kinds 1, 3), 4 + an even number (2), 3 (4, 5), 3 + 2 or 4 + 2 (6, 7, by flag bit 0), 2 (9), at least 1 (10); kind 8 at least 3, each count's names present, and every arg consumed. Kind 8 runs steps 8 and 9 together, arg by arg |
| 9 | `IndexOutOfRange` (the args' section, the arg's index in the section, the value, the limit) | each term arg below the term table's length and each name arg below the string list's length; `ABSENT` where the table allows it. The ID halves of kinds 4, 6 and 7 take any value |

Then `CountMismatch` (`"args"`) unless the records consumed exactly the args section. A Describe body has no term
table, so its limit for term args is 0.

### 4.7 One encoding: what a receiver enforces

| Layout | Enforced by the receiver | Held by the sender's convention only |
|---|---|---|
| Header, section table, padding, command header | every byte | |
| Request term table | the text order, no `xsd:string`, no duplicate entry | the entries' order: Rust builders intern terms in order of first use, a triple's parts before it |
| String list | the ends' order | each string once, in order of first use |
| Write's names, Describe's strings | | interned: Write in record order, arg by arg; Describe's labels, then relationship types, then property keys, then the declarations' names in record order |
| Result batch | the cell count and indices, each term encoding whole | terms in order of first appearance, reading the rows in order and each row's columns in order; each distinct encoding once; strings interned in term order, text then annotation |
| Commit results, Round decisions | the boolean and zero bytes | |

A decoder that re-encodes a received body reproduces it byte for byte when the sender followed these conventions;
every golden frame does.

## 5. Bodies of the connection

### 5.1 Hello (1)

A client opens a connection: the protocols it speaks, its release, the largest frame it takes and its name.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 2 | `protocol_min` | u16 | at least 1, refused as `FieldValue` (`"protocol_min"`) |
| 2 | 2 | `protocol_max` | u16 | at least `protocol_min`, refused as `FieldValue` (`"protocol_max"`) |
| 4 | 4 | `release` | u32 | any |
| 8 | 4 | `max_frame` | u32 | the largest frame the client takes, 524,288 to 1,048,576 (`FieldValue` `"max_frame"`) |

| Section | Contents |
|---|---|
| 0 | the client's name, UTF-8 |

Order: unused bytes 12–127 zero; `protocol_min`; `protocol_max`; `max_frame`; sections; section 0 UTF-8.

The floor of 524,288 bytes holds a Result frame of one term of the largest encoding, a vector of 65,535 dimensions
(262,143 bytes), with room to spare.

### 5.2 Welcome (2)

The server's answer to Hello: the protocol chosen, its limits, and how the connection authenticates.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 2 | `protocol` | u16 | the chosen protocol, at least 1 (`FieldValue` `"protocol"`) |
| 2 | 1 | `auth` | u8 | 1: the connection authenticates before its first request (section 11.1); 0: the server runs without authentication; else `FieldValue` (`"auth"`) |
| 4 | 4 | `release` | u32 | the server's release |
| 8 | 4 | `max_frame` | u32 | the largest frame the server takes, 524,288 to 1,048,576 (`FieldValue` `"max_frame"`) |
| 12 | 4 | `max_streams` | u32 | the streams a connection holds in flight, at least 1 (`FieldValue` `"max_streams"`) |
| 16 | 4 | `initial_credit` | u32 | the Result frames each result stream may send before a Credit |
| 20 | 4 | `max_write_set` | u32 | the largest write-set record a transaction commits, in bytes (section 11.9) |
| 24 | 4 | `transaction_ms` | u32 | the longest a read-write transaction lives, in milliseconds (section 11.9) |

| Section | Contents |
|---|---|
| 0 | the server's name, UTF-8 |
| 1 | the authentication mechanisms the server accepts: string list `ends` (section 4.3) |
| 2 | the mechanisms: string list `text`, each a SASL mechanism name (section 11.1) |

Order: unused bytes 3 and 28–127 zero; `protocol`; `auth`; `max_frame`; `max_streams`; sections; section 0 UTF-8; the
string list (1, 2); with `auth` 1 at least one mechanism, with `auth` 0 none (`CountMismatch` `"mechanisms"`).

### 5.3 Authenticate (18)

One message of a SASL exchange (RFC 4422) on stream 0: the client's first names the mechanism, and each side answers
the other's until the server's final message, or its refusal (section 11.1). A connection authenticates before its
first request and again, on the open connection, before its credential expires.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 4 | `step` | u32 | the message's place in its exchange, from 0: the client's messages even, the server's odd (`FieldValue` `"step"`) |
| 4 | 1 | `final` | u8 | boolean: the server's last message of the exchange; 0 from a client (`FieldValue` `"final"`) |
| 8 | 8 | `principal` | u64 | in a final message, the principal the connection acts for; 0 otherwise (`FieldValue` `"principal"`) |
| 16 | 8 | `expires` | u64 | in a final message, when the credential expires, in microseconds of Unix time, 0 for never; 0 otherwise (`FieldValue` `"expires"`) |

| Section | Contents |
|---|---|
| 0 | the mechanism's name, UTF-8: in the client's message of step 0, one the Welcome listed; empty in every other (`FieldValue` `"mechanism"`) |
| 1 | the mechanism's message, its own bytes |

Order: unused bytes 5–7 and 24–127 zero; `step`; `final`; `principal`; `expires`; sections; section 0 UTF-8 and its
rule.

| Mechanism | Messages |
|---|---|
| `SCRAM-SHA-256` (RFC 5802, RFC 7677) | step 0 the client-first-message, 1 the server-first-message, 2 the client-final-message, 3 the server-final-message, final |
| `OAUTHBEARER` (RFC 7628) | step 0 the client's initial response, holding the token; 1 the server's final message on success; on a refusal, 1 the server's error JSON, naming the issuer's discovery document and the scope to ask for, 2 the client's single byte `0x01`, then the server's Error 19 |

`OAUTHBEARER` is offered only on a connection encrypted with TLS (`SECURITY.md`).

## 6. Bodies of sessions, transactions and drops

### 6.1 Register (3)

A session is registered for the connection's principal, which owns it (section 11.5). The client sends `session` 0;
the server answers with the session's number.

| Command header | Width | Field | Type |
|---|---|---|---|
| 0 | 8 | `session` | u64 |

No sections: the body is empty. Order: unused bytes 8–127 zero; sections.

### 6.2 Begin (4)

A transaction begins on the frame's stream, which becomes the transaction's (section 11.2). The client sends
`read_version` 0; the server answers with the snapshot the transaction reads.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 8 | `namespace` | u64 | the transaction's namespace, which its every request acts in |
| 8 | 8 | `read_version` | u64 | |
| 16 | 1 | `mode` | u8 | 0 read-write, 1 read-only; else `FieldValue` (`"mode"`) |

No sections. Order: unused bytes 17–127 zero; `mode`; sections.

### 6.3 Commit (5)

A transaction commits through a session with the session's next request number, or through no session, `session` 0
(section 11.5). The client sends `commit_version` 0 and no results; the server answers with the commit version and the
result of each commit-time operation the transaction asked for, in the order asked.

| Command header | Width | Field | Type |
|---|---|---|---|
| 0 | 8 | `session` | u64 |
| 8 | 8 | `request` | u64 |
| 16 | 8 | `commit_version` | u64 |

| Section | Contents |
|---|---|
| 0 | results, 16 bytes each; empty in a request |

| Result offset | Width | Field | Type |
|---|---|---|---|
| 0 | 8 | `uid` | u64: the node an insert-if-absent or upsert-and-link found or created; a counter add's new value, an i64 in two's complement; 0 for compare-and-set |
| 8 | 1 | `created` | u8 boolean: whether the operation created the node; 0 for compare-and-set and counter add |
| 9 | 7 | zero | 0 |

Order: unused bytes 24–127 zero; sections; in a request, section 0 empty (`CountMismatch` `"results"`); section 0 a
whole number of 16-byte results (`SectionLength`); each result in order: `created ≤ 1` (`FieldValue` `"created"`),
bytes 9–15 zero (`PaddingNotZero` at the body offset).

### 6.4 Abort (6)

The frame's transaction aborts. The command header is all zero (`ReservedNotZero` otherwise); no sections.

### 6.5 Drop (19)

A drop on a stream of its own, outside any transaction: a predicate, a graph, a namespace's data, a namespace, or every
namespace. It commits as a transaction does, through a session with the session's next request number or through no
session, and is answered as a Commit is: the client sends `commit_version` 0; the server answers with the drop's
commit version. The commit step decides it as the last transaction of its round, and its write-set record names the
drop in section 10 (section 12.2). A transaction that read or wrote what the drop removes, below its version, aborts
with Conflict, kind Drop.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 1 | `kind` | u8 | 1 a predicate, 2 a graph, 3 a namespace's data, 4 a namespace, 5 every namespace; else `FieldValue` (`"kind"`) |
| 8 | 8 | `namespace` | u64 | the namespace of kinds 1 to 4; 0 for kind 5 (`FieldValue` `"namespace"`) |
| 16 | 8 | `session` | u64 | as Commit's |
| 24 | 8 | `request` | u64 | as Commit's |
| 32 | 8 | `commit_version` | u64 | as Commit's |

| Section | Contents |
|---|---|
| 0 | the term table's records (section 4.4): one IRI, the predicate's for kind 1 and the graph's for kind 2; none for kinds 3 to 5 |
| 1 | the term table's text |

Order: unused bytes 1–7 and 40–127 zero; `kind`; `namespace`; sections; the term table; one entry of kind 1 (IRI) for
drop kinds 1 and 2, none for 3 to 5 (`CountMismatch` `"drop terms"`).

A graph drop removes every key of the graph in the namespace. A namespace's data drop removes the namespace's data and
keeps its catalog. A namespace drop removes everything in the namespace and writes its initial catalog. A drop of
every namespace removes every quad key, keeps the dictionary and writes the root namespace's initial catalog. The
privilege each takes is in `SECURITY.md`.

## 7. Bodies of data requests

### 7.1 Read (7)

A data request on a stream of its own, in the transaction it names: a quad pattern, at most `limit` rows (0 for no
limit), and the resume key a previous result of the same request ended with, empty to start (section 11.8). A
transaction's reads run beside each other, each on its stream, all at the transaction's snapshot; cancelling one ends
that read alone (section 11.4).

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 8 | `transaction` | u64 | the stream number of the transaction's Begin, an open transaction of the connection |
| 8 | 4 | `limit` | u32 | |
| 12 | 4 | `flags` | u32 | bit 0: return each row's statement ID (section 9.1); bits 1 to 31 zero (`FieldValue` `"flags"`) |

| Section | Contents |
|---|---|
| 0 | the pattern (section 4.5), no parameter slot |
| 1 | the term table's records (section 4.4) |
| 2 | the term table's text |
| 3 | the resume key, bytes the server gave |

Order: unused bytes 16–127 zero; `flags`; sections; the term table; the pattern.

A Read naming a transaction the connection does not hold open is refused with Error 1 (Protocol) on its stream.

### 7.2 Write (8)

A changeset in the transaction of the frame's stream, in the transaction's namespace. No reply; a refusal arrives as an
Error on the stream and ends the transaction. The command header is all zero.

| Section | Contents |
|---|---|
| 0 | the change records, 16 bytes each, in order (section 4.6) |
| 1 | the args, u32s |
| 2 | the term table's records |
| 3 | the term table's text |
| 4 | the names: string list `ends` (section 4.3) |
| 5 | the names: string list `text` |

Order: command header bytes 0–127 zero; sections; the term table (2, 3); the names (4, 5); the args a whole number of
u32s (`SectionLength`, section 1); the records.

### 7.3 Operation (9)

A commit-time operation in the transaction of the frame's stream, decided by the commit step in commit order. No reply;
its result comes with the Commit reply.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 1 | `kind` | u8 | 1 insert-if-absent, 2 upsert-and-link, 3 compare-and-set, 4 counter add; else `FieldValue` (`"kind"`) |
| 8 | 8 | `delta` | i64 | kind 4's addend; 0 otherwise (`FieldValue` `"delta"`) |
| 16 | 4 | `properties` | u32 | kinds 1 and 2: the properties written when the node is created; 0 otherwise (`FieldValue` `"properties"`) |
| 20 | 4 | `links` | u32 | kind 2: the links; 0 otherwise (`FieldValue` `"links"`) |

| Section | Contents |
|---|---|
| 0 | the args, u32s |
| 1 | the term table's records |
| 2 | the term table's text |

| Kind | Args | Count |
|---|---|---|
| 1 insert-if-absent | graph (term or `ABSENT`), the unique predicate, the value, then per property its predicate and value | 3 + 2 × `properties` |
| 2 upsert-and-link | as kind 1, then per link its predicate, the other node, and the direction (0: the node is the edge's subject, 1: its object) | 3 + 2 × `properties` + 3 × `links` |
| 3 compare-and-set | graph, subject, predicate, the expected value, the new value | 5 |
| 4 counter add | graph, subject, predicate | 3 |

Insert-if-absent resolves the node that holds `value` for the unique `predicate` in `graph`, creating it with the
properties when none does. Upsert-and-link does the same and writes the links between that node and the others in the
same step. Compare-and-set replaces `subject`'s single value of `predicate` with `new` when it is `expected`; else the
commit is refused with `OperationFailed` and the value found. Counter add adds `delta` to `subject`'s integer counter
`predicate` without reading it; its result carries the counter's new value, computed by the commit step in commit
order.

Order: unused bytes 1–7 and 24–127 zero; `kind`; `delta`; `properties`; `links`; sections; the term table; the args a
whole number of u32s (`SectionLength`); the count (`CountMismatch` `"args"`); each arg in order: a link's direction is 0
or 1 (`FieldValue` `"link direction"`), every other arg is below the term table's length (`IndexOutOfRange`, section
0), arg 0 may be `ABSENT`.

## 8. Bodies of prepared statements and the dictionary

### 8.1 Prepare (10)

A statement prepared once per connection, which holds it (section 11.7): a quad pattern with parameter slots. The
client sends the pattern and its parameters' types with `statement` and `schema` 0 and no result types; the server
answers with the statement's id, its namespace's schema id as the connection's principal sees it (section 11.7), and
the result column types.

| Command header | Width | Field | Type |
|---|---|---|---|
| 0 | 8 | `namespace` | u64 |
| 8 | 8 | `statement` | u64 |
| 16 | 8 | `schema` | u64 |

| Section | Contents |
|---|---|
| 0 | the pattern, parameter slots allowed |
| 1 | the term table's records |
| 2 | the term table's text |
| 3 | parameter types, one column-type byte per parameter, in parameter order |
| 4 | result column types, one byte per unbound slot in slot order; empty in a request |

A column type is 1 (a node: an IRI or a blank node) or 2 (any term). The graph, subject and predicate slots take nodes;
the object slot takes any term. The server answers each unbound slot's result type by its position: 1 at graph,
subject and predicate, 2 at object.

Order: unused bytes 24–127 zero; sections; the term table; the pattern; section 3's bytes, then section 4's, each 1 or 2
(`FieldValue` `"column type"`); parameter types count the pattern's parameter slots (`CountMismatch` `"parameter
types"`); in a request, result types are empty, and in an answer they count the unbound slots (`CountMismatch` `"result
types"`).

### 8.2 Execute (11)

A prepared statement executed on a stream of its own, in the transaction it names, as a Read is (section 7.1): its
parameters bound by position, at most `limit` rows (0 for no limit), from the resume key a previous result of the same
request ended with.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 8 | `statement` | u64 | |
| 8 | 8 | `transaction` | u64 | the stream number of the transaction's Begin, an open transaction of the connection |
| 16 | 4 | `limit` | u32 | |
| 20 | 4 | `flags` | u32 | bit 0: return each row's statement ID (section 9.1); bits 1 to 31 zero (`FieldValue` `"flags"`) |

| Section | Contents |
|---|---|
| 0 | the term table's records |
| 1 | the term table's text |
| 2 | the parameters, u32 term indices, parameter 0 first |
| 3 | the resume key |

Order: unused bytes 24–127 zero; `flags`; sections; the term table (0, 1); the parameters a whole number of u32s
(`SectionLength`); each below the term table's length (`IndexOutOfRange`, section 2).

### 8.3 Describe (12)

What a namespace holds, the data dictionary's answer, as far as the connection's principal may read it. The client
sends the namespace alone: `read_version` 0 and seven empty sections. The server answers with the snapshot it read, the
labels, relationship types and property keys, and the catalog's declarations the principal may read.

| Command header | Width | Field | Type |
|---|---|---|---|
| 0 | 8 | `namespace` | u64 |
| 8 | 8 | `read_version` | u64 |

| Section | Contents |
|---|---|
| 0 | strings: string list `ends` |
| 1 | strings: string list `text`, each IRI in plain text |
| 2 | labels, u32 string indices, in order |
| 3 | relationship types, u32 string indices |
| 4 | property keys, u32 string indices |
| 5 | declarations: change records of kinds 8, 9, 10, in that order, each kind's records in the order of their UIDs (section 4.6); an entity type's `extra` is its preorder number: its place, from 1, in a preorder walk of the type hierarchy, each type before its children, roots and siblings in the order of their UIDs |
| 6 | the declarations' args, u32s |

Order: unused bytes 16–127 zero; sections; the string list (0, 1); sections 2, 3, 4 in turn, each a whole number of
u32s (`SectionLength`) and each index below the string count (`IndexOutOfRange`); the args (6) a whole number of u32s;
the records (5).

The request's body, seven empty sections, is 56 bytes: the table, every `offset` 56 and every `len` 0.

## 9. Bodies of results and control

### 9.1 Result (13)

A columnar, dictionary-encoded batch of a Read or Execute: per column a u32 cell per row, an index into the batch's
term table or `ABSENT` for unbound; the term table holding the batch's terms in the encoding of section 10, with the
texts those encodings name but do not hold; and, when the request asked, each row's statement ID. A row is one
statement the pattern matched; the columns are the pattern's unbound slots, in slot order. Rows come in the store's key
order. The stream's last batch carries the header flag `LAST`; its resume key, when not empty, continues a limited
read. A read that matches nothing answers one batch flagged `LAST`, `rows` 0 and `columns` the unbound slots.

| Command header | Width | Field | Type |
|---|---|---|---|
| 0 | 4 | `columns` | u32 |
| 4 | 4 | `rows` | u32 |

| Section | Contents |
|---|---|
| 0 | the cells, u32s, column by column: the cell of column c and row r at index `c × rows + r`; `columns × rows` cells |
| 1 | the term offsets, u32s, one per term: term i starts at `offsets[i]` within section 2, the first at 0 |
| 2 | the terms' encodings, back to back; term i ends where term i + 1 starts, the last at the section's end |
| 3 | the term texts: per term two u32s, `text` then `annotation`, each a string index or `ABSENT` |
| 4 | strings: string list `ends` |
| 5 | strings: string list `text` |
| 6 | the resume key |
| 7 | the statement IDs, u64s, one per row in row order; empty unless the request set its `flags` bit 0 |

A term's `text` is what its encoding names by UID or by hash: an IRI's text, or a blank node's skolem IRI, for a UID
encoding; the lexical form of a literal held by hash. Its `annotation` is a language tag held by hash, or the IRI of a
datatype held by UID (section 10.3). A statement ID names the statement in Write's change records 4, 6 and 7, whose
args are its low and high u32 halves (section 4.6).

Order: unused bytes 8–127 zero; sections; offsets (1) a whole number of u32s; texts (3) a whole number of 8-byte pairs
(`SectionLength`); one pair per term (`CountMismatch` `"term texts"`); the string list (4, 5); cells (0) a whole number
of u32s; `columns × rows` cells, the product computed in 64 bits (`CountMismatch` `"cells"`); each cell `ABSENT` or
below the term count (`IndexOutOfRange`, section 0); with no terms, section 2 is empty (`CountMismatch` `"term
encodings"`); each term in order: offset 0 for the first, its bytes from its offset to the next term's offset (or the
section's end) hold exactly one whole encoding of section 10.1 (`BadTerm`, section 2, the term); each text index
`ABSENT` or below the string count (`IndexOutOfRange`, section 3); the statement IDs (7) empty or 8 bytes per row
(`CountMismatch` `"statement ids"`).

### 9.2 Credit (14)

More credit for the frame's result stream: `frames` more Result frames may be sent.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 4 | `frames` | u32 | at least 1 (`FieldValue` `"frames"`) |

No sections. Order: unused bytes 4–127 zero; `frames`; sections.

### 9.3 Cancel (15)

The frame's stream is cancelled. The command header is all zero; no sections.

### 9.4 Error (16)

A request refused on the frame's stream; or, on stream 0, the connection's last frame after a connection error, a
refused handshake or authentication, or an expired credential (sections 2.6, 11.1).

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 2 | `code` | u16 | an error code of the table below (`FieldValue` `"code"`) |
| 8 | 8 | `detail` | u64 | as the code defines; 0 where the table gives none |

| Section | Contents |
|---|---|
| 0 | the message, UTF-8, for people |
| 1 | for `OperationFailed`, the value found: a request term table's records (section 4.4), one entry; empty otherwise |
| 2 | that term table's text |

Order: unused bytes 2–7 and 16–127 zero; `code`; sections; the term table (1, 2), at most one entry
(`CountMismatch` `"value"`); section 0 UTF-8. An empty term table means no value: the predicate held none.

| Number | Code | Retryable | `detail` | Meaning |
|---|---|---|---|---|
| 1 | Protocol | no | 0 | A frame broke a validation rule: a connection error, after which the server closes the connection, or a stream error, which ends the request's stream (section 2.6). The message is the rule's text |
| 2 | Conflict | yes | the conflict kind below | A transaction's read conflicted with a commit ordered before it |
| 3 | TooOld | yes | 0 | The read version is below what validation still holds |
| 4 | PastLimit | yes | the limit in milliseconds | The transaction lived past its limit, the Welcome's `transaction_ms` |
| 5 | Busy | yes | 0 | The commit queue is full |
| 6 | TooLarge | no | the limit in bytes | A write set exceeds the Welcome's `max_write_set` |
| 7 | Refused | no | 0; for a term's rule, the term's index | The catalog refused a write, or a term broke the server's rules (section 4.4) |
| 8 | OperationFailed | no | 0 | A compare-and-set did not hold; sections 1 and 2 hold the value found |
| 9 | StaleStatement | no | the statement | The statement's schema id no longer matches its namespace's |
| 10 | UnknownStatement | no | the statement | The connection holds no statement of this id: never prepared, or evicted |
| 11 | Parameters | no | the statement; 0 for a parameter type | The parameters do not fit the statement |
| 12 | SessionEvicted | no | the session | The session table evicted the session; a retried commit refused so has an unknown outcome (section 11.5) |
| 13 | RequestOutOfOrder | no | the request number expected | A commit's request number is not one above the session's last |
| 14 | SessionBusy | yes, after a delay | the session | The session has a commit in flight |
| 15 | StreamLimit | no | the limit | The connection holds as many streams as Welcome allows |
| 16 | ProtocolVersion | no | the server's highest protocol | No protocol both sides speak; the server closes the connection |
| 17 | Unavailable | no | 0 | The node refuses writes after a failed log sync, until it restarts |
| 18 | NotCommitted | yes | the request | A retried commit the session table does not hold, with no transaction open: the request was not committed, and the client runs its transaction again (section 11.5) |
| 19 | AuthenticationFailed | no | 0 | The authentication exchange failed; the server closes the connection. The message says nothing of what failed |
| 20 | Unauthenticated | no | 0 | A request before the connection authenticated, or an authentication not complete within its timeout; the server closes the connection |
| 21 | PermissionDenied | no | the action below | The connection's principal may not do what the request asks; the request is refused, and a transaction's request ends the transaction |
| 22 | CredentialExpired | no | 0 | The connection's credential expired before a new authentication; the server closes the connection |

A client retries a request refused with a retryable code. Code numbers are never reused.

| Action (`detail` of code 21) | Asked for |
|---|---|
| 1 | `read` |
| 2 | `write` |
| 3 | `declare` |
| 4 | `drop` |
| 5 | `admin` |
| 6 | `operate` |

| Conflict kind (`detail` of code 2) | Read |
|---|---|
| 1 | List: a list read whole |
| 2 | Statement: a statement tested |
| 3 | Range: a key range scanned |
| 4 | AbsentTerm: a dictionary key found absent |
| 5 | Catalog: the namespace's catalog |
| 6 | Drop: a range a drop wrote |

## 10. The term encoding of results

A Result batch holds each term in the store's own encoding, a tag byte and its payload, with one difference: the store
keeps an IRI and a blank node under one tag, `0x01`, and a Result tells them apart, `0x02` for a blank node. The tag
fixes the encoding's length, or where to read it. A vector's encoding is `3 + 4 × dimension` bytes; every other
encoding is at most 33.

### 10.1 Tags and lengths

"Inline text" is a length byte `n` (0 to 15) then `n` bytes of UTF-8. "Hash" is 16 bytes of BLAKE2b with a 16-byte
digest, unkeyed, of the text's UTF-8 bytes: the result batch's string list supplies the text it names. "UID" is a u64
**big-endian** naming an IRI, a blank node or a datatype in the store's dictionary.

| Tag | Term | Payload after the tag | Length |
|---|---|---|---|
| `0x01` | IRI | UID | 9 |
| `0x02` | blank node | UID | 9 |
| `0x10` | `xsd:boolean` by value | 1 byte | 2 |
| `0x11` | `xsd:integer` by value | 8 bytes | 9 |
| `0x12` | `xsd:decimal` by value | 16 bytes | 17 |
| `0x13` | `xsd:double` by value | 8 bytes | 9 |
| `0x14` | `xsd:float` by value | 4 bytes | 5 |
| `0x15` | `xsd:dateTime` by value | 18 bytes | 19 |
| `0x16` | `xsd:time` by value | 18 bytes | 19 |
| `0x17` | `xsd:date` by value | 18 bytes | 19 |
| `0x18` | `xsd:gYearMonth` by value | 18 bytes | 19 |
| `0x19` | `xsd:gYear` by value | 18 bytes | 19 |
| `0x1A` | `xsd:gMonthDay` by value | 18 bytes | 19 |
| `0x1B` | `xsd:gDay` by value | 18 bytes | 19 |
| `0x1C` | `xsd:gMonth` by value | 18 bytes | 19 |
| `0x1D` | `xsd:duration` by value | 24 bytes | 25 |
| `0x1E` | `xsd:yearMonthDuration` by value | 8 bytes | 9 |
| `0x1F` | `xsd:dayTimeDuration` by value | 16 bytes | 17 |
| `0x20` | `xsd:string`, lexical form ≤ 15 bytes | inline text | 2 + n |
| `0x21` | `xsd:string`, longer | hash of the lexical form | 17 |
| `0x22`, `0x25`, `0x28` | language-tagged string, lexical form ≤ 15 bytes, tag ≤ 15 bytes | inline lexical form, then inline tag | 3 + n + m |
| `0x23`, `0x26`, `0x29` | language-tagged string, lexical form > 15 bytes, tag ≤ 15 bytes | hash of the lexical form, then inline tag | 18 + m |
| `0x24`, `0x27`, `0x2A` | language-tagged string, tag > 15 bytes | hash of the lexical form, then hash of the tag | 33 |
| `0x30` | literal of a known datatype (section 10.4), lexical form ≤ 15 bytes | datatype code byte, then inline lexical form | 3 + n |
| `0x31` | literal of a known datatype, longer | datatype code byte, then hash of the lexical form | 18 |
| `0x32` | literal of another datatype, lexical form ≤ 15 bytes | the datatype's UID, then inline lexical form | 10 + n |
| `0x33` | literal of another datatype, longer | the datatype's UID, then hash of the lexical form | 25 |
| `0x40` | `https://w3id.org/ultragraphdb/ns/float32vector` | dimension u16 **little-endian**, 1 to 65,535; then `dimension` f32s **little-endian** | 3 + 4 × dimension |

The three tags of each language-tagged row are for the base direction: the first none, the second `ltr`, the third
`rtl`. A language tag over 15 bytes hashes the lexical form too, whatever its length.

An encoding is whole when its tag is listed, each inline length is at most 15 and its text UTF-8, a boolean's byte 0
or 1, a known datatype's code in section 10.4, a vector's dimension at least 1, and the bytes hold exactly the length
the table gives. A Result receiver checks this per term (section 9.1).

### 10.2 How the server chooses the encoding

| Term | Encoding |
|---|---|
| IRI | `0x01`, with the IRI's text as the term's `text` |
| Blank node | `0x02`, with its skolem IRI, `…/namespace/{n}/.well-known/genid/{uid}` (RDF 1.1 §3.5), as the term's `text`; a request names the node again by that IRI, as an IRI (section 4.4) |
| `xsd:string` literal | `0x20` up to 15 bytes, else `0x21` |
| Language-tagged literal | by tag length, then lexical form length, as the table gives |
| Literal of datatypes `0x10` to `0x1F` whose lexical form is the canonical form of its value (section 10.5) and whose value the payload holds exactly: an integer within i64, a decimal of at most 18 fraction digits within the i128 payload, a date, time or duration within its payload, and no timezone offset of −00:01, whose minutes would read as the payload's "no timezone" | the by-value tag |
| `float32vector` literal in canonical form: `[`, the elements separated by `, `, `]`, each element the shortest decimal that reads back as the same finite f32, without exponent | `0x40` |
| Any other literal of a known datatype, a by-value datatype's non-canonical form or a value its payload does not hold included | `0x30` up to 15 bytes, else `0x31` |
| A literal of any other datatype | `0x32` up to 15 bytes, else `0x33` |

Equal terms encode equally and unequal terms unequally.

### 10.3 Texts and annotations

| Tag | `text` | `annotation` |
|---|---|---|
| `0x01` | the IRI | |
| `0x02` | the blank node's skolem IRI | |
| `0x21`, `0x23`, `0x26`, `0x29`, `0x31` | the lexical form | |
| `0x24`, `0x27`, `0x2A` | the lexical form | the language tag |
| `0x32` | | the datatype's IRI |
| `0x33` | the lexical form | the datatype's IRI |
| every other tag | | |

### 10.4 Known datatypes

A known datatype is named by a one-byte code in tags `0x30` and `0x31`; codes 1 to 16 are also the by-value datatypes.
`xsd:` is `http://www.w3.org/2001/XMLSchema#`, `geo:` is `http://www.opengis.net/ont/geosparql#`, `ugdb:` is
`https://w3id.org/ultragraphdb/ns/`.

| Code | Datatype | Code | Datatype | Code | Datatype |
|---|---|---|---|---|---|
| 1 | `xsd:boolean` | 12 | `xsd:gDay` | 23 | `xsd:nonPositiveInteger` |
| 2 | `xsd:integer` | 13 | `xsd:gMonth` | 24 | `xsd:negativeInteger` |
| 3 | `xsd:decimal` | 14 | `xsd:duration` | 25 | `xsd:unsignedLong` |
| 4 | `xsd:double` | 15 | `xsd:yearMonthDuration` | 26 | `xsd:unsignedInt` |
| 5 | `xsd:float` | 16 | `xsd:dayTimeDuration` | 27 | `xsd:unsignedShort` |
| 6 | `xsd:dateTime` | 17 | `xsd:long` | 28 | `xsd:unsignedByte` |
| 7 | `xsd:time` | 18 | `xsd:int` | 29 | `xsd:dateTimeStamp` |
| 8 | `xsd:date` | 19 | `xsd:short` | 30 | `geo:geoJSONLiteral` |
| 9 | `xsd:gYearMonth` | 20 | `xsd:byte` | 31 | `geo:wktLiteral` |
| 10 | `xsd:gYear` | 21 | `xsd:nonNegativeInteger` | 32 | `ugdb:float32vector` |
| 11 | `xsd:gMonthDay` | 22 | `xsd:positiveInteger` | 33 | `xsd:anyURI` |

### 10.5 By-value payloads

Each multi-byte number in a by-value payload is **big-endian**. A decimal number `x` is stored as the i128
`x × 10¹⁸`, exactly: 18 fraction digits.

| Tag | Payload | Value |
|---|---|---|
| `0x10` | u8 | 0 false, 1 true; any other byte is no whole encoding |
| `0x11` | i64 | the integer |
| `0x12` | i128 | the decimal × 10¹⁸ |
| `0x13` | f64, IEEE 754 binary64 | the double |
| `0x14` | f32, IEEE 754 binary32 | the float |
| `0x15`–`0x1C` | i128 `T`, then i16 `Z` | `T`: the value's `timeOnTimeline` in seconds × 10¹⁸ (below); `Z`: the timezone offset in minutes, or `0xFFFF` for none |
| `0x1D` | i64 months, then i128 seconds × 10¹⁸ | the duration's year-month part and day-time part |
| `0x1E` | i64 | months |
| `0x1F` | i128 | seconds × 10¹⁸ |

`T` is `timeOnTimeline` of XML Schema 1.1 Part 2 (§E.3.4), over the value's seven properties, with the timezone offset
`tz` in minutes (0 when absent), `div` and `mod` rounding toward negative infinity, and `daysInMonth(y, m)` the
Gregorian month length:

```
yr = year − 1, or 1971 when the type has no year
mo = month, or 12 when the type has no month
da = day − 1, or daysInMonth(yr + 1, mo) − 1 when the type has no day
hr = hour, or 0;   mi = minute − tz, minute 0 when absent;   se = second, or 0
T  = 31536000 × yr + 86400 × (yr div 400 − yr div 100 + yr div 4)
   + 86400 × Σ(m = 1 … mo − 1) daysInMonth(yr + 1, m) + 86400 × da + 3600 × hr + 60 × mi + se
```

| Type | Properties it has |
|---|---|
| `dateTime` | year, month, day, hour, minute, second |
| `time` | hour, minute, second (hour 24 reads as 0) |
| `date` | year, month, day |
| `gYearMonth` | year, month |
| `gYear` | year |
| `gMonthDay` | month, day |
| `gDay` | day |
| `gMonth` | month |

`T = 0` is 0001-01-01T00:00:00 in UTC. To read the value back, a client takes `L = ⌊T / 10¹⁸⌋ + 60 × tz`, the local
time in whole seconds after 0001-01-01T00:00:00 in the proleptic Gregorian calendar with astronomical year numbering
(year 0 exists and is a leap year). The local date is day `L div 86400` counted from 0001-01-01 as day 0; the hour is
`(L mod 86400) div 3600` and the minute `(L mod 3600) div 60`; the second is `(T / 10¹⁸) mod 60`, with its fraction. A
client reads the properties its type has and ignores the others.

A literal is encoded by value only when its lexical form equals the form below, so a client writes that form back.
The forms are the protocol's own, independent of any library's formatting, and golden vectors hold them:

| Datatype | Canonical lexical form |
|---|---|
| boolean | `true`, `false` |
| integer | decimal digits, `-` for negatives, no `+`, no leading zero |
| decimal | the integer part, then `.` and the fraction's digits without trailing zeros when the fraction is nonzero; `-` for negatives; `0` for zero |
| double, float | `INF`, `-INF`, `NaN`, else the shortest decimal that reads back as the same binary64 (binary32) value, without exponent: `1`, `0.5`, `-0`, `100000000000000000000` |
| dateTime | `[-]YYYY-MM-DDThh:mm:ss[.f][tz]`: the year at least 4 digits, the second two digits before its point and its fraction as decimal's |
| time | `hh:mm:ss[.f][tz]` |
| date, gYearMonth, gYear | `[-]YYYY-MM-DD[tz]`, `[-]YYYY-MM[tz]`, `[-]YYYY[tz]` |
| gMonthDay, gMonth, gDay | `--MM-DD[tz]`, `--MM[tz]`, `---DD[tz]` |
| timezone `[tz]` | `Z` for offset 0, else `±hh:mm` |
| duration and its two subtypes | `-` when negative, then `P`; from the months, `yY` and `mM`, each only when nonzero; from the seconds, `dD` when the days are nonzero, then `T` followed by `hH`, `mM` and `sS` for each nonzero part, `s` in decimal's form. A zero duration or dayTimeDuration is `PT0S`, a zero yearMonthDuration `P0M` |

## 11. The connection

### 11.1 Handshake and authentication

1. The client opens a TCP connection, encrypted with TLS from its first byte when the server is configured for TLS
   (`SECURITY.md`), and sends Hello on stream 0: header `sequence` 1, `cluster` the cluster it is configured for,
   `release` its release, `protocol` 1, every other field 0; command header the protocols it speaks and its
   `max_frame`.
2. The server validates the frame. A frame breaking a rule, or a first frame of another command, is answered with an
   Error of code 1 (Protocol) on stream 0, and the server closes the connection.
3. When `protocol_min` exceeds the server's highest protocol, the server answers an Error of code 16 (ProtocolVersion),
   `detail` the server's highest protocol, on stream 0, and closes the connection.
4. Otherwise the server answers Welcome on stream 0, its `protocol` the lesser of its highest protocol and
   `protocol_max`. A server speaks every protocol from 1 to its highest. The Welcome's header carries `protocol` 1 and
   the server's `release`; its command header the server's limits and `auth`, and its sections the mechanisms.
5. The client validates the Welcome and refuses a `protocol` outside its Hello's range (`FieldValue` `"protocol"`),
   closing the connection.
6. With `auth` 1, the client authenticates: an Authenticate exchange on stream 0 (section 5.3) by a mechanism the
   Welcome listed. The server's final Authenticate names the principal the connection acts for and when its credential
   expires. A refusal is an Error of code 19 (AuthenticationFailed) on stream 0, and the server closes the connection.
   A request on any stream before the final Authenticate, or an exchange not complete within the authentication
   timeout (section 11.9), is refused with an Error of code 20 (Unauthenticated) on stream 0, and the server closes
   the connection. With `auth` 0, nothing is exchanged, and the connection acts for
   the anonymous principal, which a server offers only on its loopback interface (`SECURITY.md`).

Every later frame carries in its header the cluster, the sender's `release`, `protocol` equal to the Welcome's
`protocol`, its stream and its sequence. No side sends a frame larger than the other's `max_frame`: the client's from
its Hello, the server's from its Welcome.

**Re-authentication.** A credential that expires, as a token does, is renewed on the open connection: before the
expiry the final Authenticate named, the client runs a new exchange on stream 0, by any mechanism the Welcome listed,
while requests on the other streams go on. The exchange must end as the same principal; another is refused with code
19, and the connection closes. The new final Authenticate's expiry replaces the old. A credential that expires before
a new exchange ends, or that its principal loses (a password changed, the principal disabled, the mechanism removed),
ends the connection: an Error of code 22 (CredentialExpired) on stream 0, then the close.

### 11.2 Streams

- Stream 0 carries the connection's own frames: Hello, Welcome, Authenticate, and the server's Error that ends the
  connection.
- Every request opens a new stream. The client numbers streams from 1 on each connection, upward, and never reuses a
  number on that connection. At most `max_streams` streams are open at once; the server refuses another with Error code
  15 (StreamLimit), `detail` the limit. A request on stream 0 other than Hello and Authenticate, or a new request on a
  stream already open, is a connection error (section 2.6).
- The server answers streams independently, in whatever order their results are ready.
- A stream ends with its frame flagged `LAST`, when the client cancels it, or with an Error on it. Every reply that
  ends its stream carries `LAST`: Register, Commit, Prepare, Describe and Drop replies, a read's last Result, and every
  Error. Begin's reply does not: the transaction's stream stays open.
- Each side numbers the frames it sends on the connection, its `sequence`, from 1 and one more for each frame. A
  receiver refuses a frame whose sequence is not one above the previous frame's from the same side: a connection
  error (`SequenceOrder`), which a lost, repeated or reordered frame cannot pass. A frame of a stream that has ended is
  late; the Rust client drops it.

| Request | Stream | Reply |
|---|---|---|
| Authenticate | stream 0 | Authenticate, until the server's final one; or Error 19, closing the connection |
| Register | a new stream | Register with the session number, or Error; the stream ends |
| Begin | a new stream, which becomes the transaction's | Begin with `read_version` filled; the stream stays open |
| Read, Execute | a new stream, naming the transaction | Result frames, the last flagged `LAST`; or Error |
| Write, Operation | the transaction's stream | none; a refusal is an Error on the stream, which ends the transaction and answers its next request |
| Commit | the transaction's stream; a retry goes on a new stream of its own | Commit or Error; the stream ends |
| Abort | the transaction's stream | none; the stream ends |
| Prepare, Describe, Drop | a new stream | Prepare, Describe or Drop, or Error; the stream ends |
| Credit | the read's stream | none |
| Cancel | a read's stream, or the transaction's | none |

A transaction is its stream and its reads. Its stream takes one request at a time; its reads run beside it, each on a
stream of its own, at the transaction's snapshot. When the transaction ends, by its Commit's reply, its Abort, or an
Error on its stream, the reads still open end with it, as a Cancel ends them (section 11.4): a client that wants a
read's every row reads it to its `LAST` first, as SQL closes a transaction's cursors when it ends. The Rust client
sends no Write, Operation or Commit in a read-only transaction (Begin `mode` 1).

### 11.3 Credit

Each read's stream starts with the Welcome's `initial_credit`. The server sends a Result frame on a stream only while
the stream's credit is above 0, and each Result frame it sends spends 1. A Credit frame on the stream adds `frames`
(saturating at 2³² − 1). Only Result frames spend credit; an Error and every other frame leave it. The server ignores
Credit for a stream that is not open. The Rust client grants more credit when its count reaches 0 and it waits for a
batch.

### 11.4 Cancel and disconnection

- A Cancel frame on a read's stream drops the frames the server queued for it, and the server sends nothing further on
  it. Credit for it afterwards is ignored. The read's transaction goes on.
- A Cancel on a transaction's stream aborts the transaction and ends its reads.
- A Cancel never undoes a commit the commit step decided: a Commit or Drop whose stream is cancelled may still commit,
  and the client learns its outcome by retrying it through its session (section 11.5).
- A connection that closes, from either side, aborts its open transactions. Its commits already in the commit step
  proceed, and its sessions, which belong to its principal, carry over to the principal's next connection.
- The server keeps TCP keepalive on, and closes a connection that sends no frame for its idle limit (section 11.9).
  The protocol has no ping command.

### 11.5 Sessions and exactly-once commits

- A client registers a session with Register (`session` 0) and receives its number. The session belongs to the
  connection's principal. A connection holds as many sessions as it needs concurrent writers; reads need no session.
- Each Commit and Drop carries its session and a request number one above the session's last committed request; a new
  session's first request is 1. The server refuses another number with code 13 (RequestOutOfOrder), `detail` the
  number expected.
- One commit is in flight per session; the server refuses a second with code 14 (SessionBusy), which a client retries
  after a delay.
- Only the session's principal commits through it or reads its stored reply; another principal's request is refused
  with code 21 (PermissionDenied), `detail` 2, `write`.
- The server's session table holds each session's last committed request and its whole reply: the commit version and
  every commit-time operation's result. A retry of that request, with the same session and request number, returns the
  stored reply byte for byte, results included; so a client whose connection dropped after sending a Commit sends it
  again on the next connection, on a new stream, without a transaction, and is answered from the table if it committed.
  A retry of a request the table does not hold, with no transaction open for it, is answered with code 18
  (NotCommitted): the request was not committed, and the client runs its transaction again. A retry while the original
  is still in the commit step is answered with code 14.
- The table is durable: a session's registration and each committed request with its reply are written in the same
  round as the commit, so the table survives a restart of the server exactly as the commit does.
- The table is bounded; an evicted session's request is refused with code 12 (SessionEvicted). A retry refused with
  code 12 has an unknown outcome: the client reports it as unknown and does not retry.
- After a Commit reply the session's next request number is one higher; after a refusal it stays.
- A commit through no session, `session` 0, commits without exactly-once: a lost reply leaves its outcome unknown.
- A Register whose reply is lost leaves a session no client uses, which ages out of the bounded table; the client
  registers again.

### 11.6 Commit-time operations

A transaction sends any number of Operation frames before its Commit. The Commit reply's results hold one 16-byte
result per operation, in the order the operations were sent: the node's UID and whether the operation created it for
kinds 1 and 2; the counter's new value for kind 4; `uid` 0 and `created` 0 for kind 3. A compare-and-set that does not
hold refuses the commit with code 8 (OperationFailed), the value found in the Error's term table.

### 11.7 Prepared statements and the schema id

- A statement is prepared on a connection, which holds it; its id starts at 1 and is never reused on the connection.
  The connection's cache holds a bounded number of statements and evicts the oldest; executing an evicted statement is
  refused with code 10 (UnknownStatement).
- Preparing refuses a parameter declared `2` (any term) at a graph, subject or predicate slot with code 11
  (Parameters), `detail` 0.
- Execute refuses, in this order: an unknown statement (code 10); a statement prepared against another schema id than
  its namespace's current one (code 9, StaleStatement), after which preparing it again succeeds; parameters that are not
  one per parameter slot (code 11); a literal bound at a slot of column type 1 (code 11). Each `detail` is the statement
  id.
- The schema id is XXH3-64, seed 0, of the Describe body (the data before any chunk table) that holds the namespace's
  declarations alone, as the connection's principal sees them: sections 2, 3 and 4 empty, the strings only the
  declarations' names, interned in record order, arg by arg, and the declarations in Describe's order (section 8.3).
  It changes with the catalog and with what the principal may read, and not with the data. For the golden
  `describe_reply.frame` it is `0xFE70341D112B65A9`.

### 11.8 Resume keys

A Result batch's resume key continues its own request: the same Read or Execute, in the same transaction, with the
same pattern, parameters and flags, sent on a new stream with the key in its resume section. Rows come in the store's
key order, so a resumed read neither repeats nor skips a row at the transaction's snapshot. The server refuses a key
that lies outside the pattern's keys with Error 1 (Protocol) on the request's stream.

### 11.9 Limits

The Welcome states the limits a client must know before it sends: `max_frame`, `max_streams`, `initial_credit`,
`max_write_set` and `transaction_ms`. A transaction is bounded by both: its write-set record within `max_write_set`,
which one Round frame holds, refused with code 6 (TooLarge) otherwise; its life within `transaction_ms`, refused with
code 4 (PastLimit) otherwise. A load larger than one transaction is many transactions.

| Limit | Default |
|---|---|
| `max_frame` | 1,048,576 bytes |
| `max_write_set` | 1,040,384 bytes |
| `transaction_ms`, a read-write transaction's life | 10,000 ms |
| `max_streams` | 256 |
| `initial_credit` | 4 |
| Sessions in the table | 4,096 |
| Statements per connection | 256 |
| Idle limit, a connection sending no frame | 600 s |
| Authentication timeout, from Welcome to the final Authenticate | 60 s |
| Refusal interval after 100 consecutive failed authentications of a principal | 15 minutes |

### 11.10 Authorization

Every request is authorized against the connection's principal (`SECURITY.md`). A statement the principal may not read
is absent from a read: a pattern naming a predicate or graph it may not read matches nothing, and one leaving it
unbound returns no row of it. A write, declaration, drop, administration or cluster operation the principal may not do
is refused with code 21 (PermissionDenied), `detail` the action (section 9.4).

## 12. Internal to the server and the log

No client sends or receives these. The log stores each round as a Round frame, header and body, and stage 10's
replication will carry the same bytes. A Round frame's header holds `sequence` its round's `position`, `stream` 0,
`flags` 0, `protocol` 1, `view` 0, `replica` 0 and `release` the release of the node that wrote it.

### 12.1 Round (17)

The log record of a round: each transaction's decision in round order and the write-set record of each committed one.
Committed transaction k of the round (counting committed ones from 0) has commit version `first_version + 2k`.

| Command header | Width | Field | Type |
|---|---|---|---|
| 0 | 8 | `position` | u64: the round's place in the log, gapless from 1 |
| 8 | 8 | `parent` | u64: the previous round's frame `checksum` |
| 16 | 8 | `incarnation` | u64: the log's incarnation, raised each time a log is created afresh |
| 24 | 8 | `first_version` | u64: the commit version of the round's first committed transaction |
| 32 | 4 | `transactions` | u32: the decision count |

| Section | Contents |
|---|---|
| 0 | decisions, 16 bytes each |
| 1 | the record table: per committed transaction `offset` u32 and `len` u32 within section 2 |
| 2 | the write-set records (section 12.2) |

| Decision offset | Width | Field | Type |
|---|---|---|---|
| 0 | 2 | `outcome` | u16: 0 committed, else the error code of the abort |
| 2 | 2 | zero | 0 |
| 4 | 4 | `record` | u32: the committed transaction's record index, counting from 0; `ABSENT` for an abort |
| 8 | 8 | `detail` | u64: the abort's detail; 0 for a commit |

Record 0 starts at section offset 0 and record i at `align8` of record i − 1's end; the padding between is zero and
section 2 ends with the last record.

Order: unused bytes 36–127 zero; sections; decisions a whole number of 16 bytes (`SectionLength`); their count equals
`transactions` (`CountMismatch` `"decisions"`); each decision: bytes 2–3 zero (`PaddingNotZero`); a commit's `record`
equals the commits before it (`FieldValue` `"record"`) and its `detail` is 0 (`FieldValue` `"detail"`); an abort's
`outcome` is an error code (`FieldValue` `"outcome"`) and its `record` is `ABSENT` (`FieldValue` `"record"`); the record
table a whole number of 8 bytes; one entry per commit (`CountMismatch` `"records"`); each entry: inside section 2
(`IndexOutOfRange`, section 1), at the expected offset (`FieldValue` `"record offset"`), padding zero
(`PaddingNotZero`), the record valid (`Record`, wrapping the record's refusal); section 2 ends with the last record
(`TrailingBytes`).

### 12.2 Write-set record

What a worker builds for a transaction: its writes as store entries and the conflict keys of its writes. The commit step
writes it again, once, for a transaction that ran commit-time operations: the operations' writes added and their results
in section 9, so the log holds exactly what was applied and what was answered. It is the body layout of section 4.1 with
a 40-byte fixed part and 11 sections; its offsets count from the record's first byte.

| Fixed part | Width | Field | Type |
|---|---|---|---|
| 0 | 8 | `namespace` | u64 |
| 8 | 8 | `read_version` | u64 |
| 16 | 8 | `session` | u64 |
| 24 | 8 | `request` | u64 |
| 32 | 8 | `principal` | u64: the principal the transaction acted for, the administrator of a security change; 0 without authentication (`SECURITY.md`) |

| Section | Contents |
|---|---|
| 0 | list deltas, in the posting format: key-value entries |
| 1 | dictionary entries |
| 2 | statement records |
| 3 | catalog entries |
| 4 | entries of the keyspace without versions |
| 5 | the commit record the client attached, bytes |
| 6 | conflict points: entries whose value is present and empty, a key written |
| 7 | conflict statements: a list key and the object it changed |
| 8 | conflict ranges: the first key and the key past the last |
| 9 | the commit-time operations' results, 16 bytes each in the Commit reply's result layout (section 6.3), in the order the transaction asked them; empty for a transaction that asked none |
| 10 | the record's operation, empty for a transaction: one 16-byte operation |

An operation is a write the store computes when it applies the record, at the record's commit version, rather than one
the record carries:

| Operation offset | Width | Field |
|---|---|---|
| 0 | 2 | `operation` u16: 1 drop a predicate, 2 swap a build in, 3 undo a build, 4 drop a graph, 5 drop a namespace's data, 6 drop a namespace, 7 drop every namespace |
| 2 | 6 | zero |
| 8 | 8 | `argument` u64: the dropped predicate's UID in the record's namespace, the build's record number, the dropped graph's UID; 0 for operations 5, 6 and 7 |

- **A drop** (operations 1 and 4 to 7, the Drop command's kinds 1 to 5) removes what section 6.5 says of its kind:
  the predicate's keys and its catalog entry, the graph's keys, the namespace's data, the namespace, or every
  namespace. Its record holds the dropped key ranges in section 8, so a transaction reading or writing into them below
  the drop's version conflicts; it is the last record of its round.
- **Swap a build in** writes the lists the build made, complete at the version of the round that started it, and its
  new declaration. **Undo a build** writes the build's old declaration back, after the build failed or a drop
  cancelled it, and removes every index key of the predicate that declaration does not name. Each is a round of its
  own. Its record's section 3 holds the catalog entry of the declaration it writes, and its other sections nothing; a
  build no longer held when its end was proposed has an empty section 3, and its end writes nothing.
- The build's record number is the first version of the round whose declaration started it.

A key-value section is entries back to back, each 8-aligned within the section:

| Entry offset | Width | Field |
|---|---|---|
| 0 | 4 | `key_len` u32 |
| 4 | 4 | `value_len` u32, `ABSENT` for a removal, which holds no value |
| 8 | `key_len` | the key |
| 8 + `key_len` | `value_len` | the value |
| | to a multiple of 8 | zero |

Order: the fixed part and table (section 4.1); sections 0–4 and 6–8 in turn, each entry inside its section
(`EntryOutOfBounds`) with zero padding (`PaddingNotZero`); every list delta's value present (`FieldValue` `"list
delta"`); every conflict point's value present and empty (`FieldValue` `"conflict point"`); every conflict statement's
and conflict range's value present (`FieldValue` `"conflict statement"`, `"conflict range"`); section 9 a whole number
of 16-byte results (`SectionLength`), each with `created ≤ 1` (`FieldValue` `"created"`) and bytes 9–15 zero
(`PaddingNotZero`); section 10 empty or one operation (`SectionLength`), its `operation` 1 to 7 (`FieldValue`
`"operation"`) and bytes 2–7 zero (`PaddingNotZero`).

The keys' and values' contents, the posting format of list deltas and the commit record are the store's: the server
alone writes and reads them, and the store's mapping contract versions them. This file states their framing.

Recovery replays the log's rounds above the store's applied version, which is the last commit version of a round the
log holds, or 0. Each committed record is written at its commit version: sections 0 to 4 as its writes, a session's
registration (request 0) or committed request with its stored reply (its commit version, its results from section 9),
and its operation. A swap rebuilds the build at the version before its own, from the declaration in its section 3,
then swaps it in; a swap or undo writes nothing when the store's catalog no longer declares the predicate, as its
first apply found its build cancelled by a drop. No record between the drop and the end declares the predicate again:
a write executed before the drop conflicts with its ranges, a declaration during the build is refused, and the server
proposes a build done before the drop at the drop's own sync, before any later write is decided.

A checkpoint trims the log through a round and records, beside its position, the last commit version it covers.
Recovery refuses to start, naming both versions, when the store's applied version is below the checkpoint's; naming
the version, when the store's applied version is neither the checkpoint's nor a held round's last version, nor 0 on a
log never trimmed.
