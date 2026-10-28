# UltraGraphDB binary protocol

This file specifies the bytes of UltraGraphDB's binary protocol, version 1: the frame, every command's body, the term
encodings, the error codes and the connection's rules. A client in any language implements the protocol from this file
and the golden frames in `ultragraphdb_protocol_wire/testdata/golden/`. The Rust crates `ultragraphdb_protocol_wire`
(the bytes) and `ultragraphdb_protocol_transport` (the connection's state machines) implement it for client and server,
and this file states what they do. Section 13 lists the points the code leaves open and where the design documents
differ from the code.

## Contents

1. Conventions
2. The frame
3. Commands
4. The body layout and its shared structures
5. Bodies of the connection: Hello, Welcome
6. Bodies of sessions and transactions: Register, Begin, Commit, Abort
7. Bodies of data requests: Read, Write, Operation
8. Bodies of prepared statements and the dictionary: Prepare, Execute, Describe
9. Bodies of results and control: Result, Credit, Cancel, Error
10. The term encoding of results
11. The connection: handshake, streams, credit, cancel, sessions, statements
12. Internal to the server and the log: Round and the write-set record
13. Open points

## 1. Conventions

| Rule | Statement |
|---|---|
| Byte order | Little-endian, except inside a result term's encoding (section 10), which states its own order per field |
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
| 16 | 16 | `nonce_reserved` | u128 | Reserved for an authenticated cipher | 0 |
| 32 | 8 | `cluster` | u64 | The cluster's identity, drawn at random when the cluster is created | the receiver's cluster |
| 40 | 4 | `size` | u32 | The frame's length, header and body | 256 to 1,048,576 |
| 44 | 4 | `view` | u32 | The VSR view | 0 in client frames and on one node |
| 48 | 4 | `release` | u32 | The sender's release | any |
| 52 | 2 | `protocol` | u16 | The frame format's version | see section 11.1 |
| 54 | 2 | `command` | u16 | What the body is (section 3) | a number of section 3 |
| 56 | 8 | `stream` | u64 | The request the frame belongs to | 0 for connection-level frames |
| 64 | 1 | `replica` | u8 | The sending replica | 0 from clients |
| 65 | 1 | `flags` | u8 | Bit 0 (`LAST`, value 1): the stream's last frame | bits 1 to 7: any; senders write 0 |
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
| 0–7 | `df fb f9 a6 ea 89 4f 61` | `checksum` | `0x614F89EAA6F9FBDF` |
| 8–15 | `a6 d7 a5 4a ce a6 54 33` | `checksum_body` | `0x3354A6CE4AA5D7A6` |
| 16–31 | `00` × 16 | `nonce_reserved` | 0 |
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
| 136–255 | `00` × 120 | command header, unused | |
| 256–263 | `08 00 00 00 0f 00 00 00` | body: section 0's `offset`, `len` | 8, 15 |
| 264–278 | `75 6c 74 72 61 67 72 61 70 68 2d 74 65 73 74` | section 0 | `ultragraph-test` |

XXH3-64 (seed 0) of bytes 8 to 255 is `0x614F89EAA6F9FBDF`, the `checksum`. The body's 23 bytes take one chunk, so
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
| 4 | `ReservedNotZero` (offset of the first nonzero byte) | Bytes 16 to 31, then bytes 66 to 127, are zero |
| 5 | `WrongCluster` | `cluster` is the receiver's cluster |
| 6 | `SizeBelowHeader` | `size ≥ 256` |
| 7 | `FrameTooLarge` | `size ≤ 1,048,576`; a stream receiver also refuses a `size` past its receive buffer, naming the buffer's capacity |
| 8 | `LengthMismatch` | The bytes received equal `size` |
| 9 | `BodyLength` | The body's length has a split into chunks and table (section 2.3) |
| 10 | `Chunk` (index 0, bytes 0 to D) | One chunk: XXH3-64(data, seed 0) equals `checksum_body` |
| 10 | `ChunkTable` | Two or more chunks: XXH3-64(table, seed 0) equals `checksum_body` |
| 11 | `Chunk` (index, start, end) | Each chunk in order equals its leaf |
| 12 | `WrongCommand` | A command's view reads a frame of that command |
| 13 | the command's rules | The command header, then the body (sections 4 to 9, 12) |

A stream receiver reads the 256-byte header first, applies steps 1 to 7 to it, then reads `size − 256` more bytes and
applies the rest.

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

Every RDF term a request names (Read, Write, Operation, Prepare, Execute) is a u32 index into the request's term table:
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

A client opens a connection: the protocols it speaks, its release and its name.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 2 | `protocol_min` | u16 | at least 1, refused as `FieldValue` (`"protocol_min"`) |
| 2 | 2 | `protocol_max` | u16 | at least `protocol_min`, refused as `FieldValue` (`"protocol_max"`) |
| 4 | 4 | `release` | u32 | any |

| Section | Contents |
|---|---|
| 0 | the client's name, UTF-8 |

Order: unused bytes 8–127 zero; `protocol_min`; `protocol_max`; sections; section 0 UTF-8.

### 5.2 Welcome (2)

The server's answer to Hello.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 2 | `protocol` | u16 | the chosen protocol, at least 1 (`FieldValue` `"protocol"`) |
| 4 | 4 | `release` | u32 | the server's release |
| 8 | 4 | `max_frame` | u32 | the largest frame the server takes, 4,096 to 1,048,576 (`FieldValue` `"max_frame"`) |
| 12 | 4 | `max_streams` | u32 | the streams a connection holds in flight, at least 1 (`FieldValue` `"max_streams"`) |
| 16 | 4 | `initial_credit` | u32 | the Result frames each result stream may send before a Credit |

| Section | Contents |
|---|---|
| 0 | the server's name, UTF-8 |

Order: unused bytes 2–3 and 20–127 zero; `protocol`; `max_frame`; `max_streams`; sections; section 0 UTF-8.

## 6. Bodies of sessions and transactions

### 6.1 Register (3)

A session is registered. The client sends `session` 0; the server answers with the session's number.

| Command header | Width | Field | Type |
|---|---|---|---|
| 0 | 8 | `session` | u64 |

No sections: the body is empty. Order: unused bytes 8–127 zero; sections.

### 6.2 Begin (4)

A transaction begins on the frame's stream. The client sends `read_version` 0; the server answers with the snapshot the
transaction reads.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 8 | `namespace` | u64 | |
| 8 | 8 | `read_version` | u64 | |
| 16 | 1 | `mode` | u8 | 0 read-write, 1 read-only; else `FieldValue` (`"mode"`) |

No sections. Order: unused bytes 17–127 zero; `mode`; sections.

### 6.3 Commit (5)

A transaction commits through a session with the session's next request number. The client sends `commit_version` 0
and no results; the server answers with the commit version and the result of each commit-time operation the
transaction asked for, in the order asked.

| Command header | Width | Field | Type |
|---|---|---|---|
| 0 | 8 | `session` | u64 |
| 8 | 8 | `request` | u64 |
| 16 | 8 | `commit_version` | u64 |

| Section | Contents |
|---|---|
| 0 | results, 16 bytes each |

| Result offset | Width | Field | Type |
|---|---|---|---|
| 0 | 8 | `uid` | u64: the node an insert-if-absent or upsert-and-link found or created; 0 for compare-and-set and counter add |
| 8 | 1 | `created` | u8 boolean: whether the operation created the node |
| 9 | 7 | zero | 0 |

Order: unused bytes 24–127 zero; sections; section 0 a whole number of 16-byte results (`SectionLength`); each result in
order: `created ≤ 1` (`FieldValue` `"created"`), bytes 9–15 zero (`PaddingNotZero` at the body offset).

### 6.4 Abort (6)

The frame's transaction aborts. The command header is all zero (`ReservedNotZero` otherwise); no sections.

## 7. Bodies of data requests

### 7.1 Read (7)

A data request in the transaction of the frame's stream: a quad pattern, at most `limit` rows (0 for no limit), and the
resume key a previous result ended with, empty to start.

| Command header | Width | Field | Type |
|---|---|---|---|
| 0 | 8 | `namespace` | u64 |
| 8 | 4 | `limit` | u32 |

| Section | Contents |
|---|---|
| 0 | the pattern (section 4.5), no parameter slot |
| 1 | the term table's records (section 4.4) |
| 2 | the term table's text |
| 3 | the resume key, bytes the server gave |

Order: unused bytes 12–127 zero; sections; the term table; the pattern.

### 7.2 Write (8)

A changeset in the transaction of the frame's stream. No reply; a refusal arrives as an Error on the stream and ends the
transaction.

| Command header | Width | Field | Type |
|---|---|---|---|
| 0 | 8 | `namespace` | u64 |

| Section | Contents |
|---|---|
| 0 | the change records, 16 bytes each, in order (section 4.6) |
| 1 | the args, u32s |
| 2 | the term table's records |
| 3 | the term table's text |
| 4 | the names: string list `ends` (section 4.3) |
| 5 | the names: string list `text` |

Order: unused bytes 8–127 zero; sections; the term table (2, 3); the names (4, 5); the args a whole number of u32s
(`SectionLength`, section 1); the records.

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
`predicate` without reading it.

Order: unused bytes 1–7 and 24–127 zero; `kind`; `delta`; `properties`; `links`; sections; the term table; the args a
whole number of u32s (`SectionLength`); the count (`CountMismatch` `"args"`); each arg in order: a link's direction is 0
or 1 (`FieldValue` `"link direction"`), every other arg is below the term table's length (`IndexOutOfRange`, section
0), arg 0 may be `ABSENT`.

## 8. Bodies of prepared statements and the dictionary

### 8.1 Prepare (10)

A statement prepared once per session: a quad pattern with parameter slots. The client sends the pattern and its
parameters' types with `statement` and `schema` 0 and no result types; the server answers with the statement's id, its
namespace's schema id (section 11.7), and the result column types.

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
types"`); result types are empty or count the unbound slots (`CountMismatch` `"result types"`).

### 8.2 Execute (11)

A prepared statement executed in the transaction of the frame's stream, its parameters bound by position, at most
`limit` rows (0 for no limit), from the resume key a previous result ended with.

| Command header | Width | Field | Type |
|---|---|---|---|
| 0 | 8 | `statement` | u64 |
| 8 | 4 | `limit` | u32 |

| Section | Contents |
|---|---|
| 0 | the term table's records |
| 1 | the term table's text |
| 2 | the parameters, u32 term indices, parameter 0 first |
| 3 | the resume key |

Order: unused bytes 12–127 zero; sections; the term table (0, 1); the parameters a whole number of u32s
(`SectionLength`); each below the term table's length (`IndexOutOfRange`, section 2).

### 8.3 Describe (12)

What a namespace holds, the data dictionary's answer. The client sends the namespace alone: `read_version` 0 and seven
empty sections. The server answers with the snapshot it read, the labels, relationship types and property keys, and
the catalog's declarations.

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
| 5 | declarations: change records of kinds 8, 9, 10, in that order (section 4.6); an entity type's `extra` is its preorder number |
| 6 | the declarations' args, u32s |

Order: unused bytes 16–127 zero; sections; the string list (0, 1); sections 2, 3, 4 in turn, each a whole number of
u32s (`SectionLength`) and each index below the string count (`IndexOutOfRange`); the args (6) a whole number of u32s;
the records (5).

The request's body, seven empty sections, is 56 bytes: the table, every `offset` 56 and every `len` 0.

## 9. Bodies of results and control

### 9.1 Result (13)

A columnar, dictionary-encoded batch of a Read or Execute: per column a u32 cell per row, an index into the batch's
term table or `ABSENT` for unbound; the term table holding the batch's terms in the encoding of section 10, with the
texts those encodings name but do not hold. The stream's last batch carries the header flag `LAST`; its resume key,
when not empty, continues a limited read.

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

A term's `text` is what its encoding names by UID or by hash: an IRI's text or a blank node's label for a UID
encoding, the lexical form of a literal held by hash. Its `annotation` is a language tag held by hash, or the IRI of a
datatype held by UID (section 10.3).

Order: unused bytes 8–127 zero; sections; offsets (1) a whole number of u32s; texts (3) a whole number of 8-byte pairs
(`SectionLength`); one pair per term (`CountMismatch` `"term texts"`); the string list (4, 5); cells (0) a whole number
of u32s; `columns × rows` cells (`CountMismatch` `"cells"`); each cell `ABSENT` or below the term count
(`IndexOutOfRange`, section 0); with no terms, section 2 is empty (`CountMismatch` `"term encodings"`); each term in
order: offset 0 for the first, its bytes from its offset to the next term's offset (or the section's end) hold exactly
one encoding of section 10.1 (`BadTerm`, section 2, the term); each text index `ABSENT` or below the string count
(`IndexOutOfRange`, section 3).

### 9.2 Credit (14)

More credit for the frame's result stream: `frames` more Result frames may be sent.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 4 | `frames` | u32 | at least 1 (`FieldValue` `"frames"`) |

No sections. Order: unused bytes 4–127 zero; `frames`; sections.

### 9.3 Cancel (15)

The frame's stream is cancelled. The command header is all zero; no sections.

### 9.4 Error (16)

A request refused on the frame's stream; or, on stream 0, the connection's last frame after a protocol error or a
refused handshake.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 2 | `code` | u16 | an error code of the table below (`FieldValue` `"code"`) |
| 8 | 8 | `detail` | u64 | as the code defines; 0 where the table gives none |

| Section | Contents |
|---|---|
| 0 | the message, UTF-8, for people |
| 1 | the value found, for `OperationFailed`: an RDF term in N-Triples syntax; empty otherwise |

Order: unused bytes 2–7 and 16–127 zero; `code`; sections; section 1 UTF-8; section 0 UTF-8. An empty section 1 means
no value.

| Number | Code | Retryable | `detail` | Meaning |
|---|---|---|---|---|
| 1 | Protocol | no | 0 | A frame broke a validation rule; the server closes the connection after this reply. The message is the rule's text |
| 2 | Conflict | yes | the conflict kind below | A transaction's read conflicted with a commit ordered before it |
| 3 | TooOld | yes | 0 | The read version is below what validation still holds |
| 4 | PastLimit | yes | the limit in milliseconds | The transaction lived past its limit |
| 5 | Busy | yes | 0 | The commit queue is full |
| 6 | TooLarge | no | the limit in bytes | A write set exceeds the largest frame |
| 7 | Refused | no | 0 | The catalog refused a write |
| 8 | OperationFailed | no | 0 | A compare-and-set did not hold; section 1 holds the value found |
| 9 | StaleStatement | no | the statement | The statement's schema id no longer matches its namespace's |
| 10 | UnknownStatement | no | the statement | The session holds no statement of this id: never prepared, or evicted |
| 11 | Parameters | no | the statement; 0 for a parameter type | The parameters do not fit the statement |
| 12 | SessionEvicted | no | the session | The session table evicted the session |
| 13 | RequestOutOfOrder | no | the request number expected | A commit's request number is not one above the session's last |
| 14 | SessionBusy | no | the session | The session has a commit in flight |
| 15 | StreamLimit | no | the limit | The connection holds as many streams as Welcome allows |
| 16 | ProtocolVersion | no | the server's highest protocol | No protocol both sides speak |
| 17 | Unavailable | no | 0 | The node refuses writes after a failed log sync, until it restarts |

A client retries a request refused with a retryable code. Code numbers are never reused.

| Conflict kind (`detail` of code 2) | Read |
|---|---|
| 1 | List: a list read whole |
| 2 | Statement: a statement tested |
| 3 | Range: a key range scanned |
| 4 | AbsentTerm: a dictionary key found absent |
| 5 | Catalog: the namespace's catalog |
| 6 | Drop: a range a drop wrote |

## 10. The term encoding of results

A Result batch holds each term in the store's own encoding: a tag byte and its payload. The tag fixes the encoding's
length, or where to read it. A vector's encoding is `3 + 4 × dimension` bytes; every other encoding is at most 33.

### 10.1 Tags and lengths

"Inline text" is a length byte `n` (0 to 15) then `n` bytes of UTF-8. "Hash" is 16 bytes of BLAKE2b with a 16-byte
digest, unkeyed, of the text's UTF-8 bytes: the result batch's string list supplies the text it names. "UID" is a u64
**big-endian** naming an IRI, a blank node or a datatype in the store's dictionary.

| Tag | Term | Payload after the tag | Length |
|---|---|---|---|
| `0x01` | IRI or blank node | UID | 9 |
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

An encoding is whole when its tag is listed, each inline length is at most 15, a vector's dimension is at least 1, and
the bytes hold exactly the length the table gives. A Result receiver checks this per term (section 9.1).

### 10.2 How the server chooses the encoding

| Term | Encoding |
|---|---|
| IRI, blank node | `0x01`, with the IRI's text or the label as the term's `text` |
| `xsd:string` literal | `0x20` up to 15 bytes, else `0x21` |
| Language-tagged literal | by tag length, then lexical form length, as the table gives |
| Literal of datatypes `0x10` to `0x1F` whose lexical form is the canonical form of its value (section 10.5) | the by-value tag |
| `float32vector` literal in canonical form: `[`, the elements separated by `, `, `]`, each element the shortest decimal that reads back as the same finite f32, without exponent | `0x40` |
| Any other literal of a known datatype, a by-value datatype's non-canonical form included | `0x30` up to 15 bytes, else `0x31` |
| A literal of any other datatype | `0x32` up to 15 bytes, else `0x33` |

Equal terms encode equally and unequal terms unequally.

### 10.3 Texts and annotations

| Tag | `text` | `annotation` |
|---|---|---|
| `0x01` | the IRI or the blank node's label | |
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
| `0x10` | u8 | 0 false, 1 true; a reader takes any nonzero byte as true |
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

A literal is encoded by value only when its lexical form equals the form below, so a client writes that form back:

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

### 11.1 Handshake

1. The client opens a TCP connection and sends Hello on stream 0: header `cluster` the cluster it is configured for,
   `release` its release, `protocol` 1, every other field 0.
2. The server validates the frame. A frame breaking a rule, or a first frame of another command, is answered with an
   Error of code 1 (Protocol) on stream 0, and the server closes the connection.
3. When `protocol_min` exceeds the server's highest protocol, the server answers an Error of code 16 (ProtocolVersion),
   `detail` the server's highest protocol, on stream 0, and closes the connection.
4. Otherwise the server answers Welcome on stream 0, its `protocol` the lesser of its highest protocol and
   `protocol_max`. A server speaks every protocol from 1 to its highest. The Welcome's header carries `protocol` 1 and
   the server's `release`.
5. The client validates the Welcome and refuses a `protocol` outside its Hello's range (`FieldValue` `"protocol"`).

Every later client frame carries in its header the cluster, the client's `release`, `protocol` equal to the Welcome's
`protocol`, and its stream. A client sends no frame larger than the Welcome's `max_frame`.

### 11.2 Streams

- Stream 0 carries the connection's own frames: Hello, Welcome, and the server's closing Error.
- Every request opens a new stream. The client numbers streams from 1 on each connection, upward, and never reuses a
  number on that connection. At most `max_streams` streams are open at once; the server refuses another with Error code
  15 (StreamLimit), `detail` the limit.
- The server answers streams independently, in whatever order their results are ready.
- A stream ends when its last frame arrives, when the client cancels it, or when an Error on it ends it. A frame of a
  stream that has ended is late; the Rust client drops it, and drops a frame whose `checksum` equals the previous
  frame's of its stream as a duplicate.

| Request | Stream | Reply |
|---|---|---|
| Register | a new stream | Register with the session number, or Error; the stream ends |
| Begin | a new stream, which becomes the transaction's | Begin with `read_version` filled; the stream stays open |
| Read, Execute | the transaction's stream | Result frames, the last flagged `LAST`; or Error |
| Write, Operation | the transaction's stream | none; a refusal is an Error on the stream, which ends the transaction and answers its next request |
| Commit | the transaction's stream; a retry goes on a new stream of its own | Commit or Error; the stream ends |
| Abort | the transaction's stream | none; the stream ends |
| Prepare, Describe | a new stream | Prepare or Describe, or Error; the stream ends |
| Credit, Cancel | the result's stream | none |

A transaction is its stream: one request at a time waits on it, and cancelling its result stream ends the
transaction. The Rust client sends no Write, Operation or Commit in a read-only transaction (Begin `mode` 1).

### 11.3 Credit

Each result stream starts with the Welcome's `initial_credit`. The server sends a Result frame on a stream only while
the stream's credit is above 0, and each Result frame it sends spends 1. A Credit frame on the stream adds `frames`
(saturating at 2³² − 1). Only Result frames spend credit; an Error and every other frame leave it. The server ignores
Credit for a stream that is not open. The Rust client grants more credit when its count reaches 0 and it waits for a
batch.

### 11.4 Cancel

A Cancel frame on a stream drops the frames the server queued for it, and the server sends nothing further on it.
Credit for it afterwards is ignored.

### 11.5 Sessions and exactly-once commits

- A client registers a session with Register (`session` 0) and receives its number. A connection holds as many
  sessions as it needs concurrent writers; reads need no session.
- Each Commit carries its session and a request number one above the session's last committed request; a new
  session's first request is 1. The server refuses another number with code 13 (RequestOutOfOrder), `detail` the number
  expected.
- One commit is in flight per session; the server refuses a second with code 14 (SessionBusy). The Rust client retries a
  SessionBusy commit after a delay.
- The server's session table holds each session's last committed request and its whole reply: the commit version and
  every commit-time operation's result. A retry of that request, with the same session and request number, returns the
  stored reply byte for byte, results included; so a client whose connection dropped after sending a Commit sends it
  again on the next connection, on a new stream, without a transaction, and is answered from the table if it committed.
- The table is durable: a session's registration and each committed request with its reply are written in the same
  round as the commit, so the table survives a restart of the server exactly as the commit does (decided 2026-10-28).
- The table is bounded; an evicted session's request is refused with code 12 (SessionEvicted).
- After a Commit reply the session's next request number is one higher; after a refusal it stays.

### 11.6 Commit-time operations

A transaction sends any number of Operation frames before its Commit. The Commit reply's results hold one 16-byte
result per operation, in the order the operations were sent: the node's UID and whether the operation created it for
kinds 1 and 2, `uid` 0 and `created` 0 for kinds 3 and 4. A compare-and-set that does not hold refuses the commit with
code 8 (OperationFailed), the value found in the Error's section 1.

### 11.7 Prepared statements and the schema id

- A statement is prepared per session; its id starts at 1 and is never reused within the session. The cache holds a
  bounded number of statements and evicts the oldest; executing an evicted statement is refused with code 10
  (UnknownStatement).
- Preparing refuses a parameter declared `2` (any term) at a graph, subject or predicate slot with code 11
  (Parameters), `detail` 0.
- Execute refuses, in this order: an unknown statement (code 10); a statement prepared against another schema id than
  its namespace's current one (code 9, StaleStatement), after which preparing it again succeeds; parameters that are not
  one per parameter slot (code 11). Each `detail` is the statement id.
- The schema id is XXH3-64, seed 0, of the Describe body (the data before any chunk table) that holds the namespace's
  declarations alone: sections 2, 3 and 4 empty, the strings only the declarations' names, interned in record order,
  arg by arg, and the declarations in the order the server lists them. It changes with the catalog and not with the
  data. For the golden `describe_reply.frame` it is `0xFE70341D112B65A9`.

## 12. Internal to the server and the log

No client sends or receives these. The log stores each round as a Round frame, header and body, and stage 10's
replication will carry the same bytes.

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
a 32-byte fixed part and 10 sections; its offsets count from the record's first byte.

| Fixed part | Width | Field | Type |
|---|---|---|---|
| 0 | 8 | `namespace` | u64 |
| 8 | 8 | `read_version` | u64 |
| 16 | 8 | `session` | u64 |
| 24 | 8 | `request` | u64 |

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
| 9 | the commit-time operations' results, 16 bytes each in the Commit reply's result layout (section 6.3), in the order the transaction asked them; empty for a transaction that asked none (decided 2026-10-28) |

A key-value section is entries back to back, each 8-aligned within the section:

| Entry offset | Width | Field |
|---|---|---|
| 0 | 4 | `key_len` u32 |
| 4 | 4 | `value_len` u32, `ABSENT` for a removal, which holds no value |
| 8 | `key_len` | the key |
| 8 + `key_len` | `value_len` | the value |
| | to a multiple of 8 | zero |

Order: the fixed part and table (section 4.1); sections 0–4 and 6–8 in turn, each entry inside its section
(`EntryOutOfBounds`) with zero padding (`PaddingNotZero`); every conflict point's value present and empty (`FieldValue`
`"conflict point"`); every conflict statement's and conflict range's value present (`FieldValue` `"conflict
statement"`, `"conflict range"`); section 9 a whole number of 16-byte results (`SectionLength`), each with
`created ≤ 1` (`FieldValue` `"created"`) and bytes 9–15 zero (`PaddingNotZero`). The keys' and values' contents are the
store's. Recovery restores a session's stored reply from the record: its commit version from the round, its results
from section 9.

## 13. Open points

### 13.1 Left to the server, which does not exist yet

`ultragraphdb_server_session` and `ultragraphdb_server_node` hold no code. The connection rules of section 11 come from
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
