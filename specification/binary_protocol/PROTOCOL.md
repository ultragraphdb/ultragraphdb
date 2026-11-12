# UltraGraphDB binary protocol

This file specifies the bytes of UltraGraphDB's binary protocol, version 1: the frame, every command's body, the term
encodings, the error codes and the connection's rules, for a client's connection to the frontend and for the role
connections between the frontend, compute and storage. A client in any language implements the protocol from this file
and the golden frames in `../../ultragraphdb_protocol/ultragraphdb_protocol_wire/testdata/golden`. The Rust crates `ultragraphdb_protocol_wire`
(the bytes) and `ultragraphdb_protocol_transport` (the connection's state machines) implement it for client, server and
roles. `SECURITY.md` beside this file describes the security architecture the protocol carries: authentication,
authorization and encryption.

This file leads the code: group 8 of stage 5.5 brings the crates and the golden frames to it
(`../../openspec/changes/archive/2026-11-09-stage-5-5-three-role-refactor/tasks.md`).

**Roles by stage.** Through stage 5.7 a cluster runs three roles, the frontend, compute and storage, and the frontend's
process hosts the metadata group and the sequencer; sections 1 to 13 state that cluster. Stage 5.8 moves the metadata
group and the sequencer into the *system group*, a replica group that storage hosts, and stage 5.9 makes the frontend
and compute one role, compute (`../../openspec/designs/UltraGraphDB Distribution Architecture.md`). Sections 11.12a and
11.14 state what each stage changes, and from its stage on each of them, and every paragraph beginning "From stage",
governs where another section differs. Every other rule
stands, read with "the system group" for the frontend as the host of the metadata group and the sequencer from stage
5.8, and with "compute" for the frontend as the role holding the client endpoint from stage 5.9. Each stage's change
brings the sections it changes to its text, in the order of `../../ROADMAP.md`.

## Contents

1. Conventions
2. The frame
3. Commands
4. The body layout and its shared structures
5. Bodies of the connection: Hello, Welcome, Authenticate, Nodes
6. Bodies of sessions, transactions and drops: Register, Begin, Commit, Abort, Drop
7. Bodies of data requests: Read, Write, Operation, Query
8. Bodies of prepared statements and the dictionary: Prepare, Execute, Describe, Install, Purge, Migrate
9. Bodies of results and control: Result, Credit, Cancel, Error
10. The term encoding of results
11. The connection: handshake, authentication, streams, credit, cancel, sessions, statements, limits, partitions, read
    levels and session tokens, role connections, the system group and compute, declarations and drops, finding
    compute nodes
12. Internal to the server and the log: Round and the write-set record
13. Bodies of role commands: Join, Members, Subscribe, EpochClose, Lease, Placement, Catalog, Challenge, Verify, Bind,
    Recall, ComputeRead, Scan, Entries, Reference, CloseRequest, Closed, DefaultEntry, Report, Held, Frontier,
    Neighbours, Session, Check, Ended, Roster, Declare, Defaults

## 1. Conventions

| Rule | Statement |
|---|---|
| Byte order | Little-endian, except inside a result term's encoding (section 10), and a system ID. A result term takes the store's term encoding (`../storage/STORAGE.md` section 5.1) with names given by the hash of their text in place of system IDs, which orders for equality only: its by-value numbers are big-endian and a vector's elements little-endian; section 10 states the order per field. A system ID is big-endian (below) |
| System ID | 16 bytes, the 128-bit ID the store names an entity or a statement by, big-endian as the store compares it: type prefix (bytes 0–1), sequence (2–9), bucket (10–12), reserved (13–15, zero). It is valid when its type prefix and sequence are not 0 and its reserved bytes are zero. System IDs cross only between roles, in role commands (section 13), and into the log (section 12). A client names and receives an entity by its IRI, a blank node by its skolem IRI, a statement without a statement ID by its quad and one with a statement ID by its reifier's skolem IRI (sections 4.4, 4.6, 9.1, 10.2); no frame a client sends or receives holds a system ID |
| Version | u64: `epoch << 22 \| sequence`, the sequence even, 0 to 2²² − 2. The version bound of epoch `e`, `e << 22 \| 0x3FFFFF`, is above every version stamped at or below `e`. A field holding a version or a version bound has its low 22 bits even or all ones, refused otherwise as `FieldValue` naming the field |
| Epoch | u64 holding an epoch number of 42 bits, 1 to 2⁴² − 1, or 0 for none where a field allows it; a receiver refuses any other value as `FieldValue` naming the field |
| Partition | u32, numbered from 1; 0 for none where a field allows it |
| Tenant | u64, from 0: the engine's unit of isolation, one catalog of numbered versions in the metadata group, one run of buckets, the grants of its principals. The metadata group holds the tenants (section 11.13), and the frontend and Bind refuse a number it does not hold (sections 11.10, 13.10). Nothing reads, writes or names across tenants; the cluster scope alone reaches over them, for backup and export (`../../openspec/notes/SECURITY.md`). Tenants may share a bucket run, of which there are at most 256: isolation is by the tenant every tile header, tile key and conflict key carries (`../storage/STORAGE.md` sections 6.2 and 13.1, section 12.2 here), and by grant, not by resource |
| Namespace | A unit of organisation inside a tenant: one schema source installed in it with its graphs, from stage 6.6 (`../schema/SCHEMA.md`, sections 1 and 15). It has no field in any key, bucket, ID or frame but one: the identifier a migration's staged namespace takes (sections 8.4, 8.6). A query text names a namespace (section 8.6) |
| Bucket | u32, 0 to 2²⁴ − 1: a system ID's bucket field, its top 8 bits the tenant's run |
| Bucket group | u32, 0 to 2¹⁸ − 1: a bucket's top 18 bits, 64 contiguous buckets, 2¹⁰ groups per run |
| Session | 16 bytes: a session's identity, 128 random bits the frontend draws, opaque; all zero names no session (section 11.5) |
| Integers | `u8`, `u16`, `u32`, `u64`, `u128` unsigned; `i16`, `i64`, `i128` two's complement. Widths in bytes: 1, 2, 4, 8, 16 |
| Offsets | A header offset counts from the frame's first byte. A command-header offset counts from the command header's first byte, the frame's byte 128. A body offset counts from the body's first byte, the frame's byte 256. A section offset counts from its section's first byte |
| Alignment | Every section starts at a body offset that is a multiple of 8. A multi-byte field needs no alignment in memory: read every field byte by byte |
| Padding | Every padding byte, reserved byte and unused command-header byte is zero; a receiver refuses a nonzero one |
| Text | UTF-8, delimited by offsets and lengths, with no terminator and no escape grammar. A string's bytes end on a character boundary |
| `ABSENT` | The u32 `0xFFFFFFFF`: no term, no string, the default graph, an unbound cell, a removal |
| Booleans | A u8 of 0 (false) or 1 (true); a receiver refuses any other value where this file says so |
| Checksums | XXH3-64 as `doc/xxhash_spec.md` of the xxHash repository specifies it, stored as a little-endian u64.  |
| One encoding per value | Where a rule below says "one encoding", a receiver refuses every other byte sequence for the same value, so re-encoding a decoded value gives the received bytes. Section 4.7 lists which layouts hold this rule and which hold it only by the sender's convention |
| T*n* | Decision *n* of `../../openspec/notes/UltraGraphDB Scalable Tile Store.md`, section 6 |

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
| 64 | 1 | `replica` | u8 | The sending replica | 0 from clients, and from roles, which run one replica each |
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
| 3 | `CommandNotServed` (the command's number) | A connection's receiver. The side that accepted the connection: a request is one its endpoint serves, the client endpoint no role command (512–767) and a role endpoint the commands section 11.12 lists for it. The side that dialled: a frame on stream 0 is Welcome, which comes once, as the answer to its Hello, so a later one is refused; Authenticate; or Error; and a frame on another stream answers a request it sent on that stream, by a command section 3 lists in the request's "Answered by", or Error. A frame on a stream this side has ended, numbered below the next stream it will open, is late (section 11.2) whatever its command: it takes steps 4 to 15 and is then dropped. A stream at or above that number answers no request and is refused |
| 4 | `ReservedNotZero` (offset of the first nonzero byte) | Bytes 66 to 127 are zero |
| 5 | `WrongCluster` | `cluster` is the receiver's cluster |
| 6 | `FieldValue` (`"flags"`, then `"view"`, then `"replica"`) | `flags` bits 1 to 7 are zero; in a client's frame and a role's, `view` and `replica` are 0 |
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
| 16 | the command's rules | The command header, then the body (sections 4 to 9, 12, 13) |

A stream receiver reads the 256-byte header first, applies steps 1 to 8 to it, then reads `size − 256` more bytes and
applies the rest.

From stage 5.9 the side that dialled a client endpoint also takes Nodes (section 5.4) on stream 0, at any time after
the point section 5.4 gives; before that stage a receiver refuses the command as `UnknownCommand`.

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
| 4 | Begin | both | empty | Begin, or Error |
| 5 | Commit | both | sections | Commit, or Error |
| 6 | Abort | client → server, and frontend → storage on a Bind stream | empty | nothing |
| 7 | Read | client → server | sections | Result frames, or Error |
| 8 | Write | client → server, and frontend → storage on a Bind stream | sections | to a client, nothing, and a refusal is an Error; on a Bind stream, Held, or Error |
| 9 | Operation | client → server, and frontend → storage on a Bind stream | sections | to a client, nothing, and its result comes with the Commit reply; on a Bind stream, Held, or Error |
| 10 | Prepare | both | sections | Prepare, or Error |
| 11 | Execute | client → server | sections | Result frames, or Error |
| 12 | Describe | both | sections | Describe frames, the last flagged `LAST`; or Error |
| 13 | Result | server → client | sections | Credit when the client wants more |
| 14 | Credit | the side that dialled → the side that accepted | empty | nothing |
| 15 | Cancel | the side that dialled → the side that accepted | empty | nothing |
| 16 | Error | server → client | sections | |
| 17 | Round | log and server only (section 12) | sections | |
| 18 | Authenticate | both, stream 0 | sections | Authenticate, until the server's final one; or Error on stream 0 |
| 19 | Drop | both | sections | Drop, or Error |
| 20 | DirectWrite | client → server, and frontend → storage | sections | Commit, or Error |
| 21 | ConditionalWrite | client → server, and frontend → storage | sections | Commit, or Error |
| 22 | Query | client → server | sections | Result frames, or Error |
| 23 | Install | both | sections | Install, or Error |
| 24 | Purge | both | sections | Purge, or Error |
| 25 | Migrate | both | sections | Migrate, or Error |
| 26 | Ingest | both; client → server, and frontend → storage | sections | Ingest, or Error |
| 27 | Nodes | server → client, on stream 0, from stage 5.9 | sections | nothing |
| 256–511 | reserved for the replica commands of stage 8 | | | |
| 512 | Join | both; storage, compute → frontend | sections | Join, or Error |
| 513 | Members | both; storage, compute → frontend | sections | Members |
| 514 | Subscribe | storage → frontend | empty | EpochClose frames |
| 515 | EpochClose | frontend → storage | sections | nothing |
| 516 | Lease | both; storage → frontend | empty | Lease, or Error |
| 517 | Placement | both; storage, compute → frontend | sections | Placement |
| 518 | Catalog | both; storage, compute → frontend | sections | Catalog frames, the last flagged `LAST`; or Error |
| 519 | Challenge | both; the client endpoint's role → the system group, from stage 5.8 | sections | Challenge |
| 520 | Verify | both; the client endpoint's role → the system group, from stage 5.8 | sections | Verify |
| 521 | Bind | both; frontend → storage | empty | Bind, or Error; the stream stays open |
| 522 | Recall | frontend → storage | empty | Commit or Drop, or Error |
| 523 | ComputeRead | frontend → compute | sections | Result frames, or Error |
| 524 | Scan | compute → storage | sections | Entries frames, or Error |
| 525 | Entries | storage → compute | sections | Credit when compute wants more |
| 526 | Reference | both; storage, frontend → storage | sections | Reference, or Error |
| 527 | CloseRequest | storage → frontend | empty | nothing; the close arrives as an EpochClose |
| 528 | Closed | both; compute → storage | sections | Closed, or Error |
| 529 | DefaultEntry | both; storage → frontend | sections | DefaultEntry, or Error |
| 530 | Report | both; storage → frontend | sections | Report, or Error |
| 531 | Held | storage → frontend | empty | nothing |
| 532 | Frontier | compute → storage | sections | Neighbours frames, or Error |
| 533 | Neighbours | storage → compute | sections | Credit when compute wants more |
| 534 | Session | both; the client endpoint's role → the system group, from stage 5.8 | empty | Session, or Error |
| 535 | Check | both; the client endpoint's role → the system group, from stage 5.8 | sections | Check |
| 536 | Ended | the client endpoint's role → the system group, from stage 5.8 | sections | nothing |
| 537 | Roster | both; compute → the system group, from stage 5.9 | sections | Roster |
| 538 | Declare | both; the client endpoint's role → the system group, from stage 5.8 | sections | Declare, or Error |
| 539 | Defaults | both; the client endpoint's role → the system group, from stage 5.8 | sections | Defaults frames, the last flagged `LAST`; or Error |
| 540 | kept for the offer of a compiled catalog version, which stage 6.6 specifies (section 11.12a) | | | |
| 541–767 | reserved for role commands | | | |

A receiver refuses every other number, and the reserved ones, as `UnknownCommand`. A retired command's number stays
retired and is refused the same way; no command is retired. "Both" means the request and its reply share the command and
the layout: the requester sends it with the reply's fields zero, and the receiver answers with them filled.

Commands 512 to 767 are role commands: roles exchange them on role connections (section 11.12), and a client never
sends or receives one. A client endpoint refuses a role command as `CommandNotServed`, naming the command, and closes
the connection (sections 2.5, 2.6). Section 13 holds their bodies. Commands 256 to 511 are kept for stage 8's replica
commands; in this stage every receiver refuses them as `UnknownCommand`.

From stage 5.8 the system group's endpoint answers what the table has storage or compute send the frontend, and
Challenge and Verify with them, and the frontend sends it Members, Placement, Catalog, Subscribe and CloseRequest too
(section 11.12a). A number first assigned at its stage, 27 and 534 to 539, is refused as `UnknownCommand` before that
stage; Challenge and Verify, which no endpoint serves before stage 5.8, are refused until then as `CommandNotServed`.
From stage 5.9 compute sends what the table has the frontend send; ComputeRead (523) is retired, its number refused as
`UnknownCommand` and never used again; and Scan, Entries, Frontier, Neighbours and Closed pass between compute and
storage as before.

## 4. The body layout and its shared structures

### 4.1 Sections

Every body, the write-set record (section 12.2) and a resume key (section 11.8) are a fixed part, a section table, and
the sections:

| Body offset | Contents |
|---|---|
| 0 | the fixed part, `F` bytes, a multiple of 8. Every command body has `F = 0`; the write-set record has `F = 64` (section 12.2) and a resume key `F = 24` (section 11.8) |
| F | the section table: for each of the command's `N` sections, `offset` u32 then `len` u32, both body offsets and lengths |
| F + 8N | the sections, in order |

Section 0 starts at `align8(F + 8N)`. Section i ≥ 1 starts at `align8(end of section i − 1)`, where `align8(x)` rounds
up to a multiple of 8. A command may declare a section *block-aligned*: when it is not empty, it starts at the least
body offset at or above that start whose frame offset (the body offset plus 256) is a multiple of 4,096, and the
records its command places in it start at frame offsets that are multiples of 4,096 too, as its command states; an
empty block-aligned section starts where any other would. The bytes between a section's predecessor's end and its start
are zero. The body ends where its last section ends; no padding follows the last section. A section of length 0 still
has its offset. A body with `N = 0` sections is empty.

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

Every RDF term a request names (Read, Write, DirectWrite, ConditionalWrite, Operation, Prepare, Execute, Drop, Purge) is
a u32 index into the request's term table, as are the value an OperationFailed Error reports (section 9.4), the nodes a
Commit reply names (section 6.3), the terms a resume key holds (section 11.8) and the terms role commands carry by their
text (section 13): two sections, `records` and `text`.

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
| kind 5: 6 | `TermTooDeep` (records' section, the entry, the depth, limit 32) | the depth is at most 32 |
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
| IRI | an absolute IRI (RFC 3987). A skolem IRI begins with a tenant's skolem base, `https://w3id.org/ultragraphdb/ns/tenant/{n}/`, then `.well-known/genid/`; the frontend compares that text exactly, so the same path under another base is an ordinary IRI. One that names another tenant than the request's, whose tail is not `{bucket}-{random}` in the digits section 10.2 gives, or whose bucket lies outside the tenant's run, is an IRI nobody holds: the frontend refuses it in a change or an operation, and a read binding it matches nothing and binds nothing (section 11.12). In a change or an operation, any other skolem IRI is taken only when the dictionary of the partition owning the bucket its text states holds it as a blank node's skolem IRI or as a statement reifier's name, so a forged one is refused (section 13.15). In a pattern, an unheld skolem IRI matches nothing, as any IRI nobody holds. A handle IRI, `{base}{Type}/{handle}` (`../schema/SCHEMA.md` section 7.3), names an instance of a keyless type by its handle: `{Type}` a keyless entity or relation type of the tenant's catalog, or a retired name of one (`schema/SCHEMA-EVOLUTION.md` section 4.2), and `{handle}` the bucket in 6 and 104 bits in 26 lowercase hexadecimal digits, joined by `-`. It is taken as a skolem IRI is: one whose bucket lies outside the tenant's run names nobody; in a change or an operation, it is taken only when the dictionary of the partition owning its bucket holds it as an instance's IRI, so a forged one is refused; in a pattern, an unheld one matches nothing. A client never mints a handle: the partition creating a keyless instance draws it. A derived handle, which a migration gives a secondary image (`schema/SCHEMA.md` section 7.3), is a handle like any other. A keyed instance's IRI, `{base}{KeySpace}/{key}` or its type's template, is an ordinary IRI on the wire, which a client mints from the key: the frontend reads its key space and key against the tenant's catalog to find its bucket (section 11.12), and a write creating an instance under an IRI its type does not mint is refused at commit (`../schema/SCHEMA.md` section 7.3, `UGS1038`) |
| Language tag | well-formed (BCP 47) |
| Literal of a known datatype (section 10.4) | its lexical form valid for its datatype |
| The predicate slot of a pattern or a change | an IRI |
| A pattern's graph or subject slot | an IRI |
| A pattern's object slot | not a blank node's label (kind 2), which a request names again by its skolem IRI (section 10.2), and not a triple term (kind 5) |
| The graph or subject slot of a change or an operation | an IRI or a blank node |
| Any slot of a change or an operation | not a triple term: no statement holds one |

Section 11.7's code 11 for a literal parameter comes first where it applies.

### 4.5 Quad pattern

A pattern is 32 bytes, four pattern slots of 8 in the order graph, subject, predicate, object. A pattern slot, which
names a position of the pattern and never a store's slot (`../storage/STORAGE.md` section 14.6), is `kind` u32 then
`value` u32 (T48, C48):

| Kind | Slot | Value |
|---|---|---|
| 0 | unbound | 0 |
| 1 | the default graph; at the graph slot only | 0 |
| 2 | bound to a term | the term's index in the body's term table |
| 3 | a statement's parameter; Prepare only | the parameter's number: 0 for the pattern's first parameter slot, 1 for the next |

A receiver checks the section's length, then each pattern slot in order:

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

Write (section 7.2), DirectWrite (section 6.6), ConditionalWrite (section 6.7) and Describe (section 8.3) carry changes and declarations as records over an args section. A
record is 16 bytes:

| Record offset | Width | Field | Type |
|---|---|---|---|
| 0 | 1 | `kind` | u8 |
| 1 | 1 | `flags` | u8 |
| 2 | 2 | zero | 0 |
| 4 | 4 | `args_start` | u32: the index of the record's first arg in the args section |
| 8 | 4 | `args_len` | u32: the record's arg count |
| 12 | 4 | `extra` | u32: 0 |

The args section is u32s. Record i's args follow record i − 1's: `args_start` of record 0 is 0, of record i the sum of
the `args_len` before, and the records consume every arg. A term arg is a term-table index; a name arg is an index into
the body's string list; `ABSENT` is written where the table says so.

| Kind | Record | Flags | Args |
|---|---|---|---|
| 1 | Insert a statement | 0 | graph (term or `ABSENT` for the default graph), subject, predicate, object: 4 terms |
| 2 | Insert an edge with properties | 0 | graph, subject, predicate, object, then a property term and its value term per pair: 4 + 2p |
| 3 | Delete a statement without a statement ID, by its quad | 0 | graph, subject, predicate, object |
| 4 | Delete a statement with a statement ID, by its reifier | 0 | graph, the reifier's IRI (section 10.2): 2 terms |
| 5 | Delete every statement of a subject and predicate | 0 | graph, subject, predicate |
| 6 | Set a statement's property | bit 0: by quad | the statement, then the property's predicate term, then its value term |
| 7 | Delete a statement's property | bit 0: by quad | the statement, then the predicate term, then the value term or `ABSENT` for every value |
| 8 | Declare a predicate | bits 0–1: the kind (0 edge, 1 property, 2 either); bit 2: single-valued; bit 3: unique; bit 4: count index; bit 5: vector index | names: the predicate; the datatype or `ABSENT`; the tokenizer count t, then t tokenizer names; the domain count d, then d domain names; with bit 5, the factory name, the option count o, then o pairs of option name and value name |
| 9 | Declare an entity type | bit 0: abstract | names: the type's IRI; the parent's IRI or `ABSENT` |
| 10 | Declare a relation | 0 | names: the relation's IRI, then each role's IRI: 1 + r |

"The statement" of kinds 6 and 7 is, with bit 0 clear, graph and the reifier's IRI (2 terms), naming a
statement that holds a statement ID; with bit 0 set, graph, subject, predicate and object (4 terms), naming the
statement of that quad without a statement ID, which gains one with its first property. A record names a statement by
its terms alone, never by a statement ID: a reifier's IRI is the one a result batch named it by (section 9.1),
which the server accepts as section 4.4's IRI rule states.

A receiver checks the records' section is a whole number of 16-byte records (`SectionLength`), then each record in
order:

| Step | Refusal | Rule |
|---|---|---|
| 1 | `FieldValue` (`"change kind"`) | `kind` is 1 to 10 in a Write, DirectWrite or ConditionalWrite, 8 to 10 in a Describe |
| 2 | `FieldValue` (`"declaration order"`) | Describe only: `kind` is at least the record before's |
| 3 | `FieldValue` (`"flags"`) | only the kind's flag bits are set; for kind 8, bits 0–1 are not 3 |
| 4 | `FieldValue` (`"change record"`) | bytes 2–3 are zero |
| 5 | `FieldValue` (`"extra"`) | `extra` is 0 |
| 6 | `FieldValue` (`"args_start"`) | `args_start` equals the args consumed so far |
| 7 | `CountMismatch` (`"args"`) | `args_start + args_len ≤` the args' count |
| 8 | `CountMismatch` (`"change args"`) | the arg count fits the kind: exactly 4 (kinds 1, 3), 4 + an even number (2), 2 (4, 9), 3 (5), 2 + 2 or 4 + 2 (6, 7, by flag bit 0), at least 1 (10); kind 8 at least 3, each count's names present, and every arg consumed. Kind 8 runs steps 8 and 9 together, arg by arg |
| 9 | `IndexOutOfRange` (the args' section, the arg's index in the section, the value, the limit) | each term arg below the term table's length and each name arg below the string list's length; `ABSENT` where the table allows it |

Then `CountMismatch` (`"args"`) unless the records consumed exactly the args section. A Describe body has no term
table, so its limit for term args is 0.

### 4.7 One encoding: what a receiver enforces

| Layout | Enforced by the receiver | Held by the sender's convention only |
|---|---|---|
| Header, section table, padding, command header | every byte | |
| Request term table | the text order, no `xsd:string`, no duplicate entry | the entries' order: Rust builders intern terms in order of first use, a triple's parts before it |
| String list | the ends' order | each string once, in order of first use |
| Write's names, Describe's strings | | interned: Write in record order, arg by arg; Describe's labels, then relationship types, then property keys, then the declarations' names in record order, then the entries' names and the retired names' current names in entry order |
| Describe's entries (section 8.3) | their order, offsets and zero bytes | |
| Warnings (section 4.9) | the count, the indices and zero bytes | messages interned in warning order |
| Result batch | the cell count and indices, each term encoding whole | terms in order of first appearance, reading the rows in order and each row's columns in order; each distinct encoding once; strings interned in term order, text then annotation |
| Commit results, Round decisions | the boolean and zero bytes, a result's node index | a Commit reply's term table interned in result order |
| Session token | every byte | |
| Write-set record, Round dictionary entries (section 12) | zero bytes, the entries' framing, the dictionary entries' order | |
| Role commands' records (section 13) | their order and zero bytes, as each section states | a role command's term table interned in record order |

A decoder that re-encodes a received body reproduces it byte for byte when the sender followed these conventions;
every golden frame does.

### 4.8 Session token

A session token names a commit: its partition, the position of its round in that partition's log, and the epoch of its
commit version (section 11.11). It is 24 bytes, in a command header at the offset each command gives:

| Token offset | Width | Field | Type |
|---|---|---|---|
| 0 | 4 | `partition` | u32: the commit's partition, from 1 |
| 4 | 4 | zero | 0 |
| 8 | 8 | `position` | u64: the commit's round's position in the partition's log, from 1 |
| 16 | 8 | `epoch` | u64: the epoch of the commit version, `commit_version >> 22` |

A token of 24 zero bytes is no token. A receiver checks, in order: bytes 4–7 zero (`ReservedNotZero` at the frame
offset); either every field is 0, or `partition ≥ 1`, `position ≥ 1` and `epoch` 1 to 2⁴² − 1 (`FieldValue`
`"token"`). A token has one encoding. The frontend refuses a Begin, DirectWrite, ConditionalWrite or Ingest whose token's epoch is
above the sequencer's open epoch with Error 1 (Protocol) on its stream, the message naming the epoch: no commit holds such a
token.

### 4.9 Warnings

A reply carries the warnings its request's compilation reported without refusing it, such as a retired name resolved
during its migration's deprecation window (`UQL0110`, `GQL0210`, `SPQ0210`; `../schema/SCHEMA-EVOLUTION.md` section 9), in three
sections: the warnings, 24 bytes each, then their messages as a string list (section 4.3). The first Result batch of a
Query or of an Execute of a text (section 9.1), a Prepare's reply (section 8.1) and an Install's reply (section 8.4)
carry them; a reply with no warning, and every other frame holding the sections, holds them empty.

| Warning offset | Width | Field | Type |
|---|---|---|---|
| 0 | 4 | `diagnostic` | u32: the diagnostic's number in bits 0–15 and its language in bits 16–23, as code 29's `detail` encodes a refused query's (section 9.4); bits 24–31 zero |
| 4 | 4 | `source` | u32: the source the location lies in: in an Install's reply, the source's place among the request's sources, from 0; 0 for a query text |
| 8 | 4 | `offset` | u32: the location's first byte within that source's text |
| 12 | 4 | `length` | u32: the location's length in bytes |
| 16 | 4 | `message` | u32: the message's index in the string list: the diagnostic's code, then its reason and, where one exists, its fix, for people, as code 29's message gives them; a retired name's names the current one |
| 20 | 4 | zero | 0 |

A reply carries at most 64 warnings, the first in source order, and a message is at most 512 bytes, so a first Result
batch's warnings leave its rows room at the floor of `max_frame`.

Order: the warnings a whole number of 24 bytes (`SectionLength`); the string list (section 4.3); at most 64 warnings
(`CountMismatch` `"warnings"`); each warning in order: bytes 20–23 zero (`PaddingNotZero`, the body offset); bits 24–31
of `diagnostic` zero and its language one section 9.4 lists (`FieldValue` `"diagnostic"`); `source` 0 outside an
Install's reply (`FieldValue` `"source"`); `offset + length` within a u32 (`FieldValue` `"location"`); `message` below
the string count (`IndexOutOfRange`, the warnings' section, the warning); each message at most 512 bytes (`FieldValue`
`"message"`).

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

The floor of 524,288 bytes holds a Result frame of one term of the largest encoding a store holds, a vector of
`value_bytes_max` bytes (section 5.2), at most 262,144, with room to spare (T48, C12).

### 5.2 Welcome (2)

The server's answer to Hello: the protocol chosen, its limits, and how the connection authenticates.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 2 | `protocol` | u16 | the chosen protocol, at least 1 (`FieldValue` `"protocol"`) |
| 2 | 1 | `auth` | u8 | 1: the connection authenticates before its first request (section 11.1), as the frontend's client endpoint and every role endpoint require; 0: a test server running without authentication, such as the Rust client's; else `FieldValue` (`"auth"`) |
| 4 | 4 | `release` | u32 | the server's release |
| 8 | 4 | `max_frame` | u32 | the largest frame the server takes, 524,288 to 1,048,576 (`FieldValue` `"max_frame"`) |
| 12 | 4 | `max_streams` | u32 | the streams a connection holds in flight, at least 1 (`FieldValue` `"max_streams"`) |
| 16 | 4 | `initial_credit` | u32 | the Result, Entries, Neighbours, Describe or Catalog frames each stream answering a read, a Describe or a Catalog may send before a Credit, at least 1 (`FieldValue` `"initial_credit"`) |
| 20 | 4 | `max_write_set` | u32 | the largest write set a transaction commits, in bytes: its write-set record rounded up to 8 bytes and the dictionary entries it adds to its round (section 11.9) |
| 24 | 4 | `transaction_ms` | u32 | the longest a read-write transaction lives, in milliseconds (section 11.9) |
| 28 | 4 | `value_bytes_max` | u32 | the longest value a write may hold, in bytes, measured as the value's sortable encoding in a compiled layout or its term encoding in the generic layout (`../storage/STORAGE.md` sections 5.1 and 5.2) as the write holds it, and in the generic layout a term's text, which the term encoding hashes, as well (T48): the store's `tile_bytes_max / 4` (`../storage/STORAGE.md` section 3.3), 4,096 to 262,144; a write holding a longer one is refused with code 6 (TooLarge), `detail` the bound (T48, C12) (`FieldValue` `"value_bytes_max"`) |

| Section | Contents |
|---|---|
| 0 | the server's name, UTF-8 |
| 1 | the authentication mechanisms the server accepts: string list `ends` (section 4.3) |
| 2 | the mechanisms: string list `text`, each a SASL mechanism name (section 11.1) |

Order: unused bytes 3 and 32–127 zero; `protocol`; `auth`; `max_frame`; `max_streams`; `initial_credit`; `value_bytes_max`; sections; section 0 UTF-8; the
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

`OAUTHBEARER` is offered only on a connection encrypted with TLS, from stage 8 (`../../openspec/notes/SECURITY.md`, *Where it is built*).
In stage 5.5 the client endpoint and every role endpoint offer `SCRAM-SHA-256` alone, in clear text and inside TLS
alike.

### 5.4 Nodes (27)

From stage 5.9. The cluster's compute nodes as the system group holds them (section 11.14): each node's advertised
address, its tags and its state, under the list's version. A compute node sends it on stream 0, unasked and with
`flags` 0, since stream 0 does not end: once it holds a list with at least one node, and no earlier than the
connection's first final Authenticate, or the Welcome on a connection of `auth` 0; and again whenever it holds a list
of a higher version than the one it last sent the connection. No connection receives it earlier, so a peer that has
not authenticated learns nothing of the cluster.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 8 | `version` | epoch (section 1) | the list's version, the epoch of the close that announced it, at least 1 (`FieldValue` `"version"`) |
| 8 | 4 | `self` | u32 | the sender's own node, its index in section 0, or `ABSENT` when the list does not hold it yet (`FieldValue` `"self"`) |

| Section | Contents |
|---|---|
| 0 | nodes, 16 bytes each, at least one |
| 1 | tag references, u32s, each an index into the tags' string list |
| 2 | addresses: string list `ends` (section 4.3) |
| 3 | addresses: string list `text` |
| 4 | tags: string list `ends` |
| 5 | tags: string list `text` |

| Node offset | Width | Field | Type |
|---|---|---|---|
| 0 | 4 | `address` | u32: the node's advertised address, its index in the addresses |
| 4 | 4 | `tags` | u32: the index in section 1 of the node's first tag reference |
| 8 | 2 | `tag_count` | u16: the node's tag references, 1 to 8 |
| 10 | 1 | `state` | u8: 1 serving, 2 draining |
| 11 | 5 | zero | 0 |

An advertised address is `tcp://host:port`, where a client reaches the node's client endpoint, which may differ from
where the node listens: `host` a DNS name of 1 to 253 bytes, an IPv4 literal, or an IPv6 literal in square brackets,
as RFC 3986 writes an authority, and `port` 1 to 65,535 in decimal without a leading zero; at most 265 bytes. The
address is the node's identity: one node per address. A tag is 1 to 32 bytes of `a` to `z`, `0` to `9` and `_`. Nodes
come in the order of their addresses' UTF-8 bytes, and addresses are interned in node order, so a node's `address` is
its index. A node's tag references ascend by their tags' bytes without repeat, and tags are interned at their first
use in that order. A list holds at most 1,024 nodes and 256 distinct tags, which the system group enforces at Join
(section 13.1), so every list fits one frame at the floor of `max_frame`.

Order: unused bytes 12–127 zero; `version`; sections; the string lists (2, 3) and (4, 5); section 0 a whole number of
16-byte nodes (`SectionLength`), at least one and at most 1,024 (`CountMismatch` `"nodes"`); as many addresses as
nodes (`CountMismatch` `"addresses"`); at most 256 tags (`CountMismatch` `"tag strings"`); `self` below the nodes'
count or `ABSENT`; section 1 a whole number of u32s (`SectionLength`), each below the tags' count (`IndexOutOfRange`,
section 1, the reference); each node in order: bytes 11–15 zero (`PaddingNotZero`), `address` its own index, `tags`
where the node before's references end, 0 for the first, `tag_count` 1 to 8 with `tags + tag_count` within section 1,
`state` 1 or 2 (`FieldValue` `"node"`, the node's index); each address of the form above and above the one before by
its UTF-8 bytes (`FieldValue` `"address"`, the node's index); a node's tags ascending by their bytes without repeat
(`FieldValue` `"node tags"`, the node's index); the nodes' references ending with section 1 (`CountMismatch`
`"tags"`); each tag 1 to 32 bytes of the characters above and referenced by a node (`FieldValue` `"tag"`, the tag's
index).

A client holds the list of the highest version it has received on any connection to the cluster. It replaces its list
whole by one of a higher version, and drops one whose version is at or below the version it holds, which is no error:
a node that lags sends an older list. `self` tells a client which entry the connection it holds belongs to, so it
knows that connection's tags and state without dialling again.

## 6. Bodies of sessions, transactions and drops

### 6.1 Register (3)

A session is registered for the connection's principal, which owns it (section 11.5). The client sends `session` all
zero; the server answers with the session's identity, 128 random bits, never all zero.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 16 | `session` | session (section 1) | all zero in a request, not all zero in a reply (`FieldValue` `"session"`) |

No sections: the body is empty. Order: unused bytes 16–127 zero; `session`; sections.

### 6.2 Begin (4)

A transaction begins on the frame's stream, which becomes the transaction's (section 11.2). Its Begin names what every
read of the transaction shares (section 11.11): the read level, the session token the client holds, if any, the
deadline, and the read version the client names, if any. A read request names its transaction and no level of its own.
The client sends `read_version` 0, or a version it names; the server answers at once with every other field as sent
and `read_version` the version the transaction's reads take: a version the client named, as sent; else, for a
read-only transaction at level 0, the version bound of the snapshot's epoch (section 1); else 0, since a read-write
transaction reads its partition's state when it binds to the partition, whatever its level, and a read-only
transaction's read at level 1 or 2 answers at the version each partition gives it. A
token raises a read-write transaction's commit epoch and makes a read-only transaction's reads wait for its epoch,
within the deadline; a read waiting past the deadline is refused with code 24 (TimedOut) on its own stream.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 8 | `tenant` | u64 | the transaction's tenant, which its every request acts in; a tenant binds per Begin, so one connection's transactions may name several (section 11.10) |
| 8 | 8 | `read_version` | u64 | in a request, 0 or the version or version bound (section 1) the client names, which only a read-only transaction at level 0 names (`FieldValue` `"read_version"`); in a reply, as above |
| 16 | 1 | `mode` | u8 | 0 read-write, 1 read-only; else `FieldValue` (`"mode"`) |
| 17 | 1 | `level` | u8 | 0 snapshot, 1 latest, 2 monotonic-fast (section 11.11); else `FieldValue` (`"level"`) |
| 20 | 4 | `deadline_ms` | u32 | the transaction's deadline, in milliseconds from the Begin's arrival, which every read's wait shares; 0 for the Welcome's `transaction_ms` |
| 24 | 24 | `token` | session token (section 4.8) | the token the client holds; zero for none |

No sections. Order: unused bytes 18–19 and 48–127 zero; `mode`; `level`; `read_version`; `token` (section 4.8);
sections. A named `read_version` within an epoch a partition a read reaches has closed reads what that partition applied at
or below it. Any other above the partition's visibility watermark is refused at once, never held waiting, with code 27
(AheadOfWatermark) on the read's stream; one below the partition's latest logged fold horizon (section 11.9), with code 3
(TooOld).

### 6.3 Commit (5)

A transaction commits through a session with the next number of the session's request counter, or through no session,
`session` all zero (section 11.5). The client sends `commit_version` 0, no token, no results and an empty term table;
the server answers with the commit version, through a session the commit's session token (section 11.11), and the result of each
commit-time operation the transaction asked for, in the order asked. A result names the node an insert-if-absent or
upsert-and-link found or created by its IRI, or by its skolem IRI when the node is a blank node, through the reply's
term table, never by its system ID.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 16 | `session` | session (section 1) | |
| 16 | 8 | `request` | u64 | at least 1 with a session, 0 with none (`FieldValue` `"request"`) |
| 24 | 8 | `commit_version` | u64 | |
| 32 | 24 | `token` | session token (section 4.8) | zero in a request; in a reply through a session, a token whose `epoch` is `commit_version >> 22`; in a reply through no session, `session` all zero, the zero token, as a declaration's reply holds it (section 11.13) (`FieldValue` `"token"`) |

| Section | Contents |
|---|---|
| 0 | results, 16 bytes each; empty in a request |
| 1 | the term table's records (section 4.4): the results' nodes, each an IRI entry (kind 1) holding the node's IRI or skolem IRI, interned in result order; empty in a request |
| 2 | the term table's text |

| Result offset | Width | Field | Type |
|---|---|---|---|
| 0 | 8 | `value` | i64: a counter add's new value, in two's complement; 0 for the other kinds |
| 8 | 4 | `node` | u32: for an insert-if-absent or upsert-and-link, the node's index in the term table; `ABSENT` for every other kind |
| 12 | 1 | `created` | u8 boolean: whether the operation created the node; 0 for every other kind |
| 13 | 3 | zero | 0 |

Order: unused bytes 56–127 zero; `request`; `token` (section 4.8, then its rule above); sections; the term table, every entry of
kind 1 (`FieldValue` `"node term"`); in a request, sections 0 to 2 empty (`CountMismatch` `"results"`); section 0 a
whole number of 16-byte results (`SectionLength`); each result in order: bytes 13–15 zero (`PaddingNotZero` at the body
offset), `created ≤ 1` (`FieldValue` `"created"`), `node` `ABSENT` or below the term table's length (`IndexOutOfRange`,
section 0), `value` 0 where `node` is not `ABSENT` and `created` 0 where it is (`FieldValue` `"result"`).

### 6.4 Abort (6)

The frame's transaction aborts. The command header is all zero (`ReservedNotZero` otherwise); no sections.

### 6.5 Drop (19)

A drop on a stream of its own, outside any transaction: a predicate, a graph, a tenant's data or everything in a
tenant. It commits as a transaction does, through a session with the next number of the session's
counter or through no session, in the one partition it reaches: the client sends `commit_version` 0 and no token; the
server answers with the drop's commit version and, through a session, its session token, the reply the partition's
session table stores as the Drop reply (section 11.5). A drop reaching several partitions, every partition holding the tenant's buckets, is
cross-partition and refused with code 23 (CrossPartition), dropping
nothing (section 11.13). The partition decides it as the last transaction of its round, and its write-set record
names the drop as an operation (section 12.2). A read-write transaction that read or wrote what the drop removes, with
its read version below the drop's version, aborts with Conflict, kind Drop.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 1 | `kind` | u8 | 1 a predicate, 2 a graph, 3 a tenant's data, 4 everything in a tenant; else `FieldValue` (`"kind"`) |
| 8 | 8 | `tenant` | u64 | the tenant dropped in |
| 16 | 16 | `session` | session (section 1) | as Commit's |
| 32 | 8 | `request` | u64 | as Commit's |
| 40 | 8 | `commit_version` | u64 | as Commit's |
| 48 | 24 | `token` | session token (section 4.8) | as Commit's: zero in a request; in a reply through a session, a token whose `epoch` is `commit_version >> 22`; in a reply through no session, the zero token (`FieldValue` `"token"`) |
| 72 | 8 | `catalog` | u64 | kinds 3 and 4: the tenant's catalog version the client drops under, 0 for a tenant holding none; kinds 1 and 2: 0 (`FieldValue` `"catalog"`) |

| Section | Contents |
|---|---|
| 0 | the term table's records (section 4.4): one IRI, the predicate's for kind 1 and the graph's for kind 2; none for kinds 3 and 4 |
| 1 | the term table's text |

Order: unused bytes 1–7 and 80–127 zero; `request`; `kind`; `catalog`, 0 for kinds 1 and 2 (`FieldValue` `"catalog"`);
`token` (section 4.8, then its rule above); sections;
the term table; one entry for drop kinds 1 and 2, none for 3 and 4 (`CountMismatch` `"drop terms"`); the entry of kind
1, an IRI (`FieldValue` `"drop term"`, the entry's kind).

A predicate drop removes the predicate's families and their entries in every graph of the tenant
(`../storage/STORAGE.md` section 15.5), and the frontend then removes its declaration by a catalog version (section
11.13). A graph drop removes every family of the graph in the tenant with its entries. A
tenant's data drop removes the tenant's data and keeps its catalog. A drop of everything in a tenant removes its data,
and the frontend sets its catalog to the initial catalog by a catalog version, which removes every namespace installed in
it. The families and entries a drop covers are all its tenant's, whose tile keys hold it, so another tenant sharing
the bucket run keeps its own. No drop removes a
dictionary entry or spans tenants. The privilege each takes is in `../../openspec/notes/SECURITY.md`.

In a tenant that holds a schema (`../schema/SCHEMA.md` section 6.1), a Drop of kind 1 naming a predicate a schema declares
is refused with code 7 (Refused), `detail` 0, the message naming the predicate and stage 9.5: removing a declaration
is a destructive schema change (`schema/SCHEMA.md` section 13.2). A predicate `@open` admitted is dropped as any other.
A Drop of kind 4 removes the tenant's schemas with its data and its default-entry records (section 11.13), after
which the tenant takes an Install again (section 8.4).

A Drop of a tenant's data or of everything in a tenant names in `catalog` the catalog version it drops under. The
frontend refuses one whose `catalog` is not the tenant's current version with code 2 (Conflict), kind 5 (Catalog),
before any partition sees it, dropping nothing, so a Drop aimed at an earlier schema never removes the data of a later
one. A Drop removes a container whole: the history of `@bitemporal` and `@append_only` types goes with the graph or the
tenant's data that held it, which no delete or retraction removes (`../schema/SCHEMA.md` section 8.4). A Drop of kind 3
or 4 is an audit event naming the principal, the tenant and that version (`../../openspec/notes/SECURITY.md`, *Audit*).

### 6.6 DirectWrite (20)

A changeset committed as one transaction in one frame: unconditional writes, whose outcome depends on nothing the client must
learn. Every change is held or refused with the rest, and no read reaches the client. It opens a stream of its own and needs no
Begin. The reply is a Commit or an Error and ends the stream, and a retry goes on a new stream with the same session and request
(section 11.5). A set of changes larger than a frame is a read-write transaction's. A DirectWrite is a commit for every rule of
section 11.5: its request number comes from the session's one counter, one commit is in flight per session, and one in flight
when a connection is lost is in doubt.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 16 | `session` | session (section 1) | all zero for none |
| 16 | 8 | `request` | u64 | as Commit's: at least 1 with a session, 0 with none (`FieldValue` `"request"`) |
| 24 | 8 | `tenant` | u64 | the tenant it acts in |
| 32 | 24 | `token` | session token (section 4.8) | the token the client holds, which raises the commit's epoch as a Begin's does; zero for none |
| 56 | 8 | `principal` | u64 | 0 from a client, refused at the client endpoint (`FieldValue` `"principal"`); the frontend fills it with the connection's principal when it relays, and the partition's write-set record names it (section 12.2) |

| Section | Contents |
|---|---|
| 0 | the change records, 16 bytes each, in order (section 4.6), at least one |
| 1 | the args, u32s |
| 2 | the term table's records |
| 3 | the term table's text |
| 4 | the names: string list `ends` (section 4.3), empty |
| 5 | the names: string list `text`, empty |

Order: unused bytes 64–127 zero; `request`; `token` as section 4.8 says; `principal`; sections; the term table (2, 3); the names (4, 5), a string list
holding no name, since only a declaration names one (`CountMismatch` `"names"`, the names' count against 0); the args
a whole number of u32s (`SectionLength`, section 1); section 0 not empty (`CountMismatch` `"changes"`); the records. A record of kinds 8
to 10, a declaration, is refused with `FieldValue` `"record"`, after its `kind` and before anything else of it: it commits through the
metadata group (section 11.13). The term table's order stays the sender's convention (section 4.7).

A write to a predicate the catalog in effect declares unique is conditional on the value being free, so a DirectWrite holding one is
refused with code 7 (Refused), `detail` the term's index, the message naming the predicate and ConditionalWrite. The frame declares the
client's intent and the catalog checks it, so a request named direct never runs a condition the client did not ask for.
In a tenant that holds a schema the same rule reads the catalog's request-typing facts (`../schema/SCHEMA.md` section 9.6): a
DirectWrite carries changes whose every predicate is blind. A rule over the instance's own fields is validated in the
frame's turn and refuses it with code 7 and the rule's number, as a commit's would. A change that claims a key, an
`@idempotency` value (T48, C47) or a `@unique` value, or that a rule reading beyond the instance reads, a bound, a `@limit`, `@no_overlap`, `@acyclic` or a
participant's field, is refused with code 7, `detail` the term's index, the message naming the predicate and a
read-write transaction.

The frontend checks the session and the rights as for a Commit (section 11.5), chooses the partition as for a transaction's first
request (section 11.12, *Writes*), and relays the frame on a new stream to that partition's role address with `principal` filled and
every other command-header byte and the body unchanged, relaying its reply. A one-frame write through a session naming only new blank nodes as subjects and claiming no value goes, in place of a partition taken in turn,
to the one at position XXH3-64, seed 0, of its `session` and `request` command-header bytes, modulo the count, among the
partitions holding the tenant's run in ascending order, so a resend reaches the same session table. A resend is relayed the same way, and
not recalled, since its frame holds its changes: the table decides, answering the stored reply for its last request, code 14 for
a request in its commit step and code 13 below its last, and otherwise committing it. The frontend checks every subject the frame names by an IRI or a skolem IRI against its map, and refuses a frame whose subjects spread over partitions with code 23 before any relay, `detail` as for an operation bound to none. A partition answers a frame whose first subject lies outside its bucket ranges with code 26, naming the owner, and the frontend refreshes its map and
resends once; it answers a frame whose first subject lies inside them and a later one outside, or any other statement reaching another partition's state, with code 23.

The partition executes the frame in one turn at its visibility watermark, holding nothing between requests, so no lifetime limit
or sweep applies. It commits through the commit step as any transaction does: the same log, the same acknowledgement, the same
session table. A conflict on what the execution tested internally, such as a statement written concurrently, never reaches the
client as a read conflict, since the client saw no read: the partition executes the frame again at a newer watermark, at most a
bounded number of times, and past that answers code 2 (Conflict).

### 6.7 ConditionalWrite (21)

A DirectWrite's changes, at most, with commit-time operations (section 11.6), which the commit step decides in commit order: an
outcome that depends on the state at that moment, and which the reply reports. Every rule of section 6.6 holds except the unique
predicate's refusal: an operation claims a unique value. It is the one-frame form of "write this if that holds", and no read
reaches the client beforehand, so no client-held read can go stale. In a tenant that holds a
schema its changes are held to blind predicates as a DirectWrite's are, and its operations are of kinds other than 1
and 2 (section 7.3).

The command header is DirectWrite's. A failed condition is an answer, not a conflict: a compare-and-set that does not hold is refused
with code 8 (OperationFailed) and the value found, and is not retried.

| Section | Contents |
|---|---|
| 0 to 5 | as DirectWrite's; section 0 may be empty |
| 1 | the args: the change records', then each operation's in order, as the kind's Count in section 7.3 says |
| 6 | the operations, 24 bytes each, the layout of Operation's command header bytes 0 to 23 (section 7.3), at least one |

Order: as DirectWrite's, except that section 0 may be empty, and that section 6 is a whole number of 24-byte operations (`SectionLength`, section 6) and not empty
(`CountMismatch` `"operations"`), checked before the records; the records, then the operations, consuming the args exactly. Each
operation in order, by section 7.3's Order: bytes 1–7 zero (`PaddingNotZero` at the body offset); `kind`; `delta`; `properties`; `links`; its
count, and each arg, an arg's `IndexOutOfRange` naming section 1, where this command's args are. A record running past the args is section 4.6's `CountMismatch` `"args"`. An operation running past them, or args left
after the last operation, is `FieldValue` `"args_start"`, the index of the arg where that operation or those args begin.

### 6.8 Ingest (26)

A block-columnar insert in one frame: blocks of statements of one graph and predicate, or of instances of one type, laid
out by column (`../../openspec/notes/UltraGraphDB Scalable Tile Store.md`, section 4.17). It opens a stream of its own and
needs no Begin, as a DirectWrite does. The frontend splits it into one share per partition its rows reach; each share
commits atomically at its partition, as a transaction of its own, and a refused share writes nothing of any block, while
the other shares commit or fail on their own (T23, T24). The reply lists every share's outcome and ends the stream.
Every rule of section 6.6 holds of an Ingest and of each share except where this section states another: its request
number from the session's one counter, one commit in flight per session, a lost reply in doubt, the token, the checks of
the session, the rights and the tenant gate, and the blind predicates of a tenant that holds a schema. An Ingest inserts
and nothing else: a delete, an operation or a statement property set alone is a Write's, a DirectWrite's or a
ConditionalWrite's (T25).

An Ingest creates keyed instances, which a DirectWrite does not: an entity's key claim and its `@idempotency` claim
live in the instance's own bucket (`../schema/SCHEMA.md` sections 7.2 and 9.3), so the partition a row reaches holds
both, at every partition count, and its share checks them in its turn as it checks the instance's own fields (T42). A
row whose key an instance of its key space already claims, or that a row of the same share claims before it, refuses
its share with code 7 (Refused), the row named and `detail` the UGS refusal number, 1008; a row whose `@idempotency`
value an instance of its type already claims writes none of its values, and the share's rows naming its blank node
name that instance (`../schema/SCHEMA.md` section 6.6); so does a row whose value an earlier row of the same share
claims, its blank node naming the first row's instance (T48, C45). Every other claim, and every rule reading beyond
the instance, stays a read-write transaction's, as section 6.6 has it. The share's resolution lists each row it so
skips and each edge it skips because the edge is already visible (section 12.1), so apply and replay skip them too.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 16 | `session` | session (section 1) | all zero for none |
| 16 | 8 | `request` | u64 | as Commit's: at least 1 with a session, 0 with none (`FieldValue` `"request"`) |
| 24 | 8 | `tenant` | u64 | the tenant it acts in |
| 32 | 24 | `token` | session token (section 4.8) | as DirectWrite's |
| 56 | 8 | `principal` | u64 | as DirectWrite's: 0 from a client, refused at the client endpoint (`FieldValue` `"principal"`), filled by the frontend when it relays a share |
| 64 | 8 | `catalog` | u64 | the tenant's catalog version the client encoded the frame against; 0 for a tenant holding none |
| 72 | 4 | `share` | u32 | 0 from a client, refused at the client endpoint (`FieldValue` `"share"`); the share's number, from 1, when the frontend relays a share |

| Section | Contents |
|---|---|
| 0 | the term table's records (section 4.4) |
| 1 | the term table's text |
| 2 | the blocks, 32 bytes each, at least one in a request; empty in a reply |
| 3 | the blocks' data, each block's at a section offset that is a multiple of 8, back to back in block order, the bytes between zero |
| 4 | the shares' outcomes, 48 bytes each; empty in a request |

| Block offset | Width | Field | Type |
|---|---|---|---|
| 0 | 1 | `kind` | u8: 1 a relation block, 2 an entity block |
| 1 | 1 | zero | 0 |
| 2 | 2 | `columns` | u16: the block's columns |
| 4 | 4 | `rows` | u32: the block's rows, at least 1 |
| 8 | 4 | `entries` | u32: a relation block's neighbours, at least `rows`; 0 for an entity block |
| 12 | 4 | `graph` | u32: the graph's term index; `ABSENT` for the default graph |
| 16 | 4 | `subject` | u32: a relation block's predicate, an IRI's term index; an entity block's type, the term index of the type's class IRI (`../schema/SCHEMA.md` section 11.2) |
| 20 | 4 | `data` | u32: the offset of the block's data in section 3 |
| 24 | 4 | `data_len` | u32: its length |
| 28 | 4 | zero | 0 |

A block's data is its parts in this order, each starting at a multiple of 8 from the data's first byte, the bytes
between zero; `n` is `entries` for a relation block and `rows` for an entity block:

| Part | Length | Holds |
|---|---|---|
| `rows` | 4 × `rows` | u32 per row, its subject's term index: a relation block's subjects, each once; an entity block's key column, its instances, each once, a keyed instance by the IRI its key mints and a new keyless one by a blank node |
| `ends` | 4 × `rows` | relation blocks only: u32 per row, the end of its neighbours in `neighbours`, strictly increasing, the last `entries`; row `i` holds the neighbours from `ends[i − 1]`, 0 for the first, to `ends[i]` |
| `neighbours` | 4 × `entries` | relation blocks only: u32 per neighbour, the object's term index |
| column 0 to `columns − 1` | each as below | a value per row of an entity block, per neighbour of a relation block |

| Column offset | Width | Field | Type |
|---|---|---|---|
| 0 | 4 | `predicate` | u32: the attribute's predicate, an IRI's term index |
| 4 | 4 | `width` | u32: the value's width in bytes, 1 to the Welcome's `value_bytes_max`, for a fixed-width value, a vector among them (T48, C12); 0 for a variable-width one |
| 8 | 4 | `present` | u32: the values present |
| 12 | 4 | zero | 0 |
| 16 | 8 × ⌈n / 64⌉ | `validity` | u64 words: bit `i mod 64` of word `i div 64` set when value `i` is present; the bits at or past `n` zero |
| | `present × width`, or `4 × present` and the values' bytes | `values` | the present values in order, each in its attribute descriptor's sortable encoding (`../storage/STORAGE.md` section 5.2): fixed-width values back to back; variable-width ones as `present` u32 ends, each the offset just past its value within the bytes that follow, nondecreasing, then the bytes |
| | to a multiple of 8 | zero | 0 |

| Outcome offset | Width | Field | Type |
|---|---|---|---|
| 0 | 4 | `share` | u32: the share's number, from 1 |
| 4 | 4 | `partition` | u32: the partition the share reached |
| 8 | 2 | `outcome` | u16: 0 committed, else the error code refusing the share (section 9.4) |
| 10 | 2 | zero | 0 |
| 12 | 4 | `block` | u32: a refused share's first failing block, its index in the frame; `ABSENT` for a refusal naming no block, and for a committed share |
| 16 | 4 | `row` | u32: that block's first failing row, its index in the block; `ABSENT` for a refusal of the block whole, and wherever `block` is `ABSENT` |
| 20 | 4 | zero | 0 |
| 24 | 8 | `detail` | u64: the refusal's `detail`, as its code defines; 0 for a committed share |
| 32 | 8 | `commit_version` | u64: a committed share's commit version; 0 for a refused one |
| 40 | 8 | `position` | u64: a committed share's round's position in its partition's log; 0 for a refused one. With `partition` and the epoch of `commit_version` it is the share's session token (section 4.8) |

A request holds its blocks and no outcome; a reply holds the request's command header as its receiver took it, sections
0 to 3 empty and its outcomes: a partition's reply its share's one, the frontend's reply to the client every share's, by
`share`, which numbers the shares from 1 in ascending order of their partitions.

Order: unused bytes 76–127 zero; `request`; `token` as section 4.8 says; `principal`; `share`; sections; a request:
section 4 empty (`CountMismatch` `"shares"`), the term table (0, 1), section 2 a whole number of 32-byte blocks
(`SectionLength`, section 2) and not empty (`CountMismatch` `"blocks"`); each block in order: bytes 1 and 28–31 zero
(`PaddingNotZero` at the body offset); `kind` 1 or 2 (`FieldValue` `"block kind"`); `rows` at least 1, and `entries` at
least `rows` for kind 1 and 0 for kind 2 (`CountMismatch` `"block rows"`); `graph` `ABSENT` or below the term table's
length, and `subject` below it (`IndexOutOfRange`, section 2, the block); `data` at the end of the block before's data,
rounded up to a multiple of 8, 0 for the first (`FieldValue` `"block data"`); `data` and `data_len` within section 3
(`IndexOutOfRange`, section 2, the block); then its data, each part in order: every term index below the term table's
length (`IndexOutOfRange`, section 3, the offset); no row repeating a term an earlier row of the block names
(`FieldValue` `"row"`, the row); `ends` strictly increasing, the last `entries` (`FieldValue` `"ends"`); each column:
bytes 12–15 zero (`PaddingNotZero`); `width` at most `value_bytes_max` (`FieldValue` `"width"`); the bits at or past `n` clear
(`PaddingNotZero`); `present` equal to the bits set (`CountMismatch` `"column values"`); a variable-width column's ends
nondecreasing and the last equal to its bytes' length (`FieldValue` `"column ends"`); the padding zero
(`PaddingNotZero`); `data_len` equal to the length the parts take (`CountMismatch` `"block data"`); section 3 ending with
the last block's data (`CountMismatch` `"block data"`). A reply: sections 0 to 3 empty (`CountMismatch` `"shares"`);
section 4 a whole number of 48-byte outcomes (`SectionLength`, section 4) and not empty (`CountMismatch` `"shares"`);
each outcome in order: bytes 10–11 and 20–23 zero (`PaddingNotZero`); `share` one above the outcome before's, 1 for the
first, and `partition` at least 1 and above the outcome before's (`FieldValue` `"share"`); with `outcome` 0, `block` and
`row` `ABSENT`, `detail` 0, `commit_version` a version and `position` at least 1; with any other, `outcome` an error
code, `row` `ABSENT` where `block` is, and `commit_version` and `position` 0 (`FieldValue` `"outcome"`, the outcome's
index). The term table's order stays the sender's convention (section 4.7).

The frontend checks the session and the rights as for a Commit (section 11.5), and each term as section 4.4 states,
refusing the frame whole as that section says. It refuses with code 2 (Conflict), kind 5 (Catalog), before any
partition sees it, a frame whose `catalog` is not the tenant's current version, the message naming both versions: the
client describes the catalog again (section 8.3) and encodes against the version in effect. It finds each row's
partition as section 11.12 finds a first request's subject's (*Writes*). Every new blank node of the frame is created in
exactly one share, so no share names a node another creates (T43): the frame's new blank nodes, joined wherever a row
whose subject is one names another as a neighbour, form groups; a group goes to the partition of the buckets the
`@idempotency` values of its keyless entities derive (`../schema/SCHEMA.md` section 7.1), of the anchor of each
node-encoded relation instance it holds, the object of the instance's anchor role triple (section 11.12, *Writes*), and
of the rows whose subjects are IRIs or skolem IRIs and that name one of its nodes as a neighbour (T48, C47). A group
none of these places goes, in a frame through a session, to the partition section 6.6 gives a write naming only new
blank nodes, by the frame's `session` and `request`, and in a frame through no session to one partition the frontend
chooses in turn for the frame, round-robin over the partitions holding the tenant's run, every such group of the frame
going there; and a row whose subject is a new blank node goes with its group. A group these place in two partitions refuses the frame with code 23
(CrossPartition) before any relay, `detail` as for an operation bound to none, the message naming the blank node and
the first two partitions in frame order, block by block and row by row: a blank node's label names it within one frame
alone, so the client names such a node by an IRI, or keeps the rows naming it within one partition. The placement of a
frame through a session reads the frame and the frontend's map alone, so a resend splits alike. It then relays a share to each partition reached, on a new stream to its role address: the frame's command header with
`principal` and `share` filled, its term table whole, and of each block the rows that partition owns, with their
neighbours and their columns' values, in their order; a block with no such row is left out of the share, and the
share's blocks and rows are numbered anew. Once every share is answered it answers the client with one reply, naming
each refused share's block and row by their indices in the client's frame. A resend is split and relayed the same way,
and each partition's session table decides its share as section 6.6 states, keyed by the session alone, its entry
holding the share's number beside the request (section 11.5; `../storage/STORAGE.md` section 9.3, kind 10; T48, C3),
so a resend reaching a partition under another share number, after the placement map changed, finds its entry; a
resend commits only the shares not yet committed and answers the rest from the tables.

A partition executes its share in one turn at its visibility watermark as section 6.6 states, and refuses it in its
outcome: with code 2, kind 5, when `catalog` is not the version in effect at the round's epoch; with code 7 (Refused),
the block named, for a block naming a type, an attribute or a predicate the version does not declare, a column whose
`width` is not its descriptor's sortable width, a block whose type, predicate or column's predicate is not blind to
write but for its type's key and `@idempotency` claims (`../schema/SCHEMA.md` section 9.6), the message naming the
predicate and a read-write transaction, or a column or an entity block in a tenant holding no schema, whose
relation blocks carry no column; with code 7 and the row named for a value its check refuses; with code 6 (TooLarge),
`detail` the bound, for a share whose body and resolution do not fit one round of its partition's log beside that round's
header and tables. Its round logs the share's body as received, with its resolution, and no write-set record
(section 12.1).

## 7. Bodies of data requests

### 7.1 Read (7)

A data request on a stream of its own, in the transaction it names: a quad pattern, at most `limit` rows (0 for no
limit), and the resume key a previous result of the same request ended with, empty to start (section 11.8). A
transaction's reads run beside each other, each on its stream, all at the level, session token, deadline and read
version the transaction's Begin names (section 11.11), so a read names no level of its own; cancelling one ends that
read alone (section 11.4). Each row names its statement by its quad and, when the statement holds a statement ID, by
its reifier's skolem IRI (section 9.1).

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 8 | `transaction` | u64 | the stream number of the transaction's Begin, an open transaction of the connection |
| 8 | 4 | `limit` | u32 | |

| Section | Contents |
|---|---|
| 0 | the pattern (section 4.5), no parameter slot |
| 1 | the term table's records (section 4.4) |
| 2 | the term table's text |
| 3 | the resume key, bytes the server gave |

Order: unused bytes 12–127 zero; sections; the term table; the pattern.

A Read naming a transaction the connection does not hold open is refused with Error 1 (Protocol) on its stream.

### 7.2 Write (8)

A changeset in the transaction of the frame's stream, in the transaction's tenant. No reply; a refusal arrives as an
Error on the stream and ends the transaction. The command header is all zero. A statement's properties, and one of
several parallel edges, are addressed through the statement's reifier IRI, and a statement without a statement ID by
its quad (section 4.6). Declarations, records of kinds 8 to 10, commit as a catalog version of the metadata group
(section 11.13).

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
| 0 | 1 | `kind` | u8 | 1 insert-if-absent, 2 upsert-and-link, 3 compare-and-set, 4 counter add, 5 delete an instance, 6 retract an instance; else `FieldValue` (`"kind"`) |
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
| 5 delete an instance | graph (term or `ABSENT`), the instance | 2 |
| 6 retract an instance | graph (term or `ABSENT`), the instance | 2 |

Insert-if-absent resolves the node that holds `value` for the unique `predicate` in `graph`, creating it with the
properties when none does. Upsert-and-link does the same and writes the links between that node and the others in the
same step. Compare-and-set replaces `subject`'s single value of `predicate` with `new` when it is `expected`; else the
commit is refused with `OperationFailed` and the value found. Counter add adds `delta` to `subject`'s integer counter
`predicate` without reading it; its result carries the counter's new value, computed by the commit step in commit
order. Insert-if-absent and upsert-and-link claim the value at the partition owning its bucket, computed from the
predicate's system ID and the value's term encoding; when that is not the transaction's partition, the transaction
aborts with code 23 (CrossPartition, section 11.11). Their result names the node by its IRI or skolem IRI (section
6.3).

In a tenant that holds a schema (`../schema/SCHEMA.md` section 6.1), insert-if-absent and upsert-and-link are refused
with code 7 (Refused), `detail` 0, the message naming the kind: each creates a node of no declared type. Compare-and-set
and counter add write an attribute of an instance, validated at commit as any write is.

Delete an instance and retract an instance name an instance of a declared type by the IRI that names it
(`../schema/SCHEMA.md` section 7.3): an entity's, a node-encoded relation instance's, or the reifier's of an edge-encoded
relation instance. The commit step decides each in commit order against the catalog version in effect and adds its
writes to the transaction's write-set record (section 12.2). A delete applies `schema/SCHEMA.md` section 8.5: the entity or
instance is removed with the relation instances its `@on_delete` actions reach in the partition, and an entity's
tombstone is written; on an instance of a `@bitemporal` type it is a retraction. A retraction applies section 8.4
there: it closes the instance's `asserted` at the commit and applies `@on_retract` to the relation instances naming it
in the partition. A rule that refuses either refuses the commit with code 7, `detail` its UGS refusal number
(`UGS1013`, `UGS1020`, `UGS1026`), and one reaching another partition's state aborts the transaction with code 23. An
IRI no instance holds, and either kind in a tenant with no schema, is refused with code 7, `detail` 0. An edge-encoded
instance that holds no reifier is deleted by the delete of its quad, change record 3 (section 4.6).

Order: unused bytes 1–7 and 24–127 zero; `kind`; `delta`; `properties`; `links`; sections; the term table; the args a
whole number of u32s (`SectionLength`); the count (`CountMismatch` `"args"`); each arg in order: a link's direction is 0
or 1 (`FieldValue` `"link direction"`), every other arg is below the term table's length (`IndexOutOfRange`, section
0), arg 0 may be `ABSENT`.

### 7.4 Query (22)

A query of the GQL core or the SPARQL core (`../query/QUERY-GQL-CORE.md`, `query/QUERY-SPARQL-CORE.md`), or a request of UQL
(`query/QUERY-ULTRA-QL.md`), in the transaction it names, on
a stream of its own, as a Read is: its text, its parameters by name, at most `limit` rows (0 for no limit), and the
resume key a previous result of the same request ended with, empty to start (section 11.8). A statement with a writing
clause needs a read-write transaction (`mode` 0), refused in a read-only one with code 29. The reply is a stream of
Result frames (section 9.1), the first carrying the column names and the compilation's warnings (section 4.9), the last
flagged `LAST`; a SPARQL update's reply is one Result of one row with the columns `inserted` and `deleted`,
`xsd:integer` terms, and a GQL statement without `RETURN` answers one Result of no columns and no rows, flagged `LAST`
(`query/QUERY-GQL-CORE.md` section 7.1), as a UQL mutation does (`query/QUERY-ULTRA-QL.md` section 4.1). The frontend compiles the text against the catalog version in effect for the
transaction's tenant, refuses it with code 29 and the core's diagnostic (section 9.4), and otherwise plans and executes
it as the quad-reads specification routes reads and section 11.12 routes writes.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 8 | `transaction` | u64 | the stream number of the transaction's Begin, an open transaction of the connection |
| 8 | 1 | `language` | u8 | 1 GQL, 2 SPARQL, 3 UQL; else `FieldValue` (`"language"`) |
| 12 | 4 | `limit` | u32 | |

| Section | Contents |
|---|---|
| 0 | the query text, UTF-8, at most `max_frame` less 512 bytes (`CountMismatch` `"query"`) |
| 1 | parameter names: string list `ends` (section 4.3) |
| 2 | parameter names: string list `text`, each a name without its `$` or `?` |
| 3 | the parameters' values, u32 term indices, one per name in the names' order |
| 4 | the term table's records (section 4.4) |
| 5 | the term table's text |
| 6 | the resume key, bytes the server gave |

Order: unused bytes 9–11 and 16–127 zero; `language`; sections; the term table (4, 5); the names (1, 2); section 3 a
whole number of u32s (`SectionLength`), one per name (`CountMismatch` `"parameters"`), each below the term table's
length (`IndexOutOfRange`, section 3); section 0 at most `max_frame` less 512 bytes (`CountMismatch` `"query"`) and UTF-8
(`NotUtf8`, section 0). A Query naming a transaction the
connection does not hold open is refused with Error 1 (Protocol) on its stream. A parameter the text does not use is
refused with code 11 (Parameters), `detail` 0; one the text uses and the names do not give likewise.

## 8. Bodies of prepared statements and the dictionary

### 8.1 Prepare (10)

A statement prepared once per connection, which holds it (section 11.7): a quad pattern with parameter slots, or a
query text of the GQL or SPARQL core or of UQL with named parameters. The client sends the pattern or text and its parameters'
types with `statement` and `schema` 0 and no result types; the server answers with the statement's id, its tenant's
schema id as the connection's principal sees it (section 11.7), the result column types and, for a text, the names
and its compilation's warnings (section 4.9). The statement belongs to the tenant `tenant` names.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 8 | `tenant` | u64 | |
| 8 | 8 | `statement` | u64 | |
| 16 | 8 | `schema` | u64 | |
| 24 | 1 | `language` | u8 | 0 a quad pattern, 1 GQL, 2 SPARQL, 3 UQL; else `FieldValue` (`"language"`) |

| Section | Contents |
|---|---|
| 0 | `language` 0: the pattern, parameter slots allowed; 1 to 3: the query text, UTF-8 |
| 1 | the term table's records |
| 2 | the term table's text |
| 3 | parameter types, one column-type byte per parameter, in parameter order |
| 4 | result column types, one byte per result column (section 9.1): for a pattern each unbound pattern slot in pattern order, then the reifier column; for a text each projection; empty in a request |
| 5 | names: string list `ends`; for a text, in a request the parameter names in parameter order, in a reply the parameter names then the result column names; empty for a pattern |
| 6 | names: string list `text` |
| 7 | the warnings (section 4.9); empty in a request and for a pattern |
| 8 | the warnings' messages: string list `ends` |
| 9 | the warnings' messages: string list `text` |

A column type names what a parameter or result column holds:

| Byte | Holds |
|---|---|
| 1 | a node: an IRI or a blank node; a query's node, edge, reifier or relation node, by its IRI |
| 2 | any term |
| 3 | `xsd:integer` and its subtypes |
| 4 | `xsd:decimal` |
| 5 | `xsd:double` or `xsd:float` |
| 6 | `xsd:string`, a language-tagged string, or a string a query's enum, IRI, unit or catalog-type value takes |
| 7 | `xsd:boolean` |
| 8 | `xsd:dateTimeStamp` or `xsd:dateTime` |
| 9 | `xsd:date` |
| 10 | `xsd:time` |
| 11 | `xsd:duration` and its subtypes |
| 12 | a vector literal |
| 13 | an `rdf:JSON` literal: a list or a record |

For a pattern, the graph, subject and predicate slots take nodes and the object slot any term, so a pattern's types
are 1 or 2; the server answers each unbound slot's result type by its position, 1 at graph, subject and predicate, 2 at
object, and the reifier column's 1. For a text, the server answers each projection's type from the core's static
typing, 2 where it is a union of kinds.

Order: unused bytes 25–127 zero; `language`; sections; the term table; for a pattern, the pattern, and sections 5 and 6
empty (`CountMismatch` `"names"`); for a text, section 0 UTF-8 (`NotUtf8`, section 0); section 3's bytes, then
section 4's, each 1 or 2 for a pattern and 1 to 13 for a text (`FieldValue` `"column type"`); parameter types count the
pattern's parameter slots, or the text's parameter names (`CountMismatch` `"parameter types"`); in a request, result
types are empty, and in an answer they count the unbound slots and one more, or the text's projections
(`CountMismatch` `"result types"`); in an answer for a text, the names count the parameters and the result columns
(`CountMismatch` `"names"`); sections 7 to 9 empty in a request and for a pattern (`CountMismatch` `"warnings"`); the
warnings (section 4.9).

### 8.2 Execute (11)

A prepared statement executed on a stream of its own, in the transaction it names, as a Read is (section 7.1): its
parameters bound by position, at most `limit` rows (0 for no limit), from the resume key a previous result of the same
request ended with.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 8 | `statement` | u64 | |
| 8 | 8 | `transaction` | u64 | the stream number of the transaction's Begin, an open transaction of the connection |
| 16 | 4 | `limit` | u32 | |

| Section | Contents |
|---|---|
| 0 | the term table's records |
| 1 | the term table's text |
| 2 | the parameters, u32 term indices, parameter 0 first |
| 3 | the resume key |

Order: unused bytes 20–127 zero; sections; the term table (0, 1); the parameters a whole number of u32s
(`SectionLength`); each below the term table's length (`IndexOutOfRange`, section 2).

A statement prepared from a text takes its parameters by position in the order its Prepare named them, and answers
Result frames as a Query does (section 7.4), the column names and the warnings its Prepare answered on the first batch;
a statement with a writing clause executes in a read-write transaction only (code 29).

### 8.3 Describe (12)

What a tenant's catalog declares and its metadata records, as far as the connection's principal may read it, every
namespace installed in the tenant included. The client sends the tenant alone: `read_version` 0 and nine empty sections.

The frontend answers a Describe itself from the metadata group, asking no other role, and so names what is declared and
recorded rather than what data holds: `read_version` the version bound of the latest epoch the sequencer closed, or of
epoch 0 before the first close; section 2 the entity types declared in effect at that epoch; section 3 the predicates of
kind edge or either that it declares or holds a default entry for; section 4 those of kind property or either; sections
5 and 6 the catalog's declarations the principal may read; sections 7 and 8 the entries of the catalog version those
declarations are drawn from that the principal may read, the installed sources and the retired names among them. An
answer that does not fit one frame within the client's `max_frame` continues in further frames (*Continuation*, below).

| Command header | Width | Field | Type |
|---|---|---|---|
| 0 | 8 | `tenant` | u64 |
| 8 | 8 | `read_version` | u64 |

| Section | Contents |
|---|---|
| 0 | strings: string list `ends` |
| 1 | strings: string list `text`, each IRI in plain text |
| 2 | labels, u32 string indices, in order |
| 3 | relationship types, u32 string indices |
| 4 | property keys, u32 string indices |
| 5 | declarations: change records of kinds 8, 9, 10, in that order, each kind's records in the order of their declared IRIs' UTF-8 bytes (section 4.6); a record names no place in the type hierarchy, and a type a schema declares carries its prefix in its Type entry, kind 3 below |
| 6 | the declarations' args, u32s |
| 7 | entries, 16 bytes each, by kind, then by name as UTF-8 bytes, the order of a catalog version's entries (`../schema/SCHEMA.md` section 10.2) |
| 8 | the entries' bytes, back to back in entry order |

| Entry offset | Width | Field | Type |
|---|---|---|---|
| 0 | 1 | `kind` | u8: the entry's kind, below |
| 1 | 3 | zero | 0 |
| 4 | 4 | `name` | u32: the name the catalog orders the entry by, a string index; for the report, the empty string |
| 8 | 4 | `offset` | u32: the entry's first byte in section 8, where the bytes of the entry before end |
| 12 | 4 | `len` | u32: its bytes' length |

| Kind | Entry (`../schema/SCHEMA.md` section 10.2) | Carried to the principal |
|---|---|---|
| 1 | Schema | whole |
| 2 | Module | whole |
| 3 | Type | whole |
| 4 | Prefix ranges | whole |
| 5 | Attribute | where it holds `read` on every predicate the entry names |
| 6 | Predicate | likewise |
| 7 | Role | likewise |
| 8 | Navigation | likewise |
| 9 | Enum | whole |
| 10 | Record | whole |
| 11 | Named attribute type | whole |
| 12 | Quantity | whole |
| 13 | Unit | whole |
| 14 | Derivation | where it holds `read` on every predicate the entry names |
| 15 | Function | likewise |
| 16 | Graph | whole |
| 17 | Classification | where it holds `read` on every predicate the entry names |
| 18 | Report | likewise |
| 19 | Source: an installed source, by its name | where it holds `read` on every predicate the source declares |
| 20 | Retired name: a name an item held in an earlier version (`../schema/SCHEMA-EVOLUTION.md` section 4.2), by that name | whole for a type or a graph; for an attribute, a role or a navigation name, where it holds `read` on the predicates of the item holding the name now |

Kinds 1 to 18 hold the entry's bytes as the catalog version encodes them, in the canonical encoding of `../schema/SCHEMA.md`
section 10.2, laid out in `../schema/CATALOG.md`, which the protocol carries opaque as Catalog carries a version's bytes
(section 13.7). Kind 19 holds the
source's text, UTF-8, the migration sources installed with a target among them. Kind 20 holds 24 bytes:

| Retired name offset | Width | Field | Type |
|---|---|---|---|
| 0 | 1 | `item` | u8: 1 a type, 2 an attribute, 3 a role, 4 a navigation name, 5 a graph |
| 1 | 1 | `fate` | u8: 1 renamed, the item holding `current` now; 2 retired by a split, a union or a drop, holding no item, which a query naming it is refused for with the migration named |
| 2 | 2 | zero | 0 |
| 4 | 4 | `current` | u32: for `fate` 1, the item's current name, a string index; `ABSENT` for 2 |
| 8 | 8 | `version` | u64: the catalog version whose cutover retired the name, at least 1 |
| 16 | 8 | `until` | u64: for `fate` 1, when the deprecation window ends, in microseconds of Unix time, after which a query naming it is refused with the current name; 0 for 2 |

The whole and filtered kinds follow `../schema/SCHEMA.md` section 10.3's rule, extended to the entries it does not list: the
schemas' vocabulary whole, what names a predicate filtered. The schema id hashes the entries as the principal sees them
(section 11.7).

Order: unused bytes 16–127 zero; sections; the string list (0, 1); sections 2, 3, 4 in turn, each a whole number of u32s
(`SectionLength`) and each index below the string count (`IndexOutOfRange`); the args (6) a whole number of u32s; the
records (5); section 7 a whole number of 16-byte entries (`SectionLength`); each entry in order: bytes 1–3 zero
(`PaddingNotZero`, the body offset), `kind` 1 to 20 (`FieldValue` `"entry kind"`), `name` below the string count
(`IndexOutOfRange`, section 7, the entry), `offset` where the bytes before end (`FieldValue` `"entry offset"`), the
entry's bytes within section 8 (`IndexOutOfRange`, section 8, the entry), after the entry before by kind, then by its
name's UTF-8 bytes (`FieldValue` `"entry order"`, the entry), a kind-19 entry's bytes UTF-8 (`NotUtf8`, section 8), a
kind-20 entry's bytes 24 long, bytes 2–3 zero, `item` 1 to 5, `fate` 1 or 2, `current` below the string count for `fate`
1 and `ABSENT` for 2, `version` at least 1 and `until` 0 for `fate` 2 (`FieldValue` `"retired name"`, the entry); the
entries' bytes ending at section 8's end (`CountMismatch` `"entry bytes"`).

The request's body, nine empty sections, is 72 bytes: the table, every `offset` 72 and every `len` 0.

**Continuation.** An answer larger than one frame within the client's `max_frame` continues in further Describe frames
on the stream, each with the first's command header, the last alone flagged `LAST`. The client reads them as one body,
each of its nine sections the concatenation, in frame order, of that section's bytes in every frame, and checks the
Order above once, on that body; a frame breaking the first's command header is refused as `FieldValue` (`"describe"`). A
frame splits a section of records or u32s only between whole ones and a text only on a character boundary, so each
frame's sections hold whole units (`SectionLength`) and UTF-8 (`NotUtf8`). Describe frames spend credit as Result frames
do, the stream starting with the Welcome's `initial_credit` (section 11.3). An answer's sections hold at most
`catalog_bytes_max` bytes together (section 11.9), whatever the tenant holds: the frontend refuses a larger answer with
code 6 (TooLarge), `detail` the bound, and a client refuses a stream passing it as `FieldValue` (`"describe"`) and reads
no further frame of it, so assembling an answer is bounded on both sides.

### 8.4 Install (23)

The sources of a tenant's schema, installed as `../schema/SCHEMA.md` section 10.1 states: whole schemas and modules, each
replacing the source of its name, compiled with every source the tenant already holds into a new catalog version
built on `version`, committed as pending through the metadata group and taking effect at the epoch section 11.13
gives a version. The client sends the sources with `epoch` and the reply's `version` zero and no report; the server
answers with the new version's number, its effective epoch and the compilation report, or refuses with code 28 and
the diagnostics. Installing takes `declare` on the tenant, which the gate of section 11.10 checks first (`SECURITY.md`). The reply answers once the epoch is
assigned and, for a tightening change, once every partition holding the tenant's data has passed the validation scan
at that epoch, as a type change's Commit does. The reply carries the compilation's warnings (section 4.9).

**One Install per tenant before stage 9.5.** Every change to an installed schema is stage 9.5's (`../schema/SCHEMA.md`
sections 13 and 14). Until then an Install into a tenant that holds a schema is refused with code 7, `detail` 0, the
message naming Install and stage 9.5 (section 11.2), after the check of `version`. A first Install carries one or more
sources, schemas with the modules extending them.

**The fence of a first schema.** Before it checks that the tenant holds no statement, no write-path declaration and no
default-entry record (below), the frontend fences the tenant: it refuses every Write, Operation, Commit, DirectWrite, ConditionalWrite
and Drop naming the tenant with code 31 (Fenced), `detail` 0, the message naming the tenant, a transaction's refusal
ending it, and waits for the tenant's commits already in flight to be answered. It then reads the tenant at an epoch at
or above the last of those commits', at most one statement from each partition holding the tenant's run, so a
statement whose default-entry record has not yet been asked for is seen. The fence holds until the version
takes effect, or until the Install is refused, and survives a frontend's restart with the pending version it belongs
to. So no write lands between the check and the effective epoch, and a client runs a fenced write again after a delay,
when the schema validates it. Reads go on.

**A migration.** A source whose text declares a `migration` (`../schema/SCHEMA-EVOLUTION.md` section 3) installs with its target's
schema source in one Install, and the compiler checks it as section 5 there states. The version installed then holds the
target as a staged namespace, which no request but its run's sees, and the reply names it by `staged`, an identifier the
metadata group allocates, never reused in the tenant, and carries the migration report in section 3
(`schema/SCHEMA-EVOLUTION.md` sections 5 and 9, the rename map among it). Migrations installed together are one joint
migration: one run and one `staged`. Migrate runs it (section 8.6). An Install holding a migration source is refused
with code 7 before stage 9.5 (section 11.2).

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 8 | `tenant` | u64 | |
| 8 | 8 | `version` | u64 | in a request, the catalog version the sources build on, 0 for a tenant with none; in a reply, the version installed, at least 1 (`FieldValue` `"version"`) |
| 16 | 8 | `epoch` | epoch (section 1) | in a reply, the epoch the version takes effect at; 0 in a request |
| 24 | 8 | `staged` | u64 | in a reply to sources holding a migration, the staged namespace's identifier, at least 1; else 0, and 0 in a request (`FieldValue` `"staged"`) |

| Section | Contents |
|---|---|
| 0 | sources: string list `ends` |
| 1 | sources: string list `text`: for each source its name, then its text, in that order |
| 2 | the report, UTF-8, the coordination report of `../schema/SCHEMA.md` section 9.5; empty in a request |
| 3 | the migration report, UTF-8 (`../schema/SCHEMA-EVOLUTION.md` section 5); empty in a request and without a migration |
| 4 | the warnings (section 4.9); empty in a request |
| 5 | the warnings' messages: string list `ends` |
| 6 | the warnings' messages: string list `text` |

Order: unused bytes 32–127 zero; `version`, in a reply at least 1 (`FieldValue` `"version"`); `epoch`, 0 in a request
and an epoch in a reply (`FieldValue` `"epoch"`); `staged`, 0 in a request (`FieldValue` `"staged"`); sections; in a
request, sections 2 to 6 empty (`CountMismatch` `"report"`); the string list, an even count of strings, at least two
(`CountMismatch` `"sources"`), each name at most 255 bytes (`FieldValue` `"source name"`); sections 2 and 3 UTF-8
(`NotUtf8`, the section); section 3 empty when `staged` is 0 (`CountMismatch` `"migration report"`); the warnings
(section 4.9). A version built on one no longer current is refused with code 2 (Conflict), kind 5 (Catalog), as a
declaration's is. The frontend refuses a compiled version whose bytes pass `catalog_bytes_max` (sections 11.9, 11.13)
with code 7, `detail` the bound, the message naming its size, and nothing is installed. An Install into a tenant
that holds a statement, or whose catalog holds a write-path declaration or a default-entry record (section 11.13), is
refused with code 7, `detail`
0, the message naming the Drop of kind 4 that empties the tenant: a first schema governs no data written before it
(`../schema/SCHEMA.md` section 10.1).

### 8.5 Purge (24)

The legal purge of `../schema/SCHEMA.md` section 8.8: the instances a list of IRIs names, removed with every version of their
data, which no delete or retraction removes. The client sends the tenant and the IRIs with `epoch` and `removed` 0. The
frontend sequences the purge at an epoch through the metadata group and executes it at each instance's home and at every
member of its referrer set, as an entity delete's shares are, with removal in place of a tombstone's fence (section
12.2, operation 9); with one partition holding the tenant's run, it executes there alone. It answers with the purge
epoch and the instances removed once every partition the purge reaches has applied it. The metadata group records the
purge epoch with the SHA-256 of each IRI purged, never the IRI. A Purge takes `purge` on the tenant, which the gate of
section 11.10 checks first, refusing it with code 21 (`SECURITY.md`); it is an audit event. A Purge resent after a lost
reply removes nothing more and is recorded at an epoch of its own. A Purge is refused with code 7 before stage 8
(section 11.2).

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 8 | `tenant` | u64 | |
| 8 | 8 | `epoch` | epoch (section 1) | in a reply, the purge epoch; 0 in a request |
| 16 | 4 | `removed` | u32 | in a reply, the instances removed, an IRI naming none removing nothing; 0 in a request |

| Section | Contents |
|---|---|
| 0 | the term table's records (section 4.4): the instances' IRIs, each of kind 1, at most 10,000 |
| 1 | the term table's text |

Order: unused bytes 20–127 zero; `epoch`, 0 in a request and an epoch in a reply (`FieldValue` `"epoch"`); `removed`, 0
in a request (`FieldValue` `"removed"`); sections; the term table; 1 to 10,000 entries (`CountMismatch` `"purge"`), each
of kind 1 (`FieldValue` `"purge term"`, the entry's kind); `removed` at most the entries' count (`FieldValue`
`"removed"`).

### 8.6 Migrate (25)

A migration's run (`../schema/SCHEMA-EVOLUTION.md` section 6), named by the `staged` identifier its Install answered (section
8.4). A run is a record of the metadata group that moves through its states by committed transitions in the background,
surviving any restart. A Migrate asks for one action: the frontend answers once the metadata group has committed the
transition it asks for, a state read at once, and the client follows the run by reading its state.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 8 | `tenant` | u64 | |
| 8 | 8 | `staged` | u64 | the staged namespace's identifier, at least 1 (`FieldValue` `"staged"`) |
| 16 | 1 | `action` | u8 | 1 start, 2 dry run, 3 cut over, 4 cancel, 5 read the state; else `FieldValue` (`"action"`) |
| 17 | 1 | `state` | u8 | in a reply, the run's state, below; 0 in a request (`FieldValue` `"state"`) |
| 20 | 4 | `refusal` | u32 | in a reply of state 9 for a run a check failed, the UGS refusal number it failed with, 2201 to 2299 for `UGS2201` to `UGS2299`; 0 otherwise, a cancelled run's included (`FieldValue` `"refusal"`) |
| 24 | 8 | `epoch` | epoch (section 1) | in a reply, the epoch the run entered its state at: `e0` for `copying`, `e1` for `fenced`, `e2` for `cut_over`; 0 in a request |

| State | Name | Entered when (`../schema/SCHEMA-EVOLUTION.md` section 6) |
|---|---|---|
| 1 | `staged` | the Install succeeded |
| 2 | `copying` | the run started |
| 3 | `validating` | every partition has copied |
| 4 | `checked` | validation passed; a dry run stops here |
| 5 | `converging` | the cutover was requested |
| 6 | `fenced` | the last unfenced round ended |
| 7 | `catching_up` | the final round began |
| 8 | `cut_over` | the catalog version passing the namespace to the target took effect |
| 9 | `failed` | a check failed, or the run was cancelled before `cut_over`; the staged namespace is discarded |

| Action | Taken in state | Asks for | Answered at |
|---|---|---|---|
| 1 start | `staged` | the run, at snapshot epoch `e0`, through `checked` | `copying` |
| 2 dry run | `staged` | the run, stopping at `checked` for good | `copying` |
| 3 cut over | `checked`, of a run action 1 started | the rounds, the fence and the cutover | `converging` |
| 4 cancel | any before `cut_over` | the staged namespace discarded and the fence, if raised, lifted | `failed` |
| 5 read the state | any | nothing | the current state |

| Section | Contents |
|---|---|
| 0 | the message, UTF-8: in a reply of state 9 for a run a check failed, the refusal's message, naming the instance and the rule as `../schema/SCHEMA-EVOLUTION.md` section 10 states; empty otherwise |
| 1 | the migration report, UTF-8, as the run has brought it: final from `checked`; empty in a request |

Order: unused bytes 18–19 and 32–127 zero; `staged`; `action`; `state`, 0 in a request and 1 to 9 in a reply
(`FieldValue` `"state"`); `refusal`, 0 unless `state` is 9 and then 0 or 2201 to 2299 (`FieldValue` `"refusal"`);
`epoch`, 0 in a request and at most 2⁴² − 1 in a reply (`FieldValue` `"epoch"`); sections; in a request, both
sections empty (`CountMismatch` `"migrate"`); sections 0 and 1 UTF-8 (`NotUtf8`, the section); section 0 empty when `refusal` is 0 (`CountMismatch`
`"message"`).

An action the run's state does not take is refused with code 7 (Refused), `detail` 0, the message naming the state and
the action; a `staged` the tenant does not hold, or one whose run has ended, `cut_over` or `failed`, for any action but
a state read, with code 7, `detail` the identifier. Every action takes `declare` on the tenant, which the gate of
section 11.10 checks first (`../../openspec/notes/SECURITY.md`). While a run is `fenced` or `catching_up`, a write reaching a graph of a
namespace it carries is refused with code 31 (Fenced) and writes nothing; the client runs it again after a delay, and
after the cutover it executes against the target, after a failure against the source as it was. At `cut_over` the
catalog version passing the namespace to the target takes effect as any version does (section 11.13): prepared
statements of the tenant go stale (code 9), and a read-write transaction that executed against the source aborts with
code 2, kind 5 (Catalog). The source stays installed, read-only, as `{namespace}@{version}`, a name that travels, as
every namespace's, inside query texts, in UQL's `use`, GQL's `USE` and SPARQL's graph IRIs, and takes no field (section
1). A Migrate is refused with code 7 before stage 9.5 (section 11.2).

## 9. Bodies of results and control

### 9.1 Result (13)

A columnar, dictionary-encoded batch of a Read, Execute or Query: per column a u32 cell per row, an index into the
batch's term table or `ABSENT` for unbound or null; the term table holding the batch's terms in the encoding of section 10, with the
texts those encodings name but do not hold, so the batch names every IRI and blank node by its text. A row is one
statement the pattern matched, or one property of a statement when the pattern's bound subject is its reifier (section
11.12); the columns are the pattern's unbound pattern slots, in pattern order, then the reifier column. A
row names its statement by its subject, predicate, object and graph, bound in the pattern or in its columns, and, when
the statement holds a statement ID, by its reifier's IRI in the reifier column: a skolem IRI tagged as a blank node (`0x02`), or, for an instance of a
declared relation, the handle IRI of its type tagged as an IRI (`0x01`, section 10.2); a statement without a statement ID has `ABSENT` there. No row carries a statement ID: parallel
edges of one quad are told apart by their reifiers. Rows come partition by partition, in ascending order of the
partitions the read asks as the placement map in effect at its epoch names them, and within a partition in its families'
order (`../storage/STORAGE.md` sections 7.4 and 18.3); no order holds by term or by key (T38, T46). The stream's last batch carries the header flag `LAST`. Only that batch holds a resume key, and only when
its read stopped at `limit` where rows may remain: a partition reads no further to tell, since for a Frontier, whose rows are
recorded as reached, that would widen what a read-write transaction's commit validates (a Scan records its whole family
range on taking it), so a resumed read may answer one batch flagged `LAST` with `rows` 0 and no resume key. Section 6 of
every other batch is empty. A read that fails after rows restarts, or resumes from the last key of a limited read. A read that matches nothing
answers one batch flagged `LAST`, `rows` 0 and `columns` the unbound pattern slots and one more.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 4 | `columns` | u32 | for a quad pattern, the unbound pattern slots and the reifier column, 1 to 5; for a query, its projections, 1 to 1,024, or 0 with `rows` 0 for a GQL statement without `RETURN` or a UQL mutation (`FieldValue` `"columns"`) |
| 4 | 4 | `rows` | u32 | |
| 8 | 8 | `epoch` | epoch (section 1) | the epoch the batch's rows were read at: the snapshot's at level 0, the read's at level 2; 0 for a read-write transaction, level 1 and a named read version |
| 16 | 4 | `partition` | u32 | the partition the read reached when it reached one alone; 0 when it reached several |
| 20 | 1 | `monotonic` | u8 | 0 a snapshot read, or a Read or Execute of a pattern; 1 a monotonic-fast read of a query over data that may have been retracted since; 2 a monotonic-fast read over immutable data |

| Section | Contents |
|---|---|
| 0 | the cells, u32s, column by column: the cell of column c and row r at index `c × rows + r`; `columns × rows` cells |
| 1 | the term offsets, u32s, one per term: term i starts at `offsets[i]` within section 2, the first at 0 |
| 2 | the terms' encodings, back to back; term i ends where term i + 1 starts, the last at the section's end |
| 3 | the term texts: per term two u32s, `text` then `annotation`, each a string index or `ABSENT` |
| 4 | strings: string list `ends` |
| 5 | strings: string list `text` |
| 6 | the resume key |
| 7 | column names: string list `ends`; one name per column on a query's first batch, empty on every other batch and on a pattern's |
| 8 | column names: string list `text` |
| 9 | the warnings (section 4.9): on a Query's first batch, those its compilation reported; on an Execute's first batch, those its Prepare answered; empty on every other batch and on a pattern's |
| 10 | the warnings' messages: string list `ends` |
| 11 | the warnings' messages: string list `text` |

A query's batch holds one row per record of its result, the columns its projections in order: a node, a relation node
or an edge by its IRI, tagged as an IRI or a blank node as the dictionary holds it; a scalar in its term encoding; a
list or a record as an `rdf:JSON` literal; null as `ABSENT`. An update's reply is one batch of one row, the columns
`inserted` and `deleted`.

A term's `text` is what its encoding names by hash: an IRI's text, a blank node's or a reifier's skolem IRI, or the
lexical form of a literal held by hash. Its `annotation` is a language tag held by hash, or the IRI of a datatype held
by hash (section 10.3). A client names a statement with a statement ID again by its reifier's IRI, in Write's
change records 4, 6 and 7 (section 4.6). `epoch` and `partition` tell the frontend, which relays the batch, where a
transaction's later reads start from (section 11.12); neither is a system ID.

Order: unused bytes 21–127 zero; `columns`; `epoch`; `monotonic` 0 to 2 (`FieldValue` `"monotonic"`); sections; offsets
(1) a whole number of u32s; texts (3) a whole number of 8-byte pairs (`SectionLength`); one pair per term
(`CountMismatch` `"term texts"`); the string list (4, 5); the column names (7, 8), empty or one per column
(`CountMismatch` `"column names"`); the warnings (9 to 11, section 4.9);
cells (0) a whole number of u32s; `columns × rows` cells, the product computed in 64 bits (`CountMismatch` `"cells"`);
each cell `ABSENT` or below the term count (`IndexOutOfRange`, section 0); with no terms, section 2 is empty
(`CountMismatch` `"term encodings"`); each term in order: offset 0 for the first, its bytes from its offset to the next
term's offset (or the section's end) hold exactly one whole encoding of section 10.1 (`BadTerm`, section 2, the term);
each text index `ABSENT` or below the string count (`IndexOutOfRange`, section 3); each term's `text` and `annotation` a
string index where section 10.3's table names one and `ABSENT` elsewhere (`FieldValue` `"term text"`, the term's index);
for a pattern's batch, each cell of the reifier column, the last, `ABSENT` or a term of tag `0x01` or `0x02` (`FieldValue`
`"reifier"`); section 6 empty
in a frame not flagged `LAST` (`FieldValue` `"resume key"`).

### 9.2 Credit (14)

More credit for the frame's stream: `frames` more Result, Entries, Neighbours, Describe or Catalog frames may be sent.

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
| 1 | for `OperationFailed`, the value found: a request term table's records (section 4.4), one entry, cut as below when bit 0 of `detail` is set; empty otherwise |
| 2 | that term table's text |

Order: unused bytes 2–7 and 16–127 zero; `code`; sections; the term table (1, 2), at most one entry
(`CountMismatch` `"value"`); section 0 UTF-8. An empty term table means no value: the predicate held none.

A sender builds every Error frame within 524,288 bytes, the floor of `max_frame`, which every receiver takes (section 11.9): a
message longer than that frame leaves it beside the value is cut at its last character boundary within it. So a refusal naming
a long term of its request, or relaying another role's, still reaches its receiver.

The value found comes first: it takes what that frame leaves its term table's text beside an empty message, 522,960 bytes,
and the message takes what the value leaves. A value past that room is cut, and bit 0 of `detail` (value 1, `VALUE_CUT`) says
so; a value carried whole leaves the bit clear, as every other bit of code 8's `detail`, and a receiver refuses an Error of
code 8 with any other bit set (`FieldValue` `"detail"`). A literal, of kind 3 or 4, keeps its
datatype's IRI or its language tag and direction whole, and its lexical form is cut at its last character boundary within the
room they leave: still a literal of the same datatype or language, its form's start. An IRI or a blank node is left out and
the table is empty: a term names its node by its whole text (section 4.4), so a cut one would name another node, or one
nobody holds, which a client could send back as the expected value of its next compare-and-set; a literal whose datatype's IRI
or language tag alone passes the room is left out too. An empty table with the bit clear still means no value: the predicate
held none. A client receiving the bit reads the value again for it whole. A receiver relaying the Error keeps its `detail`,
and the value, already within the room, is not cut again.

| Number | Code | Retryable | `detail` | Meaning |
|---|---|---|---|---|
| 1 | Protocol | no | 0; 1 when a Scan named a transaction the partition does not hold | A frame broke a validation rule: a connection error, after which the server closes the connection, or a stream error, which ends the request's stream (section 2.6). The message is the rule's text |
| 2 | Conflict | yes | the conflict kind below | A transaction's read conflicted with a commit ordered before it |
| 3 | TooOld | yes | 0 | The read version is below what validation still holds, or below the latest logged fold horizon of the partition answering (section 11.9; T48, C34) |
| 4 | PastLimit | yes | the limit in milliseconds | The transaction lived past its limit, the Welcome's `transaction_ms`, or a one-frame write waited that long unsent at the frontend (section 11.12, *Frames on their way*); when a read was holding a write back then, the message names that read's stream and says it had no credit |
| 5 | Busy | yes | 0; 1 delta admission; 2 grid admission; 3, under the object binding, the object store unreachable or the log at its bound for want of a published checkpoint (`../storage/STORAGE.md` section 12.5) | The commit queue is full; or the partition's epoch is full and its close request has gone unanswered for `close_request_ms`, the sequencer counting as unreachable until the close arrives (section 11.12); or an Install found the frontend's compile queue full (section 11.13); or the partition's store admits no commit until compaction or a checkpoint frees room: its delta at its watermark, `detail` 1, or its grid short of free extents (`GridFull`), `detail` 2 (`../storage/STORAGE.md` sections 12.4 and 15.6; T48, C12) |
| 6 | TooLarge | no | the limit in bytes; for statement IDs, their bound | A write set exceeds the Welcome's `max_write_set`, as sent or as its commit-time operations leave it (section 11.9); a transaction's reads exceed what its read set holds; a write would pass `statements_per_object_max`; a write holds a value longer than the Welcome's `value_bytes_max`, `detail` that bound (`ValueTooLarge`, `../storage/STORAGE.md` section 3.3; T48, C12); a Commit reply's frame would pass the floor of `max_frame` (section 11.9); a row or resume key is too large for an otherwise empty Result frame, or an Execute's ComputeRead for a frame (section 13.12) |
| 7 | Refused | no | 0; for a term's rule, the term's index; for a declaration, its record's index, or its index among the transaction's declarations for ones tightening and loosening at once; for a catalog version past its cap, the cap; for an unknown tenant, the tenant; for a Lease, the maximum; for a Catalog, the version; for a schema rule at commit, the UGS refusal number (`../schema/SCHEMA.md` section 8.7); for a Migrate naming a run the tenant does not hold or that has ended, its `staged` | The catalog refused a write, a term broke the server's rules (section 4.4), or a declaration was refused: a vector index in this stage, a transaction mixing declarations with other changes, declarations tightening and loosening at once, a type change the data contradicts, or a catalog version past `catalog_bytes_max`, a Commit's or an Install's (sections 8.4, 11.13); a request named a tenant the metadata group does not hold (section 11.10); or a role's request was refused: a Lease past `block_size_max`, a Catalog of a version never held; or a schema rule refused the transaction at commit, the message naming the instance and the rule; or a Migrate's action does not fit its run (section 8.6); or the frontend does not serve the command in this stage (section 11.2) |
| 8 | OperationFailed | no | bit 0 (`VALUE_CUT`, value 1): the value found is cut or left out to fit the frame; every other bit 0 | A compare-and-set did not hold; sections 1 and 2 hold the value found |
| 9 | StaleStatement | no | the statement | The statement's schema id no longer matches its tenant's |
| 10 | UnknownStatement | no | the statement | The connection holds no statement of this id in the transaction's tenant: never prepared there, or evicted |
| 11 | Parameters | no | the statement; 0 for a parameter type | The parameters do not fit the statement |
| 12 | SessionEvicted | no | 0 | The metadata group's registry evicted the session, or never registered it; the frontend's owner check refuses the request before any partition sees it, and a retried commit refused so has an unknown outcome (section 11.5) |
| 13 | RequestOutOfOrder | no | the last request number the partition holds for the session | A commit's request number is below the session's last in the partition it reaches, or, on an open transaction's stream, at or below it |
| 14 | SessionBusy | yes, after a delay | 0 | The session has a commit in flight, or a retried commit is still in a partition's commit step; or a full registry holds no session it may evict for a Register (section 11.5) |
| 15 | Limit | no | the limit | A bound is reached: past the frontend's connections, an Error on stream 0 in place of the Welcome, then the close; past the Welcome's `max_streams`, on the new stream; past `held_bytes`, on the transaction's stream, ending the transaction (section 11.9) |
| 16 | ProtocolVersion | no | the server's highest protocol | No protocol both sides speak; the server closes the connection |
| 17 | Unavailable | on a read, yes, the read again from its start or a resume key; no otherwise; on a Commit, Drop, DirectWrite, ConditionalWrite or Ingest it leaves the request in doubt (section 11.5) | 0 | A read needs an extent absent from the node that the object store cannot supply (`../storage/STORAGE.md` section 12.5); the system group cannot be reached by the role holding the client endpoint (section 11.12a); the role or partition the request reaches stopped on an invariant violation, refuses writes after a failed log sync, until it restarts, or its role connection closed under the request (section 11.12); no compute process answers a read, or a hung one sent it nothing (section 11.12); a read found a row naming a dictionary entry its partition lacks (section 13.12); or a store failed under a Scan or a home's registration (sections 13.13, 13.15); requests reaching other partitions are served |
| 18 | NotCommitted | yes | the request | A retried commit no partition's session table holds, with no transaction open: the request was not committed, and the client runs its transaction again (section 11.5) |
| 19 | AuthenticationFailed | no | 0 | The authentication exchange failed; the server closes the connection. The message says nothing of what failed |
| 20 | Unauthenticated | no | 0 | A request before the connection authenticated, refused on the request's stream while the connection stays open for authentication; or an authentication not complete within its timeout, an Error on stream 0, after which the server closes the connection (section 11.1) |
| 21 | PermissionDenied | no | the action below | The connection's principal may not do what the request asks, by the tenant gate (section 11.10) or a check at the store; the request is refused, and a transaction's request ends the transaction |
| 22 | CredentialExpired | no | 0 | The connection's credential expired before a new authentication; the server closes the connection |
| 23 | CrossPartition | no | the transaction's partition in bits 0–31, or for an operation bound to none the lowest partition it reaches; the lowest other partition it reaches in bits 32–63 | Cross-partition work, refused in this stage: a read-write transaction reaching a second partition, a first request or one-frame write whose subjects spread over partitions, or a Drop reaching several; it aborts and writes nothing, and the message names every partition it reaches (sections 11.11, 11.13) |
| 24 | TimedOut | yes | the partition waited on | A read waited past its transaction's deadline for a partition to close the epoch it reads at, the message naming the partition and the epoch, and returns no row; or a Scan or Frontier of it stayed past `scan_stall_ms` with no frame able to leave, the message saying so, and rows already sent are no answer (sections 11.11, 13.13); or a declaring Commit waited past `catalog_check_ms` for a partition's check, the change withdrawn and nothing in doubt (section 11.13) |
| 25 | LevelRefused | no | as code 23's | A read-only transaction at level 1 (latest) whose read reaches another partition than the one its first read bound it to, or more than one; the message names the level and the partitions (section 11.11) |
| 26 | WrongPartition | yes, after refreshing the placement map | the bucket in bits 0–23, bits 24–31 zero, the partition owning it in bits 32–63; all 0 for a registration refused at a home, whose owner the registrant does not know | A key outside the receiving partition's bucket ranges, from a sender whose placement map is stale; a later subject of a request whose first lies inside them is code 23 (section 11.12). Between roles; a client receives it when the owner refuses the frontend's one resend too, or from a registration refused at a home (section 13.15), and runs its transaction again after a delay (section 11.12) |
| 27 | AheadOfWatermark | no | the watermark | A read version is above the visibility watermark of the partition answering, in an epoch that partition has not closed: refused at once when the client named it; the message names the version and the watermark (sections 6.2, 13.13) |
| 28 | SchemaRefused | no | the count of diagnostics of kind error | An Install's sources were refused: the message holds the diagnostics of `../schema/SCHEMA.md` section 10.4, each with its code and location, and nothing was installed |
| 29 | QueryRefused | no | the diagnostic's number, with the language in bits 16–23, as the table of languages below gives | A query or prepared text was refused: the message begins with the diagnostic's code (`../query/QUERY-GQL-CORE.md` section 11, `query/QUERY-SPARQL-CORE.md` section 11, `query/QUERY-ULTRA-QL.md` section 18) and its `line:column`, then the reason and the fix; a feature outside the core is named |
| 30 | EntityDeleted | no | 0 | A write names, as its subject, an entity a delete has tombstoned at its home: a commit writing to the deleted entity's ID, or a registration naming it with `mode` 0 (sections 12.2, 13.15); the transaction aborts and writes nothing, and the message names the entity's IRI. A write naming it as a participant of a declared relation, which the get-only registration answers as deleted, is refused with code 7 and `UGS1019` (`../schema/SCHEMA.md` sections 8.5 and 8.7). Run again, a transaction resolves the IRI anew, and one creating the instance creates a new one with a new ID (`../schema/SCHEMA.md` section 8.5) |
| 31 | Fenced | yes, after a delay | 0 | A write reaches a namespace a migration's run has fenced, in its states `fenced` and `catching_up` (section 8.6); it writes nothing, and the message names the namespace and the run's `staged`. After the cutover the write executes against the target. Or a write reaches a tenant whose first schema is installed and not yet in effect (section 8.4): it writes nothing, the message names the tenant, and after the version's effective epoch the write is validated against the schema |
| 32 | TagRequired | no | 0 | From stage 7: a query calls a procedure that needs a tag the compute node answering it does not carry (section 11.14). A Prepare and an Execute that compile such a query are refused alike, and a refused Query leaves its transaction open. The message begins with the tag the procedure needs, then names the procedure and the node's tags. A driver hands the refusal to its caller and moves the request to no other node |

Retryable says the same work can succeed if run again, and the code says what runs again. Code 14 on a Commit, Drop or
one-frame write, and code 5 on a Drop or one-frame write: the same request, same session and request number, on a new
stream after a delay; code 14 on a Register: the Register, after a delay. Codes 2, 3, 4, 18, 26, and 5 on a Commit: the
whole transaction on a new Begin, since its stream has ended or its read version is fixed. Code 24: the read, in the
same transaction or a new one; past the deadline a read of an unclosed epoch is refused again at once, so a new wait
takes a new Begin; a declaring Commit's, the whole transaction on a new Begin. Code 17 on a read: the read, in the same
transaction or a new one, which another compute process or the restarted role may answer. A read that already gave rows
runs again from its start or from a resume key, and its earlier rows are discarded. Code 31 runs again as code 5 does,
after a delay: a migration's fence holds for its run's final round, which the migration report estimates, and a first
schema's until its version takes effect, two closes after its Install is committed. Code numbers are never
reused. A receiver refuses an Error of code 26 with any of bits 24 to 31 of its `detail` set, and one of code 8 with any
bit but bit 0 set (`FieldValue` `"detail"`).

| Action (`detail` of code 21) | Asked for |
|---|---|
| 1 | `read` |
| 2 | `write` |
| 3 | `declare` |
| 4 | `drop` |
| 5 | `admin` |
| 6 | `operate` |
| 7 | `purge` |

| Conflict kind (`detail` of code 2) | Read |
|---|---|
| 1 | unassigned |
| 2 | Statement: a statement tested |
| 3 | Range: a key range scanned |
| 4 | AbsentTerm: a term found absent, recorded as no statement of it in the transaction's partition |
| 5 | Catalog: the tenant's catalog: the transaction executed against another catalog version than the one in effect at its commit epoch, or its declarations were built on a version no longer current (section 11.13) |
| 6 | Drop: a range a drop wrote |

| Language (bits 16–23 of a diagnostic's number) | Diagnostics |
|---|---|
| 1 | GQL: `GQLnnnn` (`../query/QUERY-GQL-CORE.md` section 11) |
| 2 | SPARQL: `SPQnnnn` (`../query/QUERY-SPARQL-CORE.md` section 11) |
| 3 | UQL: `UQLnnnn` (`../query/QUERY-ULTRA-QL.md` section 18) |
| 4 | UGS: `UGSnnnn` (`../schema/SCHEMA.md` section 10.4, `schema/SCHEMA-EVOLUTION.md` section 10) |

A diagnostic's number, in bits 0–15, is its code's four digits: `GQL0210` is 210 in language 1. Code 29 and a warning
(section 4.9) carry it so.

## 10. The term encoding of results

A Result batch holds each term as a tag byte and its payload, in the store's own value encoding, with two differences.
Where the store names an IRI, a blank node or a datatype by its system ID, a Result names it by the hash of its text,
and the batch's string list holds the text (section 9.1), so no system ID reaches a client. And a Result tells a blank
node from an IRI by its own tag, `0x02`. The tag fixes the encoding's length, or where to read it. A vector's encoding
is `3 + 4 × dimension` bytes, a bfloat16 vector's `3 + 2 × dimension`; every other encoding is at most 33.

### 10.1 Tags and lengths

"Inline text" is a length byte `n` (0 to 15) then `n` bytes of UTF-8. "Hash" is the first 16 bytes of the SHA-256
digest of the text's UTF-8 bytes: the result batch's string list supplies the text it names.

| Tag | Term | Payload after the tag | Length |
|---|---|---|---|
| `0x01` | IRI | hash of the IRI | 17 |
| `0x02` | blank node | hash of its skolem IRI | 17 |
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
| `0x32` | literal of another datatype, lexical form ≤ 15 bytes | hash of the datatype's IRI, then inline lexical form | 18 + n |
| `0x33` | literal of another datatype, longer | hash of the datatype's IRI, then hash of the lexical form | 33 |
| `0x40` | `https://w3id.org/ultragraphdb/ns/float32vector` | dimension u16 **little-endian**, 1 to 65,535; then `dimension` f32s **little-endian** | 3 + 4 × dimension |
| `0x41` | `https://w3id.org/ultragraphdb/ns/bfloat16vector` | dimension u16 **little-endian**, 1 to 65,535; then `dimension` bfloat16s **little-endian**, each the high 16 bits of an IEEE 754 binary32 | 3 + 2 × dimension |
| `0x50` | `https://w3id.org/ultragraphdb/ns/measure` by value, unit code ≤ 15 bytes | 8 bytes, then the unit's code as inline text (section 10.5) | 10 + n |
| `0x51` | `https://w3id.org/ultragraphdb/ns/precision` by value | 1 byte | 2 |

The three tags of each language-tagged row are for the base direction: the first none, the second `ltr`, the third
`rtl`. A language tag over 15 bytes hashes the lexical form too, whatever its length.

An encoding is whole when its tag is listed, each inline length is at most 15 and its text UTF-8, a boolean's byte 0 or
1, a date or time's timezone offset within 14 hours, a duration's months and seconds not of opposite signs, a known
datatype's code in section 10.4, a vector's dimension at least 1, a measure's unit code at least 1 byte, a precision's
byte 1 to 12, and the bytes hold exactly the length the table gives. A Result receiver checks this per term (section
9.1).

### 10.2 How the server chooses the encoding

| Term | Encoding |
|---|---|
| IRI | `0x01`, with the IRI's text as the term's `text`; a keyless instance's handle IRI, drawn or derived, among them (section 4.4) |
| Blank node | `0x02`, with its skolem IRI (RDF 1.1 §3.5) as the term's `text`: the tenant's skolem base, `https://w3id.org/ultragraphdb/ns/tenant/{n}/`, then `.well-known/genid/{bucket}-{random}`, `{n}` the tenant in decimal, `{bucket}` the node's bucket in 6 lowercase hexadecimal digits and `{random}` 104 random bits in 26, both zero-padded. The partition minting the node draws both from its random source, the bucket among those it owns in the tenant's run, and draws again when the IRI names one its dictionary holds; the IRI holds no part of the node's system ID beyond its bucket. A request names the node again by that IRI, as an IRI (section 4.4) |
| The reifier of an edge-encoded instance of a declared relation (`../schema/SCHEMA.md` sections 7.3 and 11.1), in the reifier column and wherever a term names it | `0x01`, with the instance's handle IRI, `{base}{Type}/{handle}`, as the term's `text`: an instance has one name, and its reifier is its identity. The handle states the statement's bucket, its subject's, as a skolem IRI does |
| The reifier of any other statement, in the reifier column (section 9.1) | `0x02`, with the reifier's skolem IRI as the term's `text`, of the same form: `{bucket}` the statement's bucket, its subject's, and `{random}` 104 random bits the subject's partition draws, drawn again when the IRI names one its dictionary holds. The dictionary holds the IRI as the reifier's name, mapping it to the statement ID and back; the IRI holds no part of the statement ID beyond its bucket. A request names the statement again by that IRI, as an IRI (sections 4.4, 4.6) |
| `xsd:string` literal | `0x20` up to 15 bytes, else `0x21` |
| Language-tagged literal | by tag length, then lexical form length, as the table gives |
| Literal of datatypes `0x10` to `0x1F` whose lexical form is the canonical form of its value (section 10.5) and whose value the payload holds exactly: an integer within i64, a decimal of at most 18 fraction digits within the i128 payload, a date, time or duration within its payload, and no timezone offset of −00:01, whose minutes would read as the payload's "no timezone" | the by-value tag |
| `float32vector` literal in canonical form: `[`, the elements separated by `, `, `]`, each element the shortest decimal that reads back as the same finite f32, without exponent | `0x40` |
| `bfloat16vector` literal in canonical form: as `float32vector`'s, each element the shortest decimal that reads back as the same finite bfloat16 | `0x41` |
| `measure` literal in canonical form (section 10.5) whose unit code is at most 15 bytes | `0x50` |
| `precision` literal in canonical form (section 10.5) | `0x51` |
| Any other literal of a known datatype, a by-value datatype's non-canonical form or a value its payload does not hold included | `0x30` up to 15 bytes, else `0x31` |
| A literal of any other datatype | `0x32` up to 15 bytes, else `0x33` |

Equal terms encode equally and unequal terms unequally.

### 10.3 Texts and annotations

| Tag | `text` | `annotation` |
|---|---|---|
| `0x01` | the IRI | |
| `0x02` | the blank node's or the reifier's skolem IRI | |
| `0x21`, `0x23`, `0x26`, `0x29`, `0x31` | the lexical form | |
| `0x24`, `0x27`, `0x2A` | the lexical form | the language tag |
| `0x32` | | the datatype's IRI |
| `0x33` | the lexical form | the datatype's IRI |
| every other tag | | |

### 10.4 Known datatypes

A known datatype is named by a one-byte code in tags `0x30` and `0x31`; codes 1 to 16 are also the by-value datatypes,
and codes 32 and 34 to 36 hold their values by tags of their own, `0x40`, `0x41`, `0x50` and `0x51` (sections 10.1,
10.5).
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
| 34 | `ugdb:bfloat16vector` | 35 | `ugdb:measure` | 36 | `ugdb:precision` |

`ugdb:bfloat16vector` holds a `Vector<BF16, n>`, `ugdb:measure` a number with its unit and `ugdb:precision` a temporal
bound's precision (`../schema/SCHEMA.md` sections 5.1, 5.5, 5.8 and 12.2).

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
| `0x15`–`0x1C` | i128 `T`, then i16 `Z` | `T`: the value's `timeOnTimeline` in seconds × 10¹⁸ (below); `Z`: the timezone offset in minutes, −840 to 840 (−14:00 to +14:00, XML Schema's range), or `0xFFFF` for none; any other offset is no whole encoding |
| `0x1D` | i64 months, then i128 seconds × 10¹⁸ | the duration's year-month part and day-time part, of one sign where both are nonzero: a duration is negative as a whole (its lexical form below), so parts of opposite signs are no whole encoding |
| `0x1E` | i64 | months |
| `0x1F` | i128 | seconds × 10¹⁸ |
| `0x50` | f64, IEEE 754 binary64, then inline text | the measure's number in its own unit, then that unit's UCUM code, a declared unit's (`../schema/SCHEMA.md` section 5.8) |
| `0x51` | u8 | the precision, coarsest first: 1 millennium, 2 century, 3 decade, 4 year, 5 month, 6 day, 7 hour, 8 minute, 9 second, 10 millisecond, 11 microsecond, 12 nanosecond (`../schema/SCHEMA.md` section 5.5); any other byte is no whole encoding |

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
| measure | the number in double's form, one space (U+0020), then the unit's UCUM code: `1.5 m`, `-40 Cel` |
| precision | the precision's name: `millennium`, `century`, `decade`, `year`, `month`, `day`, `hour`, `minute`, `second`, `millisecond`, `microsecond`, `nanosecond` |

A `Geo` value (`../schema/SCHEMA.md` section 5.7) is held by no by-value tag: the store keeps its GeoJSON geometry in the
canonical form of RFC 8785 (JCS), and a Result gives it as a `geo:geoJSONLiteral` of tag `0x30` or `0x31` whose
lexical form is that canonical JSON, so equal JSON values encode equally (T48, C27).

## 11. The connection

### 11.1 Handshake and authentication

1. The client opens a TCP connection to the frontend's client endpoint. An endpoint whose `[endpoint]` table names a
   certificate and its key (`tls_cert_file`, `tls_key_file`) speaks TLS 1.3 from the connection's first byte, and only
   TLS 1.3, with no clear-text upgrade; one that sets `plaintext = true` speaks clear text (`../../openspec/notes/SECURITY.md`,
   *Encryption*). A client configured with a trust source runs the TLS handshake first and verifies the server's
   certificate for the host it dials before it sends a byte of the protocol; a clear-text Hello reaching a TLS endpoint
   fails as a TLS handshake failure, and no frame is read. The client then sends Hello on stream 0: header `sequence`
   1, `cluster` the cluster it is configured for, `release` its release, `protocol` 1, every other field 0; command
   header the protocols it speaks and its `max_frame`.
2. The server validates the frame. A frame breaking a rule, or a first frame of another command, is answered with an
   Error of code 1 (Protocol) on stream 0, and the server closes the connection.
3. When `protocol_min` exceeds the server's highest protocol, the server answers an Error of code 16 (ProtocolVersion),
   `detail` the server's highest protocol, on stream 0, and closes the connection. When the frontend already holds as
   many connections as it allows (section 11.9), it answers an Error of code 15 (Limit), `detail` the bound, on stream 0
   in place of the Welcome, and closes the connection; the connections it holds go on.
4. Otherwise the server answers Welcome on stream 0, its `protocol` the lesser of its highest protocol and
   `protocol_max`. A server speaks every protocol from 1 to its highest. The Welcome's header carries `protocol` 1 and
   the server's `release`; its command header the server's limits and `auth`, and its sections the mechanisms.
5. The client validates the Welcome and refuses a `protocol` outside its Hello's range (`FieldValue` `"protocol"`),
   closing the connection.
6. With `auth` 1, which the frontend's client endpoint always answers, the client authenticates: an Authenticate
   exchange on stream 0 (section 5.3) by a mechanism the Welcome listed, `SCRAM-SHA-256`, the same inside TLS as in
   clear text, against a principal the metadata group holds; a cluster holding no principal refuses every exchange. The
   server's final Authenticate names the principal the connection acts for and when its credential expires. A refusal
   is an Error of code 19 (AuthenticationFailed) on stream 0, the same for an unknown name, a wrong proof and the role
   secret, and the server closes the connection. A request on a stream other than 0 before the final Authenticate is
   refused with an Error of code 20 (Unauthenticated) on the request's stream, which ends it, and the connection stays
   open for authentication. An exchange not complete within the authentication timeout (section 11.9) is answered with
   an Error of code 20 on stream 0, and the server closes the connection. The frontend counts each source's failed
   authentications and throttles, then bans, a source past them (section 11.9): a banned source's connection is closed as it
   is accepted, before TLS or any frame. With `auth` 0, which a test server alone
   answers, nothing is exchanged, and the connection acts for the anonymous principal (`../../openspec/notes/SECURITY.md`).

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
  15 (Limit), `detail` the limit. A request on stream 0 other than Hello and Authenticate, or a new request on a
  stream already open, is a connection error (section 2.6). A server keeps the highest number the connection opened, so a
  frame on a lower number that holds no open stream is late and it drops the frame, a Commit and a Drop included.
- The server answers streams independently, in whatever order their results are ready.
- A stream ends with its frame flagged `LAST`, when the client cancels it, or with an Error on it. Every reply that
  ends its stream carries `LAST`: Register, Commit, Prepare, Install, Drop, Purge and Migrate replies, a Describe's last
  frame, a read's or query's last Result, and every Error. Begin's reply does not: the transaction's stream stays open.
- Each side numbers the frames it sends on the connection, its `sequence`, from 1 and one more for each frame. A
  receiver refuses a frame whose sequence is not one above the previous frame's from the same side: a connection
  error (`SequenceOrder`), which a lost, repeated or reordered frame cannot pass. A frame of a stream that has ended is
  late; the Rust client drops it.

| Request | Stream | Reply |
|---|---|---|
| Authenticate | stream 0 | Authenticate, until the server's final one; or Error 19, closing the connection |
| Register | a new stream | Register with the session's identity, or Error; the stream ends |
| Begin | a new stream, which becomes the transaction's | Begin with `read_version` filled, and the stream stays open; or Error, and the stream ends |
| Read, Execute, Query | a new stream, naming the transaction | Result frames, the last flagged `LAST`; or Error |
| Write, Operation | the transaction's stream | none; a refusal is an Error on the stream, which ends the transaction and answers its next request |
| Commit | the transaction's stream; a retry goes on a new stream of its own | Commit or Error; the stream ends |
| DirectWrite, ConditionalWrite | a new stream; a retry goes on a new stream of its own | Commit or Error; the stream ends |
| Ingest | a new stream; a retry goes on a new stream of its own | Ingest or Error; the stream ends |
| Abort | the transaction's stream | none; the stream ends |
| Prepare, Install, Drop, Purge, Migrate | a new stream | Prepare, Install, Drop, Purge or Migrate, or Error; the stream ends |
| Describe | a new stream | Describe frames, the last flagged `LAST`; or Error |
| Credit | the read's stream | none |
| Cancel | a read's stream, or the transaction's | none |

A Write, Operation or Abort on a new stream, and a Commit through no session on a new stream, is refused with Error 1
(Protocol) on that stream, the message naming the command. A Write, Operation or Commit on a read-only transaction's
stream is refused the same way, ending the transaction. A Write, Operation or Commit on a transaction's stream after its
Commit is late, and the frontend drops it: the Commit decides the transaction, and its reply ends the stream. On a role endpoint, a Write, Operation, Commit or Abort on a stream
no Bind of kind 1 opened, and a Drop on one no Bind of kind 2 opened, is refused with Error 1 the same way.

In this stage the frontend refuses an Install, a Query and a Prepare of `language` 1 to 3 with code 7 (Refused) on the request's
stream, `detail` 0, the message naming the command and the stage that serves it: stage 6.6 for an Install and for a Query of
`language` 3, which runs UQL's CRUD subset from then (`../query/QUERY-ULTRA-QL.md` section 4.1); stage 7 for a Query and a text of
`language` 1 or 2; and, for a Prepare of `language` 3, the stage that builds UQL whole (`../../ROADMAP.md`). It refuses the same way a Purge, naming stage 8 (`../schema/SCHEMA.md` section 14), and a Migrate, an Install holding a
migration source and an Install into a tenant that holds a schema, naming stage 9.5 (`schema/SCHEMA-EVOLUTION.md`,
`schema/SCHEMA.md` section 14). A request naming a tenant passes the gate of section 11.10
first. A Query's refusal leaves its transaction open.

A transaction is its stream and its reads. Its stream takes one request at a time; its reads run beside it, each on a
stream of its own, at the transaction's read level and snapshot (section 11.11). A Commit or an Abort ends the reads still
open when the frontend takes it, as a Cancel ends them (section 11.4; section 11.12, *Reads*, orders them against held
writes): a client that wants a read's every row reads it to its `LAST` first, as SQL closes a transaction's cursors when
it ends. A write behind an open read waits for that read's `LAST`, and a read behind that write waits for the write, and only the
client's Credit drains the open read: a client reads an open read to its `LAST`, or cancels it, before it waits on a later read
behind a write. When the transaction ends by an Error on its stream, the frontend answers each read still open with that Error,
which a client that has already seen the transaction's Error drops as late, and a read naming the transaction that
arrives before that Error has left with the same Error. The Rust client sends no Write, Operation or
Commit in a read-only transaction (Begin `mode` 1).

### 11.2a Request types

A request declares its type by the command that carries it, so the server chooses its path before it reads a term:

| Type | Commands | What it costs |
|---|---|---|
| Read-only | Begin `mode` 1, then Read or Execute | No read set, no commit; a level names how far it may lag (section 11.11) |
| DirectWrite | DirectWrite (section 6.6) | One frame and its reply: no Begin, no Bind stream, no open transaction, and nothing read reaches the client |
| ConditionalWrite | ConditionalWrite (section 6.7) | As DirectWrite, its conditions decided in commit order, and the outcome reported in the reply |
| Ingest | Ingest (section 6.8) | As DirectWrite, inserts alone, by blocks; one atomic share per partition, each share's outcome in the reply |
| Read-write | Begin `mode` 0, then any of Read, Execute, Write, Operation, Query, and Commit or Abort | An open transaction, validated at commit, and aborted to the client on a conflict |
| Query | Begin `mode` 1 then Query of a statement without a writing clause; Begin `mode` 0 then any Query | As Read-only or Read-write by the transaction; a statement with a writing clause and no reading clause, whose writes the catalog's request-typing facts classify coordination-free (`../schema/SCHEMA.md` section 9.6), is executed as a DirectWrite's changes are, in one turn at the watermark |

A write computed from an earlier read of the client's belongs in a read-write transaction, or in a commit-time operation that
decides it in commit order, which ConditionalWrite carries in one frame (section 11.6). Sent as a read-only request and then a
DirectWrite, it is two independent requests, and a commit between them is a lost update the server cannot see.

### 11.3 Credit

Each read's stream, and each Describe's and Catalog's, starts with the Welcome's `initial_credit`. The server sends a
Result frame on a stream only while the stream's credit is above 0, and each Result frame it sends spends 1. A Credit
frame on the stream adds `frames` (saturating at 2³² − 1). Only Result, Entries and Neighbours frames, and the frames of
a Describe's, a Catalog's or, from stage 5.8, a Defaults' answer (sections 8.3, 13.7, 13.28), spend credit; an Error and
every other frame leave it. The server ignores Credit for a stream that is not open. A receiver of frames that spend
credit, always the side that dialled, refuses a frame past its stream's credit as a protocol violation, code 1
(Protocol) by the rule `PastCredit` naming the stream, and closes the connection, sending no Error, which only the side
that accepted sends (sections 2.6, 3). The Rust client's credit is automatic: its shells grant credit as the read's
batches are consumed, when the count reaches 0 and the caller waits for a batch, and offer no manual grant; its core
grants at least one frame, so it sends no Credit of zero frames, which a receiver refuses (section 9.2). Its credit and
its frame buffers agree: it sends a read only while its buffers hold, beside what its other reads may bring, every frame
the read's credit and a refusal can bring, one buffer left to read into, and refuses one past them before it is sent, so
a read not yet consumed never takes the buffer a reply needs; a client whose buffers are all held reads nothing until
one is given back, which holds the server back and loses no connection. It sends a Register answered code 14 again
itself, on a new stream after the retry delay and within the attempts of a commit's resend, and then returns that code
14 (sections 9.4, 11.5). On a role connection, credit holds the same way for Entries and Neighbours frames as for Result
frames, each connection's streams on their own, and a ComputeRead's stream starts with its `credit` in place of the
Welcome's (section 13.12).

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
- A role sets `TCP_NODELAY` on every TCP connection it dials or accepts, so a frame leaves as it is written: a frame held
  behind an unacknowledged one, a Cancel or a Scan's next Entries, would wait on the peer's delayed acknowledgement.

### 11.5 Sessions and exactly-once commits

- A client registers a session with Register (`session` all zero) and receives its identity: 128 random bits the
  frontend draws, which the metadata group records with the connection's principal, one command per session and none
  per commit, durably before the reply. The session belongs to that principal. A connection holds as many sessions as
  it needs concurrent writers; reads need no session. A Register whose reply is lost leaves a session no client uses,
  which ages out of the bounded registry; the client registers again.
- The client numbers a session's Commits and Drops from one counter: the first request is 1, and each is one above the
  session's last committed request. One counter serves every partition, since a client cannot know which partition a
  commit reaches. After a Commit or Drop reply the counter is one higher. After a refusal saying the request wrote nothing
  it stays, and the next commit reuses the number. Code 17 says no such thing: the request is in doubt, as after a lost
  connection, and the client resolves it by the in-doubt resend, with the same request number, before the session commits
  again. A new request refused with code 13 moves the counter above `detail`, and the client runs its transaction again.
- The frontend checks every commit and drop through a session against the metadata group's registry before it routes
  it, so no partition sees a refused one: a session the registry does not hold, evicted or never registered, is
  refused with code 12 (SessionEvicted); another principal's, with code 21 (PermissionDenied), `detail` 2, `write`.
  Only the session's principal commits through it or reads its stored reply.
- One commit is in flight per session; the frontend refuses a second with code 14 (SessionBusy), which a client retries
  after a delay.
- Each partition keeps a session table of its own, keyed by the session's 16 bytes alone (T48, C3). It records a session
  at its first commit there, with the last request number committed there, the share it was for an Ingest share, and
  its whole reply: the reply's command, its 128-byte command header and its body, a Commit, Drop or Ingest reply, with
  the commit version, the session token and every commit-time operation's result, its nodes by their text
  (`../storage/STORAGE.md` section 9.3, kind 10). A partition
  takes any request number above its last for the session, so the numbers the session committed in other partitions
  leave gaps it accepts; it refuses a number below its last with code 13 (RequestOutOfOrder), `detail` its last.
- A retry of a partition's last committed request, with the same session and request number, returns the stored reply
  byte for byte, its command, token and results included, and commits nothing again. A retry is a Recall or a one-frame or
  Drop resend, never a Commit on an open transaction's stream. The partition refuses a Commit on an open transaction
  numbered at or below its last for the session with code 13, `detail` its last, and aborts the transaction, so a reused
  number never reads another transaction's reply.
- **The in-doubt resend.** A resend of a DirectWrite or ConditionalWrite goes as section 6.6 states. A resend of a Drop goes as
  a first Drop does (section 11.12, *Drops*), to the one partition it reaches, since its frame holds what it does, and the table
  decides as in section 6.6: the stored reply for its last request, code 14 for a request in its commit step, code 13 below its
  last, and otherwise it commits. A client that sent a Commit and lost its reply, or was answered code 17, sends it again, with
  the same session and request number, on the next connection, on a new stream of its own, without a transaction. When the
  frontend still holds the original in flight, it answers code 14. Otherwise it asks the partition the original
  reached when it knows it, and every partition of the cluster when it does not, the open transaction gone with its
  connection, with Recall (section 13.11). A partition answers the stored reply when its table holds the request as
  the session's last; code 14 when the request is in its commit step, not yet decided; code 13 when its last for the
  session is above the request; and code 18 (NotCommitted) when it holds neither. The frontend answers the client, in
  this order: a stored reply any partition gave; code 14 when any answered 14; code 17 when any answered 17 or a refusal not
  named here, which is a partition that could not be asked, or its role connection closed or could not be opened before it
  answered; code 13 when any answered 13, `detail` that partition's
  last; and, every partition having answered 18, code 12 when the registry has evicted the session
  meanwhile, since its partitions may have purged its entries, else code 18: the request was not committed, and the
  client runs its transaction again.
- The registry and the tables are durable: a session's registration is durable in the metadata group before Register
  answers, and each committed request with its reply is in its partition's round with the commit (section 12.2), and
  each eviction in the round that applies it (section 12.1), so replay rebuilds a table exactly as it rebuilds the commit.
- The registry holds at most `sessions_max` sessions, evicts the least recently registered session with no commit in
  flight, and is the only place a session is evicted. A Commit, Drop or one-frame write is in flight from the owner check
  until it ends before reaching a partition, its partition answers it, or the role connection carrying it closes, whether
  or not the client's connection stays open, so a session the owner check passed is never evicted before its request has
  reached its partition; a Recall records nothing in a table and holds no session from eviction. When a full registry
  holds no session it may evict, the frontend refuses Register with code 14 (SessionBusy), evicting nothing, and the
  client registers again after a delay. The sequencer carries each eviction to every partition with its next close
  (section 13.4), and once that close is delivered to it a partition names the session in the evictions of the first
  round it logs after the close (section 12.1), which deletes the session's entry at that round's `first_version` on every
  replica and in replay alike, so a table holds at most `sessions_max` entries (T48, C3). The frontend's owner check refuses every request of an evicted session with code 12, so no partition takes
  an evicted session for one committing there first. A retry refused with code 12 has an unknown outcome: the client
  reports it as unknown and does not retry.
- A commit through no session, `session` all zero and `request` 0, commits without exactly-once: a lost reply, or code
  17, leaves its outcome unknown.
- **A commit in flight is in doubt once sent.** When the connection is lost or closed before a commit's reply, the Rust
  client answers `ConnectionLost` if the commit's frame was never sent on any connection, and `InDoubt` if it was sent,
  on any attempt; a retry that exhausts its attempts answers `InDoubt` too, a connection not made, or lost before its
  handshake ended, using one of the attempts of every commit waiting for it, so a server refusing every handshake, with
  code 15 or 19, ends the retries; a Commit, Drop or one-frame write answered
  code 17 is in doubt the same way, since the partition may hold it. The client resolves an in-doubt commit through its
  session by the in-doubt resend above. The stored reply says it committed and code 18 that it did not. Code 14 says it
  is undecided, and code 17 that a partition could not be asked: it stays in doubt and the client resends after a delay.
  Codes 12 and 13 leave it unknown for good; after 13 the counter moves above `detail`. A resent one-frame write or Drop
  has no code 18: a Commit or Drop reply says it committed, codes 12, 13, 14 and 17 read as above, codes 21 and 23, which
  the frontend gives before any partition's table decides, leave it in doubt, and any other refusal says it did not.

### 11.6 Commit-time operations

A transaction sends any number of Operation frames before its Commit. The Commit reply's results hold one 16-byte
result per operation, in the order the operations were sent (section 6.3): for kinds 1 and 2, the node, named by its
IRI or skolem IRI through the reply's term table, and whether the operation created it; for kind 4, the counter's new
value; for kinds 3, 5 and 6, `node` `ABSENT`, `value` 0 and `created` 0. No result holds a system ID. A compare-and-set that does
not hold refuses the commit with code 8 (OperationFailed), the value found in the Error's term table. Kinds 1 and 2
claim their value at the partition owning its bucket; a claim another partition owns aborts the transaction with code
23 (section 11.11). Kinds 5 and 6, an instance's delete and
retraction, refuse the commit with code 7 and the schema's refusal number where a rule of the schema forbids them
(section 7.3).

### 11.7 Prepared statements and the schema id

- A statement is prepared on a connection, which holds it; its id starts at 1 and is never reused on the connection.
  The connection's cache holds a bounded number of statements and evicts the oldest; executing an evicted statement is
  refused with code 10 (UnknownStatement).
- Preparing refuses a parameter declared `2` (any term) at a graph, subject or predicate slot with code 11
  (Parameters), `detail` 0.
- A statement prepared from a query text is compiled against its tenant's current catalog version, its names
  resolved and its parameters typed as the core's static typing requires (code 29 for a refusal); Execute binds its
  parameters by position in the order the Prepare named them.
- Execute refuses, in this order: an unknown statement, or one prepared in another tenant than its transaction's (code
  10), a tenant binding per Begin and not per connection; a statement prepared against another schema id than
  its tenant's current one (code 9, StaleStatement), after which preparing it again succeeds; parameters that are not
  one per parameter slot (code 11); a literal bound at a slot of column type 1 (code 11). Each `detail` is the statement
  id.
- The schema id is XXH3-64, seed 0, of the Describe body (the data before any chunk table; an answer of several frames
  read as one body) that holds the tenant's declarations and entries alone, as the connection's principal sees them:
  sections 2, 3 and 4 empty, the strings only the declarations' names, interned in record order, arg by arg, then the
  entries' names and the retired names' current names, in entry order, and the declarations and entries in Describe's
  order (section 8.3).
  It covers the tenant's whole catalog, every namespace installed in it, so it changes with each catalog version of the
  tenant and with what the principal may read, and not with the data or a recorded default entry (section 11.13). For the golden `describe_reply.frame`, whose predicates `knows` and
  `name` come in their IRIs' order and which holds no entry, it is `0x5FC0D2C82332781C`.

### 11.8 Resume keys

The resume key of a stream's last batch continues its own request: the same Read or Execute, in the same transaction or,
after a lost connection, in a read-only transaction at level 0 naming the version the first Begin answered, with the
same pattern and parameters, sent on a new stream with the key in its resume section. Rows come in section 9.1's
order, which a snapshot fixes, so a resumed read neither repeats nor skips a row at the transaction's snapshot; at levels 1 and 2, which hold no
snapshot, each page reads the versions its own Scans choose. A resume key is a token compute
alone writes and decodes, opaque to the client: it holds the pattern's hash, what binds it to its transaction and the
last row's terms in their text form, never a store key or a system ID, under an XXH3-64 checksum. Compute refuses, with
Error 1 (Protocol) on the request's stream, the message naming the resume key, a key whose checksum does not hold, that
does not decode as below, or whose pattern hash or `scope` is not the request's. A named version below a partition's
latest logged fold horizon answers code 3 (TooOld) as any read of it does (section 13.13).

Its layout is the body layout of section 4.1 with a 32-byte fixed part and four sections; offsets count from the key's
first byte:

| Fixed part | Width | Field | Type |
|---|---|---|---|
| 0 | 8 | `checksum` | u64: XXH3-64, seed 0, of the key's bytes from 8 to its end |
| 8 | 8 | `pattern` | u64: the pattern's hash, XXH3-64, seed 0, of ComputeRead's sections 0 to 2 as the frontend builds them for the request: the pattern with its parameters bound, and its term table (section 13.12) |
| 16 | 8 | `scope` | u64: for a read-write transaction, XXH3-64, seed 0, of ComputeRead's `transaction`; for a read-only one, ComputeRead's `read_version`, 0 at levels 1 and 2 |
| 24 | 4 | `partition` | u32: the partition that answered the last row, at least 1 |
| 28 | 4 | zero | 0 |

| Section | Contents |
|---|---|
| 0 | the last row: five u32 indices into the key's term table, its graph (`ABSENT` for the default graph), subject, predicate, object and reifier (`ABSENT` for none), bound positions included |
| 1 | the term table's records (section 4.4), a blank node or a reifier as an IRI entry holding its skolem IRI, or a declared relation's instance its handle IRI |
| 2 | the term table's text |
| 3 | the last row's ordering value: the sortable encoding of the inline column its family is ordered by (`../storage/STORAGE.md` section 7.4); empty for a family ordered by neighbour |

Compute checks, in order: the checksum; the body layout (section 4.1); bytes 28–31 zero (`PaddingNotZero`); `partition`
at least 1 (`FieldValue` `"partition"`); section 0 of 20 bytes (`CountMismatch` `"resume row"`); the term table (section 4.4); each index below the table's length, `ABSENT` only for the graph and the
reifier (`IndexOutOfRange`); the pattern hash; the `scope`. It passes over the partitions before `partition`,
resolves the last row's terms in `partition` and builds from them and the ordering value the store's resume key
(`../storage/STORAGE.md` section 18.3), which continues strictly after that row, its statement included, and reads
every later partition from its start. A statement's record gives its
properties in the order of predicate ID, then value encoding, and compute drops those up to and including the last row's in that
order, so a property removed since is passed all the same.

### 11.9 Limits

The Welcome states the limits a client must know before it sends: `max_frame`, `max_streams`, `initial_credit`,
`max_write_set`, `transaction_ms` and `value_bytes_max`. A transaction is bounded by both: its write set within `max_write_set`, refused
with code 6 (TooLarge) otherwise; its life within `transaction_ms`, refused with code 4 (PastLimit) otherwise. A load
larger than one transaction is many transactions. The frontend times a read-write transaction from its Begin's arrival.
Past `transaction_ms`, unless its Commit has already been relayed, it cancels the Bind stream, if any, and answers code 4
on the transaction's stream. A partition's own timer, started at the Bind's arrival, ends a transaction whose frontend no
longer speaks for it.

A transaction's write set is its write-set record rounded up to 8 bytes and the dictionary entries its commit adds to
its round (section 12.1). `max_write_set` is never more than a round of a partition's log holds on its own: the Round
frame of one committed record and those entries, its header, section table, decision, record entry and chunk table
counted, within the log's round bound. A round bound of 1,048,576 bytes holds a write set of 1,046,216, and one of
262,144, a log's on a drive with power-loss protection, a write set of 261,320. The frontend, which hosts no log,
announces at most the smallest write set the logs of the partitions it routes to hold: 261,320 when any of them runs on
a drive with power-loss protection. A write set the frontend takes when it arrives therefore always fits its partition's
round. The record a transaction's commit-time operations leave is held to the same bound: one over it is refused with
code 6 when the transaction is decided, `detail` the bound, taking no commit version and writing nothing; one within it
that its round has no room for waits for the next round. A transaction or one-frame write whose Commit reply's frame would
pass 524,288 bytes, the floor of `max_frame`, is refused the same way when it is decided, code 6, `detail` the bound, since its
stored reply answers a resend on any connection.

The frontend bounds its connections and each connection's streams, prepared statements and pending requests. A
connection past the frontend's bound is refused before its Welcome with code 15 (Limit), `detail` the bound (section
11.1); a stream past `max_streams` with code 15 on that stream. A connection's pending requests are its open streams,
so `max_streams` bounds them too, and the Write and Operation frames the frontend holds for a connection (section 11.12,
*Reads*) are bounded by `held_bytes`; one past it is refused with code 15 (Limit), `detail` the bound, on its transaction's
stream, ending the transaction. What a connection has on its way to a role meets the same number and stops that connection's reads,
refusing nothing (section 11.12, *Frames on their way*).

| Limit | Default |
|---|---|
| `max_frame` | 1,048,576 bytes |
| A query text, within one frame | `max_frame` less 512 bytes |
| A query's result columns | 1,024 |
| `max_write_set` | 1,040,384 bytes; at most 261,320 on a drive with power-loss protection |
| `transaction_ms`, a read-write transaction's life | 10,000 ms |
| `max_streams`, and so the pending requests of a connection | 256 |
| `initial_credit` | 4 |
| Connections the frontend holds at once | 4,096 |
| Sessions in the metadata group's registry (`sessions_max`), and so in each partition's table | 4,096 |
| A transaction's deadline, `deadline_ms` 0 | the Welcome's `transaction_ms` |
| `scan_stall_ms`, how long a partition keeps a Scan or a Frontier with no frame able to leave (sections 13.13 and 13.21) | 30,000 ms, and at most that: a client's read timeout, which no Welcome adjusts, leaves it room (section 11.11) |
| `held_bytes`, the Write and Operation frames a connection's reads hold back at the frontend and what a client connection has on its way to a role (`endpoint.held_bytes`, each counted apart), and the frames a connection holds parked at a partition (`storage.held_bytes`, section 11.12) | 4,194,304 bytes |
| `statements_per_object_max`, the statement IDs one (row, neighbour) holds | 16,000 |
| `value_bytes_max`, the longest value a write holds, measured as its sortable encoding in a compiled layout or its term encoding in the generic layout (section 5.2; `../storage/STORAGE.md` sections 5.1 and 5.2; T48) | 16,384 bytes, the store's `tile_bytes_max / 4` (`../storage/STORAGE.md` section 3.3); at most 262,144 |
| What a transaction's read set holds | 1,048,576 bytes |
| A Commit reply's frame | 524,288 bytes, the floor of `max_frame` |
| `catalog_bytes_max`, a catalog version's bytes (section 11.13), carried in as many Catalog frames as they take (section 13.7) | 16,777,216 |
| `max_frame` and `max_streams` of a role endpoint's Welcome (section 11.12) | 1,048,576 bytes and 4,294,967,295 |
| `epoch_ms_min`, the least between two closes | required, 1 to 1,000 ms |
| `epoch_ms_max`, the most between two closes | required, `epoch_ms_min` to 10,000 ms |
| `close_request_ms`, how long a close request goes unanswered before commits are refused (section 11.12) | required, 1 to 60,000 ms, at least twice `epoch_ms_max` |
| `block_size_max`, the largest sequence block a Lease takes | 1,048,576 sequences |
| `catalog_check_ms`, how long the frontend waits for each partition's report of a tightening's check (section 11.13) | 8,000 ms, 1 to 600,000: the client's default timeout, 10 s, leaves it room; a client raising this raises its own timeout too |
| Statements per connection | 256 |
| Idle limit, a connection sending no frame | 600 s |
| Authentication timeout, from Welcome to the final Authenticate | 60 s |
| `auth_failures_before_throttle`, the consecutive failed authentications of one source, an IPv4 address or an IPv6 /64, before each further attempt waits a delay doubling from 100 ms to at most 30 s | 50, at least 1 |
| `auth_throttled_failures_before_ban`, the failed authentications of a source past those before it is banned | 20, at least 1 |
| `auth_ban_ms`, how long a banned source's connections are closed as accepted, before TLS or any frame | 86,400,000 ms (24 hours), at least 1 |

A write that would hold an object under more statement IDs than `statements_per_object_max` is refused with code 6 (TooLarge),
`detail` the bound, so one object record of an Entries row (section 13.14) fits a frame at the floor of `max_frame` in a forward
and a reverse row alike: a floor frame's one section holds 523,000 bytes, and 16,000 IDs take 256,000, which with a forward row's
largest neighbour, a version run's revision of `value_bytes_max` + 8 bytes, at most 262,152 (section 5.2;
`../storage/STORAGE.md` section 3.3), and its inline values, at most 145 bytes (section 13.14), makes a record of at most
518,305, and with a reverse row's neighbour, a system ID, far less, since a reverse
row's neighbour is never a vector (T48, C12).

A partition answers every version at or above its latest logged fold horizon `H`, which its primary stamps on each round
(section 12.1) and every replica and replay folds at (`../storage/STORAGE.md` section 14.2); a read below it is refused with
code 3 (TooOld). The primary keeps `H` at or below the read version of its oldest open read-write transaction, the oldest
version a lane S read holds, the start version of every index build in progress (`../storage/STORAGE.md`
section 14.2; T48), and the version bound of the latest epoch closed `retention_ms` (`../storage/STORAGE.md`
section 3.1, 30,000 ms) before the close preceding the round's epoch, by the sequencer's logged close times, so `H` is a
function of the log (T48, C34). A Scan's or Frontier's version holds `H` on the primary alone: on a follower it holds nothing,
and a request there whose version a logged horizon passes is refused with code 3 for the rest of the request, its frames
already sent no answer. A deadline bounds a read's wait, and a read-only transaction has no other limit on its life. An open
Scan keeps its version for as long as it stays open. Between Scans a version stays only until its epoch has been closed for
`retention_ms` by the sequencer's logged times, so compute keeps a Scan of each partition open across a read that may outlast
that, and a next Scan past it is refused with code 3. Compute opens that Scan at the read's first frame from the partition when
the read's version was chosen before it began, one the client or the transaction named, since such a read may begin late in the
retention; at latest and monotonic-fast it opens it `retention_ms / 4` after the read began, so a short read opens none.

### 11.10 Authorization

Every request is authorized against the connection's principal (`../../openspec/notes/SECURITY.md`). A statement the principal may not read
is absent from a read: a pattern naming a predicate or graph it may not read matches nothing, and one leaving it
unbound returns no row of it. A write, declaration, drop, purge, administration or cluster operation the principal may
not do is refused with code 21 (PermissionDenied), `detail` the action (section 9.4).

**The tenant gate.** A tenant binds per Begin, and a principal may hold grants in several. Every request naming a
tenant, Begin, Prepare, Describe, Install, Drop, DirectWrite, ConditionalWrite, Purge and Migrate, passes one check at
the frontend before it reaches any other role or any state: whether the connection's principal may do the request's
action in that tenant. A refusal is code 21 on the request's stream, `detail` `read` for Begin, Prepare and Describe,
`declare` for an Install and a Migrate, `write` for a one-frame write, `drop` for a Drop and `purge` for a Purge. The
gate fails closed on an action or a scope it does not know. In this stage it allows every authenticated principal, and
stage 7's grants fill it in (`../../openspec/notes/SECURITY.md`, *Where it is built*). A request the gate passes naming a tenant the metadata
group does not hold is then refused with code 7 (Refused), `detail` the tenant, the message naming it, so a principal
holding no grant in a number learns nothing of whether it exists.

### 11.11 Partitions, read levels and session tokens

A storage process hosts partitions, each owning ranges of buckets (section 13.6); an entity's home is the partition
owning its bucket. A read-write transaction stays within the one partition the frontend binds it to (section 11.12):
it reads and writes only what that partition owns, and a request reaching another partition's state aborts it with
code 23 (CrossPartition), writing nothing. Its references to entities other partitions own register at their homes, as
pre-steps outside it that no race among registrants aborts (section 11.12); a home's refusal ends it as section 13.15 states. It makes a reverse or fan-out read, a value-index lookup, a
scan, a pattern with an unbound subject or a bound literal object, only when its partition owns the tenant's whole
run, as it always does with one partition. Code 23 is not retryable: the transaction run again reaches the same
partitions. A read-only transaction reads across partitions at levels 0 and 2.

A read-write transaction reads its partition's latest state whatever its level, at the partition's visibility
watermark when it binds, the Bind's `read_version` (section 13.10): every commit the partition acknowledged before it
bound is visible to it, and none after. A read-only transaction reads at the level its Begin names (section 6.2):

| Level | `level` | A read-only transaction's reads |
|---|---|---|
| Snapshot, the default | 0 | One snapshot for every read, at epoch `e`: the latest epoch the sequencer had closed when the Begin was answered, or the token's epoch when that is higher. Begin's `read_version` is `e`'s version bound. Each read waits until every partition it asks has closed `e`, then reads at that version; while `e` is the sequencer's open epoch, the frontend asks it for an early close |
| Latest | 1 | The latest state of one partition: the transaction binds to the partition of its first read, and each read answers at that partition's latest applied version. A read reaching another partition, or more than one, is refused with code 25 (LevelRefused), which leaves the transaction open |
| Monotonic-fast | 2 | Each read answers at one epoch: the lowest latest-closed epoch among the partitions it reaches, but never below its floor, the larger of the token's epoch and the highest epoch any read of the transaction has answered at when this read is sent; it waits only for a partition that has not closed the floor. A reverse read reads its entity's referrer set at the home's latest closed epoch `e`, waiting until `e` reaches the floor, and has every member partition answer at `e`, each once it has closed `e`. Quad-pattern reads are monotonic, so the transaction's reads never go back and always see its token's commit, though they need not share one snapshot |

A read-only transaction naming a read version (section 6.2) reads every partition at that version, without waiting: a
partition that has closed the version's epoch reads what it applied at or below the version; one holding the version above
its visibility watermark in an epoch it has not closed refuses the read at once with code 27 (AheadOfWatermark), and one whose
latest logged fold horizon is above it with code 3 (TooOld) (section 11.9). A read still waiting for a close at its transaction's deadline is refused with
code 24 (TimedOut), `detail` the partition it waited on, and returns no row, since compute sends no Result until every
partition it asks has closed the read's epoch. A read's silence within its transaction's deadline is that wait, and a hung
compute process answers code 17 by the deadline plus `scan_stall_ms` (section 11.12), so the Rust client's timeout for a read
runs from the later of its last progress and the Begin's deadline, is at least 30 s, the most `scan_stall_ms` holds, and does not
run for a stream whose credit is spent.

A read's answer streams under credit, so it can fail after rows have left. Only a stream ending in `LAST` is an answer: a
read ending in a refusal is no answer, whatever its stream carried before, and a snapshot read is complete or refused, never
partial. A client takes a refusal after rows as the read's failure: the stream has ended, so it sends no Credit or Cancel for
it, and no row it carried counts. The refusal ends the read alone unless section 11.12 ends the transaction with it. The Rust
client returns the refusal after the batches already handed over and resends nothing of a read; running it again is its
caller's choice.

A committed transaction's Commit reply, and a Drop's reply, through a session carries its session token (section 4.8): its
partition, its round's position in that partition's log and the epoch of its commit version. A reply through no session holds
the zero token, a declaration's and a Drop's among them. A later transaction carrying the token
sees that commit and every commit before it in that partition, on any connection. A read-write transaction carrying a
token of epoch `t` commits at epoch `t` or later: its partition holds the commit until the close of `t − 1` has been
delivered to it, so a causally later write never falls in an earlier snapshot. A read-only transaction carrying a token
reads at or above its epoch, waiting within its deadline: at level 0 its snapshot is at least the token's epoch, at
level 1 each read waits until its partition has closed the token's epoch, and at level 2 the token's epoch is part of
every read's floor. A transaction naming a read version is the exception: it reads at that version, whatever token it
carries, and the token gives it nothing. The Rust client keeps, per session, the token of the highest epoch among the
session's commits, one-frame writes and resolved resends included, and sends it with every Begin it makes for that
session and in every DirectWrite and ConditionalWrite through it; a zero token replaces nothing.

### 11.12 Role connections

The roles, frontend, compute and storage, exchange this protocol's frames over the transport at their role addresses:
`tcp://host:port` across hosts, `unix:///path` within a container, where each role's socket belongs to user 65532 with
mode 0600. The frames are the same on both carriers. Each role listens for role connections at its role address, apart
from the frontend's client endpoint: a connection's kind is the endpoint it arrives at, and the client endpoint refuses
every role command (section 3). On every connection, requests travel from the side that dialled to the side that
accepted, which answers on the request's stream (section 2.5, step 3). Role connections speak clear text in this
stage; TLS between roles arrives with stage 8 (`../../openspec/notes/SECURITY.md`). Partitions of one storage process exchange what a role
command carries as a message within the process, with the same contents.

**Handshake and the role secret.** A role connection opens as a client's does (section 11.1): Hello, then Welcome with
`auth` 1 and the one mechanism `SCRAM-SHA-256`, then an Authenticate exchange (section 5.3) before the first role
command. The exchange authenticates against the cluster's role secret, which every role reads at start from the file its
`[cluster] role_secret_file` setting names: the SCRAM username is the connecting role's name, `frontend`, `compute` or
`storage`, and the password is the secret, the file's contents without a final line break. The accepting role verifies
the peer locally, against a verifier it derives from the secret at start with a salt of 16 random bytes and 4,096
iterations, since the principals live in the frontend, which is itself a peer. Its final Authenticate names
`principal` 0 and `expires` 0: a role connection acts for no principal, and each role command acting for a client
names the client's principal in its own fields. A refusal is code 19, as on the client endpoint, and the refused role
retries until its deadline. The client endpoint authenticates against the metadata group's principals alone, so the
role secret opens no client session: it is refused there as a wrong password is. The frontend's dial of a role, its connect,
Hello and exchange together, takes at most `transaction_ms`; past that it fails as a closed connection does.

A role endpoint's Welcome announces `max_frame` 1,048,576 and `max_streams` 4,294,967,295, and a role's Hello `max_frame`
1,048,576: what a role connection carries is already bounded by the frontend's bounds on its clients' connections, streams and
frames, a relayed frame takes any size a client's may, and a role's stream refusal would reach a client as a limit it did not
pass. A partition's Welcome announces `initial_credit` 1: a reader asks for each Scan frame as it wants it, so an open Scan holds one
frame at the reader.

A role connection has no idle limit (section 11.9): the side that dialled sends nothing between requests while it reads a
Subscribe's closes.

**What each endpoint serves.** Besides Hello, Authenticate, Credit and Cancel:

| Endpoint | Serves | Requested by |
|---|---|---|
| The frontend's role address | Join, Members, Placement, Catalog | storage and compute |
| | Subscribe, Lease, CloseRequest, DefaultEntry, Report | storage |
| A partition's role address | DirectWrite, ConditionalWrite, Ingest | the frontend |
| | Bind, then Write, Operation, Commit and Abort, or Drop, on the Bind's stream; Recall | the frontend |
| | Reference | storage and the frontend |
| | Scan, Frontier, Closed | compute |
| A compute process's role address | ComputeRead | the frontend |

An endpoint refuses every other command as `CommandNotServed` (section 2.5). Challenge and Verify (sections 13.8,
13.9) are served by no endpoint in this stage: the frontend authenticates every client and hosts the metadata group, so
it verifies a client's proof in its own process; stage 8's frontends send them to the metadata group's host.

**Joining.** Storage and compute are configured with the frontend's role address alone. Each connects to it and sends
Join, naming its own role address and, for storage, the partitions it hosts; the frontend connects back to that
address for the requests it sends the role. Storage and compute learn where each partition and each compute process
listen from Members, and which partition owns a bucket at an epoch from Placement. Storage and compute retry Join until
their deadline, so the frontend's recovery need not have ended when they start.

The frontend forms the placement when its metadata group holds none: an even map over its configured partition count. A
Join claims the partitions it names. One claiming a partition outside the placement, or one another role address holds, is
refused with Error 1 (Protocol), the message naming the partition and its holder; a holder's own address, restarting,
claims it again. So any number of storage processes host the partitions between them. In this stage the placement is fixed
once formed, and a process holds its map for its lifetime; moving bucket ranges arrives with stage 8.

Each partition's core listens at its own role address and joins for itself, so a connection to a partition lands on the core
that owns it and no frame crosses cores on arrival. Peers read those addresses from Members and batch per address.

**Epochs.** Storage sends Subscribe on its connection to the frontend, naming the last close it holds, and the frontend
answers on that stream with an EpochClose per close, in order: first each close the sequencer recorded after the one
named, then each new close once the sequencer's log holds it durably. A storage process that reconnects subscribes
again from its last close, and so receives the closes it missed before any later one. Each close carries the catalog
versions the sequencer assigned at it, each taking effect at the epoch one above it, and the session evictions the
registry made since the previous close. A close is delivered to a partition once its storage process holds every frame
of the close and every catalog version the close announces, which it reads with Catalog (section 13.7) first; the
first round the partition then logs names the evicted sessions, and applying it deletes their entries from its table
(sections 11.5 and 12.1). A partition stamps every commit with its open
epoch, exactly one above the latest close delivered to it, and never advances it on its own, so it holds the catalog
versions in effect at an epoch before it stamps that epoch. It has closed an epoch once that close has been delivered
to it and it has applied every round stamped at or below it.

When the sequence of its open epoch would run out, a partition sends CloseRequest naming that epoch and holds its
commits until the close arrives; the sequencer closes the epoch once `epoch_ms_min` has passed since the previous close.
Once the request has gone unanswered for `close_request_ms`, the sequencer counts as unreachable, and the partition
refuses each held and new commit with code 5 (Busy), retryable, writing nothing, until the close arrives. A snapshot
read waiting on the open epoch makes the frontend ask its sequencer for an early close in its own process.

**Sequence blocks.** A partition leases blocks of sequences per type with Lease (section 13.5), its next block before
its current one runs out, and allocates no sequence outside a block it holds.

**Registration.** Before a partition first commits a reference from a subject of bucket group `g` to an entity `o`
another partition owns, it registers at `o`'s home with Reference (section 13.15): `o` by its IRI's text when it holds
no ID for `o`, else by its system ID, with `g` and the generation it last saw for `(o, g)`, 0 for none. The home gets or
creates `o`'s ID, adds or renews `g` in `o`'s referrer set, makes both durable in a round of its log, and only then
replies with the ID, the member's generation and the latest close delivered to it, `c`. The registering partition
commits the referencing transaction only once close `c` has been delivered to it, so the write is stamped at `c + 1` or
later and never below the member's epoch. A predicate or graph IRI another partition owns is got or created at its home
the same way before its first use, naming no bucket group and joining no referrer set. Registrations are issued while
the transaction executes, batched per home's role address within each write request, and idempotent: a repeat returns
the same ID and changes nothing, and concurrent registrants receive one ID, none aborting for the race. A partition
holding its mark of `(o, g)` sends none. The frontend registers a unique predicate's IRI the same way, naming no group,
to learn the system ID a claimed value's bucket is computed from. A participant another partition owns, which a relation
instance names (`../schema/SCHEMA.md` section 12.3), registers in get-only mode (section 13.15): its home creates no ID and
answers whether the participant is absent, live, retracted or deleted, and its type prefix.

**Parked requests.** A role connection keeps moving while a request on it waits. A partition copies every request out of the
connection's receive buffer as it arrives, hands the copy to its core and reads the connection's next frame, without waiting for
the core to take the request: the core takes its requests between turns, and a turn issues its log's sync while the sync's completion is a later
input (`../storage/STORAGE.md` section 2; `../../openspec/notes/UltraGraphDB Scalable Tile Store.md`, section 4.9;
T48), so the requests a connection carries while a sync is outstanding reach the core together and its commits share
the next round. A partition parks a Write, Operation,
DirectWrite, ConditionalWrite or Ingest share waiting on its registrations' answers, and a Write, Operation or Commit behind a read of its
transaction waiting on a home (section 13.13): it copies the frame out of the connection's receive buffer, reads the connection's
next frame, and sends the request's answer on its stream once it has one. A frame arriving on a stream with a request parked is
parked behind it and taken in its order, so a stream's requests take effect and are answered in the order they were sent, a Commit
behind the writes before it. A Cancel or an Abort on the stream ends the transaction at once, with what it parked. An Error on the
stream drops what is parked behind it, and a frame that crosses that Error on the wire is late. A connection that closes abandons
what it parked, its transactions aborting (section 11.4), and a partition that stops answers each parked request with code 17 or
closes the connection, which a frontend answers as code 17; nothing parked has reached the commit step.

The bytes a connection holds, its requests copied and not yet taken by its core together with those parked, are bounded by
`storage.held_bytes` (section 11.9). Once they reach it, the partition stops reading that connection until they fall below it, and
goes on sending what it answers. The bound refuses nothing: a frame read is parked whole, so a connection holds at most `held_bytes`
and one frame, and a refusal would end a client's transaction with a limit the client did not pass. Stopping cannot deadlock,
because nothing that releases a parked frame travels on a connection that can stop. A request waiting for its core is released by
the core's own turn, which waits on no connection; a parked frame is released by a home's reply or by its transaction's limit,
`transaction_ms`. A home answers a Reference from its own log, asking no other partition (section 13.15), and a partition sends its
registrations on connections of their own, which carry Reference alone and so park nothing and never stop. So when partition A's
write waits on B's Reference and B's write waits on A's, each home answers whatever either partition holds parked, with both
frontends' connections stopped. A Scan and its Credit travel on compute's connection, which parks nothing either. What waits on a
stopped connection, a frontend's own Reference, a Cancel or a Commit, waits at most for the limit of what is parked ahead of it.

**Frames on their way.** A frontend's role connection is one queue its client connections share, first in, first out, and a frame
leaves it as the role reads. What a client connection has on its way to a role is counted per client connection, against
`endpoint.held_bytes` (section 11.9): a Write or Operation no read holds back, from its arrival until it leaves the queue, and a
transaction's first until it is answered, since a refusal resends it; a DirectWrite, ConditionalWrite or Ingest until it is answered or passes
`transaction_ms`; a Bind, ComputeRead, Reference, Recall or Credit until it leaves the queue. A Commit, a Drop and a Cancel are not
counted: each stream sends one, and `max_streams` bounds them. Once the count reaches the bound, the frontend stops reading that
client connection until it falls below it, and goes on sending what it answers, so a partition that stops reading reaches the client
whose writes it holds. The bound refuses nothing: a frame read is counted whole, so a connection counts at most `held_bytes` and one
frame, and a refusal would end a transaction with a limit its client did not pass.

Stopping cannot deadlock, because nothing that lowers the count travels on the connection that stopped. A partition's answer sends
the next write, and a transaction's limit, whose timer runs while the connection is stopped, ends it and takes back what it queued. A
stream whose opening frame has not left is taken back whole and sends no Cancel; the partition sees a gap in the stream numbers, which
it takes. A Write behind a read is not counted, since the Credit draining that read arrives on this socket; the same number refuses it
instead (section 11.9). A Credit, Cancel, Abort or Commit a client sent behind what stopped its connection waits at most for
the limit of the transactions counted ahead of it. A one-frame write still in the queue at `transaction_ms` from its arrival is taken
back and answered code 4, having reached no partition. One that has left stays with its partition, which answers it, and is no longer
counted.

A client's count is its own. A client at its bound holds up no other client's reads, and the frames of others on a queue it shares
wait behind at most its bound. A stalled partition stops only the clients whose counts its frames fill, and each of those, having one
socket, stops whatever else it asks until its count falls. A client slow to read its answers holds up no queue, since a role connection
hands each answer to its session by message. So the frontend holds at most its connections times `held_bytes` of these frames.

**Writes.** The frontend binds a read-write transaction before its first request that reaches a partition's state, by the first
row that fits:

| The transaction's first request | The partition it binds to |
|---|---|
| reads or writes a subject it names by an IRI or a skolem IRI | the partition owning the subject's bucket: an IRI's computed from its text and the tenant, or, for an IRI a keyed type of the tenant's catalog mints, from the key space and the key it states (`../schema/SCHEMA.md` sections 7.2 and 7.3), a blank node's or a reifier's the bucket its skolem IRI states, a keyless instance's the bucket its handle IRI states (section 4.4) |
| claims a value by an insert-if-absent or upsert-and-link, and names no subject but new blank nodes | the partition owning the claimed value's bucket |
| creates, under a new blank node, an instance of an unkeyed, node-encoded relation of the tenant's catalog (`../schema/SCHEMA.md` sections 9.1 and 11.1) | the partition owning the bucket of the instance's anchor participant, the object of its anchor role's triple, so the instance lives where its anchor does |
| creates, under a new blank node, an instance of a keyless entity type holding an `@idempotency` value (`../schema/SCHEMA.md` section 7.1) | the partition owning the bucket the value derives, so a retry from any session reaches the claim |
| writes only new blank nodes as subjects | a partition the frontend chooses in turn, round-robin over the partitions holding the tenant's run, which draws the nodes' buckets from its own |
| names no subject otherwise | the partition owning the tenant's whole run, when one does; when none does, the request aborts the transaction with code 23 |

A read binding an IRI that section 4.4 makes one nobody holds, under a skolem base, binds nothing and reaches no other role: the
frontend answers it itself, at any level, with one batch flagged `LAST`, `rows` 0, `columns` the unbound pattern slots and one more, and
`epoch` and `partition` 0, and the transaction's next request or read is its first.

A Commit of a read-write transaction no request has bound and holding no declaration reaches no partition: the frontend
answers it itself, `commit_version` 0, the token zero and no results, and stores nothing. The session's counter moves as after
any Commit reply. A resend of it is answered code 18, and running the transaction again commits nothing as well.

It sends Bind (section 13.10) on a new stream to the storage process hosting the partition, and the stream becomes the
transaction's there. The frontend relays the client's Write, Operation, Commit and Abort frames on it, command headers
and bodies unchanged; storage answers as a server answers a client, filling, through a session, the commit's session token in
its Commit reply, and the frontend relays each reply on the client's stream. Only a statement's subject decides where it goes: an
object another partition owns is registered at its home, where only section 13.15's outcomes refuse the write. Before it binds, the
frontend checks every subject the first request names by an IRI or a skolem IRI against its map: a request whose subjects spread
over partitions is refused with code 23 on the transaction's stream, `detail` as for an operation bound to none, binding nothing,
and the transaction ends. A partition refuses a first request whose first subject lies outside its bucket ranges with code 26
(WrongPartition), naming the bucket and its owner; the frontend refreshes its placement map, then binds to the owner and resends
once, and when the owner refuses with code 26 too, the frontend answers the client code 26 on the transaction's stream. Code 26 of
`detail` 0 names no owner and is a home's refusal of a registration (section 13.15), so the frontend answers it to the client at once. A partition
refuses one whose first subject lies inside its ranges and a later one outside with code 23. Until the transaction's first request is
answered, the frontend sends none of its later requests to a partition or to compute, keeping them, and on code 26 sends them
after the resend, in order. A first read counts as answered, for this rule alone, at its first frame or its Error, since its
refusals come before any row. A later request reaching another
partition aborts the transaction with code 23, at the frontend when its map shows it and at the partition otherwise; so
does a unique value claimed in another partition, a delete by pattern reaching another partition's state, and a
reverse or fan-out read over a tenant spread over several partitions. A write naming a statement by its reifier (a
delete of the statement, or a property set or removed through it) is checked at the partition against the subject the
reifier resolves to, once the write executes: when another partition owns that subject's bucket, code 23 aborts the
transaction, `detail` the transaction's partition and the subject's owner, and nothing it wrote is held. When compute answers a read of a read-write
transaction with code 23, the frontend cancels the transaction's Bind stream, which aborts it at the partition. When that read
is the transaction's first request, the frontend refreshes its map, and if the refreshed map moves the read's subject, it binds
to the owner and resends the read once. Otherwise it relays code 23 on the read's stream, drops the writes it holds, cancels the
transaction's other reads still open, and answers with code 23, the first's `detail`, each of those reads, each read it holds,
every later Read, Execute or Query naming the transaction, which stays open for section 7.1 until then, and the next request on
the transaction's stream, which ends it.

**Drops.** The frontend sends a Drop to the one partition holding every bucket it reaches (section 6.5): it binds a
stream with Bind of kind 2, relays the Drop on it, and relays the partition's Drop reply, which ends the stream. A resend
goes the same way, and the partition's session table decides it (section 11.5). A Drop reaching several partitions is refused
with code 23 before any partition sees it.

**Reads.** The frontend sends a client's Read to compute as ComputeRead (section 13.12), an Execute with its
statement's parameters bound into the pattern, carrying what compute needs of the transaction: its mode and level, its
partition and identity once bound, the read version or epoch it reads at, its floor, its principal and the time left
until its deadline. A read-write transaction binds before its first read reaches compute. The frontend relays
compute's Result frames on the client's stream, and takes from them the highest epoch a monotonic-fast read has answered
at, the floor of the transaction's next read, and the partition a level-1 transaction's first read bound it to. It holds a
level-1 transaction's later read until its first read's first Result names the partition, or that read ends; a first read
refused leaves the next read first. It relays each Credit and Cancel a client sends on a read's stream to that read's
ComputeRead stream, which starts with the ComputeRead's `credit` (section 13.12).

The frontend sends each ComputeRead to the next compute process of Members, round-robin per endpoint core. It moves to the
next only when the connection cannot be opened, or closes before any frame of the read reached the client, and with none
left the read is refused with code 17 (Unavailable). Once a frame has reached the client, the read is never resent, and a
closed connection answers it with code 17. A compute process that sends no frame of a read for `scan_stall_ms` past the
later of the read's deadline and its last frame, while the read's stream holds credit, is hung: the frontend cancels the
ComputeRead and answers the read with code 17.

The frontend sends a read of a read-write transaction only once every write request the client sent on the transaction's stream before it has been
answered, holding the read until then, and sends such a write only once every read of the transaction already sent has
been answered, so writes and reads keep the order the client sent them: a read sees the writes before it, whichever
connection carries it, none after it appears in part of its answer, and reads with no write between them run beside one
another (section 11.2). A write is answered by Held or an Error (section 13.20). A read is answered by its `LAST` or an Error,
or once the frontend has sent its Cancel, whether the client's or a Commit's; one the client cancels while it is held is dropped unsent. Compute reaches the partition on a connection of its own, so this order is the frontend's to keep.

A Commit and an Abort wait for no read. Taking a Commit, the frontend cancels the transaction's reads still open, drops unsent
those it still holds, sends the writes held behind them in order on the Bind stream, and then sends the Commit, so every write
the client sent before its Commit is in what commits. Taking an Abort, it cancels the reads and drops the held writes. No frame
of a cancelled read reaches the client, and a span its Scan records after the Cancel only widens what the commit validates.

A read under way when its transaction ended is refused at the partition, and compute's Error 1 of `detail` 1 (section 13.13) may
arrive first: the frontend holds it until the Bind stream ends, then answers the read with the Error the stream gave, such as
code 4 (PastLimit), or the code 23 a read's refusal cancelled the stream with, and sends nothing further when it has already
answered the read.

Compute reads through storage's read kernels: Scan (section 13.13), records of the partition at a role address by
their keys and families read whole, at a version, each family bounded by a row limit, batched per address; and Frontier
(section 13.21), many rows of one relation in one direction, or of one attribute, with a type range, inline filters and
a top-k. A relation whose liveness derives from its participants' (`../storage/STORAGE.md` section 18.4) is filtered
by compute: it keeps a relation's row only while each participant's `asserted_end` is absent, read by a Frontier of kind
6 at the participant's home, and in lane X that Frontier records the participant's forward row, so a retraction of a
participant committed meanwhile conflicts (T48, C44). It resolves a bound term in the partition whose
rows it reads; an absent entry there means that partition holds no statement of the term and answers no row. Only where one
ID must key several partitions, a reverse read's object and a resume key's terms, does it read the term's home. It names a
row's system IDs by the dictionary of the partition that answered the row. A term and the rows keyed by its ID take two reads,
a Scan of the term's dictionary record first. It chooses the partitions a read asks by its bound positions:

| Pattern | Partitions asked |
|---|---|
| A bound subject that is a reifier's skolem IRI, or the handle IRI of an edge-encoded instance, which the catalog's Type entry tells from a node-encoded one | the partition its bucket names, for the statement's record (`../storage/STORAGE.md` section 9.3, kind 18), one row per property matching the pattern, with `ABSENT` in the reifier column |
| A bound subject | the subject's partition alone |
| An unbound subject and a bound IRI or blank-node object, a reverse read | first the object's home, for its referrer set at the read's epoch `e`, once the home has closed `e`, or, at a named version of epoch `e`, its set at the home's latest applied version (`at` 2, waiting for nothing), since a set only grows and an edge at that version may have its member above it in the home's own order; then the partitions the set's bucket groups resolve to through the placement map in effect at `e`, for their reverse rows of the object at `e`, batched per address, each once it has closed `e`. A partition the set does not name is not asked. A reverse row's object records hold their statement IDs as a forward row's do, so this read takes no further Scan for them |
| An unbound subject and a bound vector object | every partition holding a bucket of the tenant's run, for the predicate's forward families, or every predicate's when it is unbound, compute keeping the objects equal to the vector's encoding, since no reverse key names a vector |
| Any other, a bound literal object among them | every partition holding a bucket of the tenant's run, batched per address |

A value-index lookup and a lookup by reverse count are compute's own reads for the query core, called inside compute and
reaching the same partitions, a reverse count being the sum of the partitions' counts; no client request names them in this
stage.

It answers the partitions one after another in section 9.1's order, into Result batches that name every term by its
text, with resume keys of its own (section 11.8). It grants a partition's read credit as it drains that partition's
answer, so a read of rows that gave rows and then stalled (section 13.13) waited its turn: compute goes on with a new read at the
same version from the resume key its last frame gave, and only one that stalled before giving a row refuses the read with code 24. Compute sends a read's reads of rows only once each
partition it asks has closed the read's epoch, which a Scan of dictionary records at that epoch shows; a read with no term to resolve
there asks a range of space 1 of the dictionary for it. So a read of rows never parks on another partition's close. Before a monotonic-fast read reaching
several partitions, it asks them their latest closed epochs with Closed (section 13.17). A read-write transaction's
reads ask its partition alone, at the Bind's version, and every Scan of them names the transaction, so the partition
records what they read in the transaction's reads, which its commit validates (section 13.13).

**Default entries, checks and builds.** A partition whose commit first writes a predicate the catalog version in effect
does not declare asks the metadata group to record the predicate's default entry with DefaultEntry (section 13.18),
after the commit and holding no write for it; on recovery it asks again for every undeclared predicate its replayed
rounds wrote. At a catalog version's effective epoch, each partition holding the tenant checks a type change the
version makes against its data, holding its commits of that epoch until the check completes, and starts the index
builds the version declares; a version arriving while a build runs waits for the build's end, and the versions after it
behind it. Once the round taking a version effect is durable the partition reports the outcome of every version it took
effect, tightening or not, and each build that fails, to the metadata group with Report (section 13.19), which the
frontend answers once the group holds the report; a partition sends again a report left unanswered when its connection
closed. A partition starting over a log past a version's epoch loads the version in effect at its applied epoch before it
replays, and a close delivered again takes its versions effect again, checked again.

**Errors.** A role answers a refusal with Error, its codes those of section 9.4; code 26 passes between roles, and
reaches a client only as *Writes* states. A role or partition stopped on an invariant violation answers code 17
(Unavailable) to every request it affects, and the frontend answers code 17 to each client request it routed there.
The frontend passes every other code a role answers to the client unchanged, on the client's stream. When a role connection
closes under a request, whether the peer went or a frame was refused, the side that dialled answers each read and request it
carried with code 17, except a read none of whose frames reached the client, which the frontend sends to the next compute
process (*Reads*), and compute sends no Scan again for such a read: the read ended in a refusal, and what it sent before is
no answer (section 11.11). A partition that leaves a request of compute's unanswered past the time the request waits on an epoch
(`deadline_ms`) and twice `scan_stall_ms` beyond is hung, since it refuses any Scan or Frontier it holds that long: compute closes the
connection, which ends what it carried so, and dials again for the next read.

### 11.12a The system group and compute, from stages 5.8 and 5.9

**The system group (stage 5.8).** The metadata group's state machine and the sequencer are the state machines of one
replica group, the system group, which a storage process hosts beside its partitions
(`../../openspec/designs/UltraGraphDB Distribution Architecture.md`). It has one replica until stage 8. Its host listens
for role connections at the system group's role address, an address of its own beside its partitions', and every other
process is configured with that address, where storage and compute were configured with the frontend's. A role
connection to it opens as section 11.12 states. A partition its host also hosts dials that address as a partition of any
other process does: no partition asks the system group anything per commit, so one path serves both and is tested once.
A role connection's role is its SCRAM username (section 11.12), by which the system group tells a frontend's connection
from a partition's. In this section "the system group" is the group and "its host" the storage process running it. The
system group forms the placement when it holds none, an even map over its configured partition count, as section 11.12
has the frontend do.

From stage 5.8 each endpoint serves, besides Hello, Authenticate, Credit and Cancel:

| Endpoint | Serves | Requested by |
|---|---|---|
| The system group's role address | Join | compute and storage |
| | Members, Placement, Catalog | the frontend, compute and storage |
| | Subscribe, CloseRequest | the frontend and storage |
| | Lease, DefaultEntry, Report | storage |
| | Challenge, Verify, Session, Check, Ended, Declare, Defaults | the frontend |
| A partition's role address | as section 11.12 | as section 11.12 |
| A compute process's role address | ComputeRead | the frontend |

The frontend accepts no role connection from this stage and sends no Join. Each of its endpoint cores dials the system
group, each partition and each compute process on connections of its own. The frontend reads the addresses from Members:
at its start, until Members lists every partition of the placement; again when one of its role connections closes; and
again before it refuses a read for want of a compute process.

**Epochs.** Each endpoint core subscribes to the sequencer's closes (section 13.3). It holds no close of its own, so its
Subscribe sets `latest`: the stream begins with the latest durable close, and a subscription the sequencer ends as no
longer held is opened again the same way. From the closes the core takes the latest closed epoch that a Begin and a
Describe answer with, each catalog version's effective epoch, and the placement version in effect, reading the map again
with Placement when a close assigns one. The latest closed epoch is therefore each core's own: one connection's
snapshots never go back, and two connections may answer at an epoch and then at the one before, where only a token
carries the floor from one to the other (section 11.11). A partition may have received a close the core has not, so a
token whose epoch is above the core's open epoch is not refused as section 4.8 states: a Begin naming it is held until
the core's stream delivers the close before the token's epoch, and refused with code 24 (TimedOut) at its deadline; a
DirectWrite, a ConditionalWrite and an Ingest naming it go to their partition, which holds the commit until that close
(section 11.11). A snapshot read waiting on the open epoch makes it send CloseRequest naming that epoch, with
`partition` 0 (section 13.16).

**Authentication.** The system group's endpoint serves Challenge and Verify (sections 13.8 and 13.9), and the frontend
sends them for each client's exchange, so a principal's stored key and server key never leave the system group. The
final Authenticate's `expires` is 0 for SCRAM. The counts of failed authentications, the throttle and the ban of section
11.9 stay with the process that accepted the connection: each process holding a client endpoint counts for itself, in
one of its cores for all of them. A principal's change reaches no endpoint before stage 7, whose administration commands
carry a revocation with the closes, as evictions travel.

**Sessions.** The registry, the owner check and the mark of a commit in flight stay together in the metadata state
machine's host, as section 11.5 has them, because the registry evicts only a session with no commit in flight. The
frontend asks for each by a role command:

- **Register.** It draws the session's identity and sends Session (section 13.23) with the connection's principal. It
  answers the client's Register once Session is answered, which is once the registration is durable. A full registry
  holding no session it may evict answers code 14, which the frontend passes to the client. A Session whose answer is
  lost with its connection answers the client's Register with code 17: the client registers again and the frontend draws
  a new identity, and the session left behind ages out (section 11.5). `sessions_max` bounds the cluster's sessions, and
  is sized to the connections of every process holding a client endpoint.
- **The owner check.** Before it routes a Commit, a Drop, a DirectWrite, a ConditionalWrite or an Ingest through a
  session, and before it asks a partition a Recall, it sends Check (section 13.24): kind 2 for a request that may record
  the session in a partition's table, kind 1 for a Recall. It routes the request only once its Check passed, and answers
  the client with the code a failed Check gives. A check naming a session that has a request in flight answers 14, of
  kind 1 too: the mark is the one place that knows a resend's original is still on its way, whichever core or node the
  resend arrives at. When every partition has answered a Recall with 18, the frontend sends a second Check of kind 1 and
  answers the client 12 on outcome 12, else 18 (section 11.5).
- **In flight.** A Check of kind 2 that passes marks the session in flight. The frontend sends Ended (section 13.25)
  when the request ended before reaching a partition, when its partition answered it, for an Ingest its last share's, or
  when the role connection carrying it closed. A mark ends by an Ended on the connection that began it. When that
  connection closes, the system group keeps its marks for `transaction_ms` (section 11.9), the longest a request no
  endpoint speaks for can still reach a commit step, and ends them then; an Ended on any connection ends a mark whose
  connection has closed. For `transaction_ms` after the system group begins to listen, when it holds no mark of the
  requests then on their way, it evicts no session, refusing a Register that would need an eviction with code 14, and
  answers every check of kind 1 with 14.

An endpoint core sends the Endeds of one turn in one frame and then the Checks of that turn in one frame, one check per
session: a further request through a session waits for the next frame, so the system group decides every outcome. A
Check reads the registry as every registration decided before it left it, durable or not, and a mark exempts its session
from an eviction decided after it. The round trip a
Check adds to a commit is the price of one authority over sessions; whether a check can be held without asking is item 13 of `../../openspec/notes/backlog.md`.

**While the system group cannot be reached.** A core whose connection to the system group closed dials again. Until it
is answered, it answers an Authenticate with Error 17 (Unavailable) on stream 0 and closes the connection, counting no
failure, so an outage bans no source; a Register with code 17; and a Commit, a Drop or a one-frame write through a
session with code 17, cancelling the Bind stream of a transaction whose Check went unanswered, so that nothing commits
later and the client's resend finds it not committed. A Begin, a read and a commit through no session go on from what
the core holds. The process exits with code 69 once the system group has been gone for `[cluster] join_deadline_ms`.

**The catalog.** The frontend reads catalog versions with Catalog (section 13.7), and takes a tenant's existence from
Placement's runs and a version's effective epoch from the closes. A tenant's current version, which section 11.13 has a
declaring transaction build on and section 11.7 a schema id follow, is the highest the process has learned: from one
Catalog read when a tenant is first used, where code 7 with `detail` 0 says the tenant holds none, from each close's
announcements, and from each Declare's reply. It is never asked per request, and nothing is decided from it: the system
group checks the version a Declare is built on. Which version a Describe reports is the system group's to say, since
only it knows a version being checked or withdrawn: the frontend asks with a Catalog request of `version` 0 naming the
latest closed epoch (section 13.7). What section 11.13 has the frontend do in its own metadata group, the metadata state
machine's host does: committing a version built from a transaction's declarations, the version a Drop calls for, the
wait of a tightening's check and its restoring version, and the default-entry records a Describe names. The frontend
asks for them by Declare (section 13.27) and reads the records with Defaults (section 13.28); every rule of section
11.13 stands. A Declare is one frame: a Write that takes a transaction's declarations past what one Declare frame holds
within the role connection's `max_frame` is refused with code 6 (TooLarge), `detail` the bound, ending the transaction,
and a larger schema comes by Install. A Declare of kind 1 is never sent again: when its connection closes under it, the
frontend answers the client's Commit with code 17, in doubt, and Describe shows what took effect, which from this stage
is a second way a tightening's outcome is left in doubt. A Declare of kind 2 may be sent again. The metadata state
machine and its log treat a version's bytes as opaque. The system group's host, before it appends, links the catalog's
model and no parser of UGS or query text: the decoder, the encoder, the comparison of two versions, and the application
of declaration records and drops with their refusals.

Stage 6.6 adds two things here. A compiled version, which the role holding the client endpoint compiles from an
Install's sources (`../schema/SCHEMA.md` section 10.1), is offered to the system group by the command kept at 540. Every
version holds its allocation counters, a Drop's and a write-path version's included, and a tenant holding none starts
from the initial values, so compiling is a function of the sources and the base version. The system group commits an
offered version only while the version it was built on is still the tenant's. It first decodes the offered bytes and
encodes them back, and compares the version with its base: the tenant, `version` one above the base's, each counter at
or above the base's, the numbers of retained entries unchanged and new ones taken from the base's counters in entry
order. It refuses a version failing any of these, since every partition stops on a version it cannot decode and a number
issued twice cannot be taken back.

The fence of a first schema (section 8.4) is a record of the system group. The role holding the client endpoint asks for
it by a role command naming the tenant and the base version, refused when the base is no longer the tenant's or a fence
is held. The fence is durable before it is answered, named in the sequencer's next close `k`, as evictions are, and
logged by each partition in its first round after `k`. The Install then reads each partition at the closed epoch `k +
1`, which holds every commit stamped before the fenced round. The fence is lifted at the version's effective epoch, or
by a close naming its release: when the Install is refused, or when the connection that asked closes with no offer
committed. A partition refuses a fenced tenant's Commit, one-frame write, Ingest share and Drop at its commit step with
code 31, so the fence holds whichever compute node a write arrives at. Stage 6.6's change specifies both commands'
bodies and the close's fields.

**Compute holds the client endpoint (stage 5.9).** The frontend and compute are one role, compute. Role kind 1 is
retired, the SCRAM usernames of role connections are `compute` and `storage`, and compute does everything sections 1 to
11.12 and this section have the frontend do, and joins as section 13.1 states. A partition refuses a Drop of kind 3 or 4
whose `catalog` is not the version in effect at its round's epoch with code 2, kind 5, as it refuses an Ingest share
(section 6.8): the check section 6.5 has the client endpoint make is then an early refusal, since a node one close
behind would pass a Drop aimed at an earlier schema. From stage 5.9:

- **Reads.** Compute answers a client's Read, Execute and Query itself, on the core its connection arrived on, by the
  kernels, levels, lanes and partitions sections 11.12 and 13.12 state for compute. ComputeRead is retired, and with it
  the rules of section 11.12, *Reads* and *Errors*, about choosing a compute process, a hung compute process, a read
  moved to the next compute process and the relay of a read's Credit and Cancel.
- **A transaction's order.** The core that holds a read-write transaction's Bind stream sends its Scans and Frontiers
  too, and keeps the order section 11.12, *Reads*, states between a transaction's writes and reads.
- **Connections.** An endpoint core holds one role connection to each partition, for writes and reads alike, and one to
  the system group. Nothing dials compute, so it has no role address.

| Endpoint, from stage 5.9 | Serves | Requested by |
|---|---|---|
| The system group's role address | Join, Members, Placement, Catalog, Subscribe, CloseRequest | compute and storage |
| | Lease, DefaultEntry, Report | storage |
| | Challenge, Verify, Session, Check, Ended, Roster, Declare, Defaults | compute |
| A partition's role address | DirectWrite, ConditionalWrite, Ingest; Bind, then Write, Operation, Commit and Abort, or Drop, on the Bind's stream; Recall; Scan, Frontier, Closed | compute |
| | Reference | storage and compute |

### 11.13 Declarations, drops and the catalog

A tenant is created explicitly and recorded in the metadata group, whose tenants Placement's runs list (section 13.6): the
cluster's first start creates tenant 0, and a further tenant is created, or a tenant removed, by the cluster operation of
`../../openspec/notes/SECURITY.md`'s `operate` action, whose command arrives with stage 7's administration commands. A cluster wipe removes each
tenant. A tenant's catalog lives in the metadata group as numbered versions, which the protocol carries as opaque bytes
(section 13.7); no partition holds catalog keys. A partition validates and applies each round against the version in
effect at the round's epoch. A version is immutable once committed, and the metadata group keeps every version a
partition's log names above that partition's last checkpoint, so replay finds the version each round was applied under
(T48, C5).

- **Declarations.** Write records of kinds 8 to 10 (section 4.6) are declarations. A transaction whose Writes hold
  declarations holds nothing else: a Write mixing them with other change records, an Operation or a read in such a
  transaction, or a declaration in a transaction that has changed or read data, is refused with code 7 (Refused),
  `detail` the record's index, 0 for an Operation or a read, ending the transaction. The frontend executes the
  transaction itself and binds it to no partition. Its Commit names no session, `session` all zero and `request` 0,
  refused with code 7 otherwise.
  In a tenant that holds a schema a declaration is refused with code 7, `detail` the record's index, the message
  naming Install: such a tenant's catalog is the compilation of its sources alone (`../schema/SCHEMA.md` section 6.1).
- **Schemas.** UGS sources arrive by Install (section 8.4): the frontend compiles them as `../schema/SCHEMA.md` section 10.1
  states and commits the version as pending, which takes effect as the next item states; the compiled version's
  declarations of kinds 8 to 10 and its entries are what Describe reports (section 8.3). Before stage 9.5 a
  tenant takes one Install, and its writes are fenced from the Install's check until the version takes effect
  (section 8.4).
- **Vector indexes.** In this stage a declaration of a vector index through the write path, kind 8 with flag bit 5, is
  refused with code 7, `detail` the record's index, its message naming the predicate and stating that vector indexes
  arrive with stage 8; the catalog is unchanged. A vector literal is still stored as a value. An Install whose sources
  declare `@index(ann)` is accepted: the declaration is recorded in the catalog version and reported as declared and
  unenforced (`UGS0902`), and no index is built before stage 8 (`../schema/SCHEMA.md` section 14). A write of a
  `ugdb:bfloat16vector` value is refused with code 7 until stage 8, `detail` the term's index, the message naming the
  stage.
- **A new version.** At the Commit, the frontend builds a catalog version from the declarations on the version current
  when the transaction began, and commits it in the metadata group as pending. A version built on one no longer current
  is refused with code 2 (Conflict), kind 5 (Catalog), the message naming both versions, and the client runs its
  transaction again against the current one; an offer resent after a lost reply is refused the same way, and Describe
  shows whether the first took effect. A version whose bytes pass `catalog_bytes_max` (section 11.9) is refused at its
  Commit with code 7, `detail` the bound, the message naming its size, and the catalog is unchanged; an Install's is
  refused the same way (section 8.4).
- **Its epoch.** At its next close `c + 1` the sequencer assigns every pending version the effective epoch `c + 2`,
  records the assignment in its durable close record and announces it with that close (section 11.12, *Epochs*); the
  metadata group learns the epoch from that close, and a version pending when the frontend crashes is assigned at the
  next close after recovery. Rounds of epoch `c + 2` and later are validated against the version, earlier ones
  against the version before. The Commit reply answers once the epoch is assigned and, for a version that tightens a
  predicate, once every partition holding the tenant at that epoch has passed its check: `commit_version` the first
  version of the effective epoch, `(c + 2) << 22`, the token zero, and no results. A check the data contradicts in any
  partition refuses the Commit with code 7, `detail` 0, the message naming the predicate and the subject or object at
  fault, and the metadata group commits a version restoring the previous declaration, so the refused change alters no
  data.
- **A tightening's check.** A version tightens a predicate when, against the version it is built on or the default
  entry, the predicate becomes single-valued, becomes an edge or a property from another kind, or takes a datatype
  constraint other than the one it held; it loosens a predicate when it makes it multi-valued from single-valued, of
  another kind from an edge or a property, or changes or removes a datatype constraint it held. An edge becoming a
  property does both. A transaction's declarations that tighten one predicate or aspect and loosen another are refused at
  the Commit with code 7, `detail` the index among them of the first that loosens, the message naming its predicate, and
  the catalog is unchanged: what a loosening lets in during the check would contradict the version restoring it. The
  client commits the loosening, then the tightening. The check runs as follows.
  - *The wait.* The frontend's metadata group holds the version as being checked from its offer, and each partition's
    report (section 13.19) durably. The frontend waits `catalog_check_ms` (section 11.9) from the close that assigned
    the epoch, for a report from every partition the placement map gives the tenant at that epoch. A partition with
    none of the predicate's data reports that the check passed.
  - *Every partition passed.* The version stands, and the Commit is answered as *Its epoch* states.
  - *A partition refused.* The group withdraws the version, and the Commit is refused as *Its epoch* states.
  - *A partition silent past the wait.* A partition that reports nothing within `catalog_check_ms`, whether stopped,
    unreachable or still scanning, withdraws the version the same way: the Commit is refused with code 24 (TimedOut),
    `detail` the lowest partition silent, the message naming it and the version. The change did not happen, so nothing
    is in doubt, and the client runs its transaction again on a new Begin. A partition that fails during its check and
    restarts is delivered the close again and checks again, and its report counts when it arrives within the wait.
  - *The restoring version.* Withdrawing is one command with the offer of the next version, built on the current one:
    each declaration the withdrawn version changed and no later version has changed since returns to what the version
    before it held, or goes when that version held none. It is durable before the Commit's refusal, and takes effect
    as a version does. It loosens what the withdrawn version tightened, so no check runs on it and no data refuses it.
    A report of a version no longer being checked is answered and changes nothing.
  - *While a version is checked.* Another version built on it is refused with code 2, kind 5, the message naming the
    version being checked, and the client runs its transaction again once the Commit before it is answered; a Drop's
    version (*Drops*) is still built. Describe reports the latest version in effect at the latest close before any
    being checked, leaving out one withdrawn, so a client never reads a declaration a check may yet withdraw.
  - *Writes.* A partition validates writes against the version from the round that takes it effect in that partition,
    a predicate it refused keeping the previous declaration until the restoring version arrives. A write it refuses
    meanwhile wrote nothing and is true of that version whether or not the version is later withdrawn; a client runs it
    again. So no write the version forbids lands in a partition after its check.
  - *A frontend that stops.* The check is the metadata group's, so a restarted frontend takes it up and waits
    `catalog_check_ms` anew, the partitions sending again each report left unanswered. The client's connection closes
    under the Commit, which leaves it in doubt as a lost reply does, the one way a tightening's outcome is left in doubt:
    Describe shows what took effect once the check ended, and until then a transaction built on the version is
    refused as above. A client whose connection closes changes nothing about the check.
- **Transactions against a version.** A read-write transaction that executed against another catalog version than the
  one in effect at its commit epoch, or than the last its partition's rounds took effect, its commit-time operations
  included, aborts with code 2, kind 5 (Catalog), and its retry executes against the version in effect. A prepared
  statement's schema id changes with each version (section 11.7).
- **Default entries.** A write of a predicate the version in effect does not declare is validated with the default
  entry: kind either, multi-valued, no datatype constraint and no index. Its partition asks the metadata group to
  record that entry (section 11.12), keyed by the tenant and the predicate and belonging to no namespace, a command that
  finds the record when it exists, creates no catalog version, stales no prepared statement and aborts no transaction. A
  drop of a predicate, a graph or a tenant's data leaves the record, which Describe names as recorded (section 8.3); a
  drop of everything in a tenant removes the tenant's records with its catalog.
- **Index builds.** An index build a version starts begins in each partition at the version's effective epoch, from
  the data at the epoch's first version, walking its source at that version in paced beats beside compaction
  (`../storage/STORAGE.md` section 16.7); until a partition's build completes, reads of that partition use the interim
  declaration, without the indexes being built or removed. A build a restart interrupts resumes from its build record
  as section 12.2's recovery states, and has not failed. A build that fails is reported to the metadata group, which commits a
  version restoring the previous declaration (T48, C4).
- **Drops.** A Drop reaching one partition commits there (section 6.5). The frontend then removes a dropped predicate's
  declaration, or sets a tenant's catalog to the initial catalog and removes its default-entry records, by a catalog
  version it builds on the current one,
  taking effect as above; until a dropped predicate's removal takes effect, the partition maintains no index of it. The
  frontend commits that version as pending before it relays a Drop's reply, stored or new, and a version that would change
  nothing is not built; so a frontend that stopped in between leaves the step to the client's resend, whose outcome is
  otherwise in doubt. A Drop through no session has no resend; a client that lost its reply runs the Drop again, which
  removes nothing more and builds this version. A Drop reaching several partitions is refused with code 23.

### 11.14 Finding compute nodes

From stage 5.9 a cluster may run several compute nodes, and a client finds them from the cluster itself
(`../../openspec/designs/UltraGraphDB Distribution Architecture.md`, *Clients: discovery, tags and load*).

**Seeds.** A client is configured with one or more seed addresses, each of the form of an advertised address (section
5.4) or a name resolving to several. It tries them in the configured order, with backoff; a refused authentication
ends the attempt for all of them. Seeds serve the first contact, and a client that can reach no node of its list
returns to them, keeping the version it holds: versions only rise within one cluster.

**The node list.** The list of compute nodes is state of the metadata state machine, so one version names one list on
every replica and across a restart. A compute node sends, in this order, a Join with the address it advertises to
clients, its tags and its state (section 13.1), a Subscribe (section 13.3) and a Roster (section 13.26). A Join adds
its entry or changes it, and the entry belongs to the connection of the latest Join for its address; a Join for an
address another connection holds replaces that entry, and the other connection's end then changes nothing. An entry
leaves by a logged removal, which the system group proposes once the entry's connection has been closed for two
closes, counted from the later of the connection's close and the system group's start to listen. The list's version is
the epoch of the close that announced its latest change: an EpochClose names a change with `nodes` 1 (section 13.4), a
compute node receiving one reads the list with Roster, and sends it to each of its authenticated client connections as
Nodes (section 5.4). A client swaps its list whole for one of a higher version, and never edits its list itself.

**What a client does with the list.** A client never waits for Nodes: until a list arrives it uses the connection it
has, and it checks tags only once it holds a list. It keeps the connections a new list still names and dials a newly
listed node after a random delay. A listed node it cannot reach is skipped and tried again with exponential backoff
and jitter.

**Tags.** A tag names a group of compute nodes, and a node carries one to eight. Tags are an open set that an operator
extends; `query` is the group a client uses when it names none. A client picks a tag and sends its requests to serving
nodes that carry it. No request names a tag: a node knows its own. Tags choose the node alone, and the classification
of sections 11.2a and 11.11 chooses a request's path on it as before.

**Draining.** A compute node about to stop joins again with `state` 2, and stops no earlier than after it has sent
each of its clients a Nodes naming it draining. A client begins no transaction and sends no one-frame request on a
node its list names draining or names no longer. A transaction open there runs to its end, its reads included; a
read-only transaction, which has no limit on its life, ends when the node stops.

**A tag a node lacks.** From stage 7 a procedure a query calls declares the tag it needs. A node that does not carry it
refuses the query when it compiles it, before any work, with code 32 (TagRequired), and a Prepare or an Execute that
compiles it is refused the same way. A client asked for a tag no serving node of its list carries refuses the request
itself, naming the tags the list holds.

**Load.** How much a node accepts and which node a client picks are stage 8's to specify: credit to send, granted by
the node, and the node's requests in flight and latency on its replies and on a probe. Nothing in this version carries
them, and `max_streams` bounds a connection as section 11.2 states.

## 12. Internal to the server and the log

No client sends or receives these. Each partition's log stores each of its rounds as a Round frame, header and body,
and stage 8's replication will carry the same bytes. A Round frame's header holds `sequence` its round's `position`,
`stream` 0, `flags` 0, `protocol` 1, `view` 0, `replica` 0 and `release` the release of the node that wrote it. The
record is logical: it holds what each entry decided as quads and system IDs, never the entries the store derives from
them, and replaying it derives the same state on every replica and in every release whose derivation version
(`../storage/STORAGE.md` section 3.2) it names. What apply cannot compute from the entries alone, a count row's degree
before the round and the round's `@limit` aggregate and statistics deltas, execution derives and the round carries in
its derivations section, so apply adds and reads no tile (T48, C1).

### 12.1 Round (17)

The log record of one partition's round: each entry's decision in round order, the write-set record of each committed
one or, for an Ingest share, its body as received with its resolution (T39), and the dictionary entries the round
creates, the derivations its execution made and the sessions it evicts. An entry is a transaction, an Ingest share, a
Drop, a Purge, a registration the partition accepted as an entity's home, which its commit step admits as it admits a
transaction, or an index build's start or completion.
Committed entry k of the round (counting committed ones from 0) has commit version `first_version +
2k`. A round never spans a close: one delivered to its partition ends the open round, so its entries carry one epoch,
`first_version >> 22`,
and each tenant's records name the one catalog version in effect at it.

| Command header | Width | Field | Type |
|---|---|---|---|
| 0 | 8 | `position` | u64: the round's place in the log, gapless from 1 |
| 8 | 8 | `parent` | u64: the previous round's frame `checksum` |
| 16 | 8 | `incarnation` | u64: the log's incarnation, raised each time a log is created afresh |
| 24 | 8 | `first_version` | u64: the commit version of the round's first committed entry; for a round holding no committed entry, the version its first committed entry would have taken, the epoch's next unused version, which the round consumes for nothing else (T48) |
| 32 | 4 | `transactions` | u32: the decision count, one per entry |
| 40 | 8 | `derivation` | u64: the derivation version the round was applied under (`../storage/STORAGE.md` section 3.2), any value |
| 48 | 8 | `horizon` | u64: the fold horizon (`../storage/STORAGE.md` section 14.2) the partition's primary stamped on the round, 0, a version or a version bound (section 1), never below the round before's |
| 56 | 1 | `flags` | u8: bit 0 `checkpoint`, a round the primary logs while `GridFull` holds its partition, whose position `../storage/STORAGE.md` section 17.1 takes as a checkpoint's (T48, C10); bits 1 to 7 zero |

| Section | Contents |
|---|---|
| 0 | decisions, 16 bytes each |
| 1 | the record table, 16 bytes per committed entry |
| 2 | the write-set records (section 12.2) |
| 3 | the dictionary entries the round creates, in the key-value layout of section 12.2: each its record kind (`../storage/STORAGE.md` section 9.3, kinds 1 to 3) as one byte followed by the record's key, and its value, each once, in ascending key order; an entry another partition's entity holds there, copied at the partition's first reference to it, counts in both directions as the round's |
| 4 | the Ingest bodies, block-aligned (section 4.1): each committed share's body as its frame carried it, the data before any chunk table, each starting at a frame offset that is a multiple of 4,096, the bytes between zero |
| 5 | the Ingest resolutions, one per Ingest body |
| 6 | the derivations, in the key-value layout of section 12.2, each key starting with its derivation kind as one byte: 1 a count row's degree before the round, one entry per count row the round's entries change, the rest of its key the row's conflict key (section 12.2, conflict family 4), naming the tenant, the graph, the predicate, the direction and the counted node, its value the degree, a u64; 2 a `@limit` aggregate's delta, one entry per aggregate the round changes, the rest of its key the tenant as a big-endian u64 followed by the aggregate's key (`../storage/STORAGE.md` section 9.3, kind 19), its value the round's change in kind 19's value layout; 3 a statistics delta, one entry per (graph, predicate) the round changes, the rest of its key the tenant as a big-endian u64, the graph's and the predicate's system IDs, its value the round's change in `../storage/STORAGE.md` section 9.4's format; each once, in ascending key order (T48, C1) |
| 7 | the evictions, 16 bytes each: the sessions the close delivered before the round evicted (section 11.5), each a session's identity (section 1), ascending; empty in every round but the first the partition logs after such a close (T48, C3) |

| Decision offset | Width | Field | Type |
|---|---|---|---|
| 0 | 2 | `outcome` | u16: 0 committed, else the error code of the abort |
| 2 | 2 | zero | 0 |
| 4 | 4 | `record` | u32: the committed entry's record index, counting from 0; `ABSENT` for an abort |
| 8 | 8 | `detail` | u64: the abort's detail; 0 for a commit |

| Record table offset | Width | Field | Type |
|---|---|---|---|
| 0 | 4 | `offset` | u32: a write-set record's offset in section 2; an Ingest body's offset in section 4 |
| 4 | 4 | `len` | u32: its length |
| 8 | 4 | `resolution` | u32: an Ingest entry's resolution's offset in section 5; `ABSENT` for a write-set record |
| 12 | 4 | `resolution_len` | u32: that resolution's length; 0 for a write-set record |

The first write-set record starts at section 2's offset 0 and each later one at `align8` of the one before's end; the
first resolution likewise in section 5; the first Ingest body at section 4's offset 0 and each later one at the least
offset at or above the end of the one before whose frame offset is a multiple of 4,096. The padding between is zero, and
each of the three sections ends with its last record.

A round's derivations are every additive delta apply cannot compute from the write-set records alone, and what
execution read that apply needs: each count row's degree before the round, which apply adds the round's changes to;
each `@limit` aggregate's change, which needs the value an instance held before the round, a sum's attribute or a
grouping's; and each statistics change, which needs execution's check of whether an inserted statement's quad is new,
since an entry of section 0 holding a statement ID adds it to a quad held already or not (section 12.2), and of how
many statements an emptied object held (T48). Apply writes each `@limit` and statistics delta as the additive record
the store folds (`../storage/STORAGE.md` section 9.3, kinds 19 and 9), and derives referrer-set members (kind 7) from the
records' registrations, so the round carries no prior value of any of them (T48, C1). A round's evictions take effect at it: apply deletes each named session's
entry from the partition's session table (`../storage/STORAGE.md` section 9.3, kind 10) at the round's `first_version`,
before the round's entries, a round holding no committed entry deleting at the version its `first_version` names, so
every replica and replay holds the same table at every position (T48, C3).

A primary whose partition `GridFull` holds (`../storage/STORAGE.md` sections 12.4 and 15.6), while its delta holds
changes or an extent has been released since the last checkpoint, logs a round with `checkpoint` set, which may hold no
entry, and the store takes its position as a checkpoint's, so a logged fact frees the
extents its checkpoints released (T48, C10).

An Ingest entry's resolution is what the partition decided for a body it logs as received: the body layout of section
4.1 with a 64-byte fixed part and three sections, its offsets counted from its first byte.

| Fixed part | Width | Field | Type |
|---|---|---|---|
| 0 | 8 | `tenant` | u64 |
| 8 | 8 | `catalog` | u64: the catalog version of `tenant` in effect at the round's epoch, which the share was validated against |
| 16 | 8 | `read_version` | u64: the version the share executed at, its partition's visibility watermark |
| 24 | 16 | `session` | session (section 1): the share's, all zero for none |
| 40 | 8 | `request` | u64: the share's request number, 0 for none |
| 48 | 8 | `principal` | u64: the principal the share acted for |
| 56 | 4 | `share` | u32: the share's number, at least 1 |
| 60 | 4 | zero | 0 |

| Section | Contents |
|---|---|
| 0 | per entry of the body's term table, in table order, the 16-byte system ID it resolved to; 16 zero bytes for a literal and for a term no block of the body names |
| 1 | the statement IDs, 24 bytes each, one per edge the share gave a statement ID, ascending by `block`, then `entry`: `block` u32, a relation block's index in the body; `entry` u32, the neighbour's index in its block; then the statement ID, 16 bytes |
| 2 | the skipped, 12 bytes each, ascending by `block`, then `row`, then `entry`: `block` u32, a block's index in the body; `row` u32, the row's index in its block; `entry` u32, the neighbour's index in its block for an edge skipped, or `ABSENT` for the row skipped whole (T48, C45) |

A share skips a row whose `@idempotency` value an instance of its type, or an earlier row of the share, already claims,
writing none of its values, and an edge already visible at its `read_version`, whose statement it would log again
(section 6.8); apply and replay skip each row and edge section 2 names, and add the rest. The resolution and the body
hold everything replay needs: each block's statements, quads of the resolved system IDs and the column values, the
statements it skipped, and its session-table entry, the share's reply rebuilt from the round. The conflict keys of the
share's inserts (section 12.2) follow from the two and are not logged.

Order: unused bytes 36–39 and 57–127 zero; `horizon` 0, a version or a version bound (`FieldValue` `"horizon"`);
`flags` bits 1 to 7 zero (`FieldValue` `"flags"`); sections; decisions a whole
number of 16 bytes (`SectionLength`); their count equals `transactions` (`CountMismatch` `"decisions"`); each decision:
bytes 2–3 zero (`PaddingNotZero`); a commit's `record` equals the commits before it (`FieldValue` `"record"`) and its
`detail` is 0 (`FieldValue` `"detail"`); an abort's `outcome` is an error code (`FieldValue` `"outcome"`) and its
`record` is `ABSENT` (`FieldValue` `"record"`); the last committed entry's version within `first_version`'s epoch,
its sequence at most 2²² − 2 (`FieldValue` `"first_version"`); the record table a whole number of 16 bytes; one entry per
commit (`CountMismatch` `"records"`); each entry: with `resolution` `ABSENT`, `resolution_len` 0 (`FieldValue`
`"record"`), the record inside section 2 (`IndexOutOfRange`, section 1), at the expected offset (`FieldValue`
`"record offset"`), padding zero (`PaddingNotZero`), the record valid (`Record`, wrapping the record's refusal); with
`resolution` not `ABSENT`, the body inside section 4 and the resolution inside section 5 (`IndexOutOfRange`, section 1),
each at its expected offset (`FieldValue` `"record offset"`), padding zero (`PaddingNotZero`), the body valid as an Ingest
request's (section 6.8) and the resolution valid (`Record`, wrapping the refusal); and the records and resolutions of one
tenant, entry kind 2 aside, naming one `catalog` (`FieldValue` `"catalog"`); sections 2, 4 and 5 each ending with their
last record (`TrailingBytes`); a resolution valid when: its fixed part and table hold (section 4.1), bytes 60–63 zero
(`PaddingNotZero`), `share` at least 1 (`FieldValue` `"share"`), section 0 16 bytes per entry of its body's term table
(`CountMismatch` `"resolution terms"`), each ID all zero or a valid system ID (`FieldValue` `"resolution term"`),
section 1 a whole number of 24-byte entries (`SectionLength`), each naming a relation block of its body and a neighbour
below that block's `entries`, after the entry before, its ID a valid system ID (`FieldValue` `"resolution
statement"`), section 2 a whole number of 12-byte entries (`SectionLength`), each naming a block of its body and a row
below that block's `rows`, its `entry` `ABSENT` or, in a relation block, a neighbour of that row, after the entry before,
and no entry of a row an entry skips whole (`FieldValue` `"resolution skipped"`); section 3's entries each inside the
section (`EntryOutOfBounds`) with zero padding (`PaddingNotZero`), every value present (`FieldValue` `"dictionary
entry"`), each key above the one before, compared byte by byte (`FieldValue` `"dictionary order"`); section 6's entries
likewise inside the section with zero padding, every value present, each key's first byte 1, 2 or 3, after it for kind
1 a conflict key of conflict family 4 and a value of 8 bytes, for kind 2 at least 12 bytes and a value of 8, 16 or 24
bytes, for kind 3 exactly 40 bytes and a value valid in `../storage/STORAGE.md` section 9.4's format (`FieldValue`
`"derivation"`), each key above the one before (`FieldValue` `"derivation order"`), and the section
ending with its last entry (`TrailingBytes`); section 7 a whole number of 16
bytes (`SectionLength`), each eviction not all zero and above the one before, compared byte by byte (`FieldValue`
`"eviction"`, the eviction's index).

Replaying a round whose `derivation` is not the reader's own derivation version (`../storage/STORAGE.md` section 3.2) is
refused with an error naming both (`DerivationMismatch`), and applies nothing of the round; so is a round whose `horizon`
is below the round before's, naming both horizons. The golden `round.frame` follows this layout once the code change
that brings `ultragraphdb_protocol_wire` to it regenerates it: this file leads the code.

### 12.2 Write-set record

What a partition builds for a transaction, and what its commit step logs for each committed entry: a
transaction's resolved quads with their system IDs, its drops and deletes in their logical form, its session-table
entry, the registrations it relied on as a referrer, and the conflict keys of its writes. The commit step writes it
again, once, for a transaction that ran commit-time operations: the operations' writes added and their results in
sections 9 to 11, so the log holds exactly what was applied and what was answered. It is the body layout of section
4.1 with a 64-byte fixed part and 12 sections; its offsets count from the record's first byte.

| Fixed part | Width | Field | Type |
|---|---|---|---|
| 0 | 1 | `entry_kind` | u8, the entry kind (T48, C48): 1 a transaction or a Drop, 2 a registration the partition accepted as an entity's home, 3 a catalog version taking effect or an index build's completion |
| 1 | 7 | zero | 0 |
| 8 | 8 | `tenant` | u64 |
| 16 | 8 | `catalog` | u64: entry kinds 1 and 3, the catalog version of `tenant` in effect at the round's epoch, which the entry was validated against; 0 for entry kind 2 |
| 24 | 8 | `read_version` | u64: entry kind 1, the version the transaction read at, its partition's visibility watermark when it bound; 0 otherwise |
| 32 | 16 | `session` | session (section 1): entry kind 1, the session committing through it, all zero for none; zero otherwise |
| 48 | 8 | `request` | u64: entry kind 1, the session's request number, 0 for none; 0 otherwise |
| 56 | 8 | `principal` | u64: entry kind 1, the principal the transaction acted for (`../../openspec/notes/SECURITY.md`); 0 otherwise |

| Section | Contents |
|---|---|
| 0 | statements inserted: key-value entries, each key a quad, its value the statement's ID, 16 bytes, or present and empty for a statement without one |
| 1 | statements removed, in section 0's layout: the statements the transaction deleted, a delete by pattern as the statements it resolved to |
| 2 | statement records: key-value entries, each key a graph and a statement ID, its value the statement's record, its properties included, or `ABSENT` for its removal; a statement ID names one quad, so the graph is that quad's and the store keys the record by the ID alone (`../storage/STORAGE.md` section 9.3, kind 18; T48, C22) |
| 3 | operations, 24 bytes each |
| 4 | registrations, 32 bytes each |
| 5 | the commit record the client attached, bytes |
| 6 | conflict points: entries whose key is a conflict key written and whose value is present and empty |
| 7 | conflict statements: entries whose key is a forward or reverse row's conflict key and whose value is the neighbour it changed in its logical form, a system ID, a term encoding or a sortable value (`../storage/STORAGE.md` section 18.3) |
| 8 | conflict ranges: entries whose key is a range's first conflict key and whose value is the conflict key past its last, empty for the end of every key |
| 9 | the commit-time operations' results, 16 bytes each in the Commit reply's result layout (section 6.3), in the order the transaction asked them; empty for a transaction that asked none |
| 10 | the results' term table's records (section 4.4), as the Commit reply holds them |
| 11 | the results' term table's text |

A quad is its graph, subject and predicate, each a 16-byte system ID and the default graph its own encoding, and its
object, a system ID or a literal's term encoding, in the encoding the round's `derivation` names. Every entry the store
derives from the record, a reverse, index or count entry, a dictionary entry aside, is derived on apply and on replay
and never logged; what such a derivation needs that the record does not carry, a count row's degree before the round,
is in the round's derivations section, and so are the round's `@limit` aggregate and statistics deltas, which apply
writes as they are logged (section 12.1; T48, C1). Sections 0 and 1 hold what the transaction changed, never an ID an object
already held and keeps: an entry of section 0 holding an ID adds it to the IDs its quad holds, an entry of section 1 holding one
removes it and leaves the object held, and an empty entry of section 1 removes the object with every ID. So a parallel edge logs one
entry whatever its object holds, and `statements_per_object_max` (section 11.9) is reached through commits.

| Entry kind | Fixed part | Sections |
|---|---|---|
| 1, a transaction | every field | 0 to 2 and 5 to 11 as it wrote and answered; 3 one operation 8 per entity it deleted at its home, else empty; 4 the registrations it relied on as a referrer, each the entity's ID, the bucket group and the generation the home's reply named |
| 1, a Drop | every field, `read_version` 0 | 3 one drop, operations 1 to 4; 8 the conflict ranges the drop covers, below; every other empty |
| 1, a Purge | `tenant`, `catalog` and `principal`; every other field 0 | 3 one operation 9 per instance the purge removes in the partition; every other empty |
| 2 | `tenant`; every other field 0 | 4 one registration: the member it adds or renews, at the entry's commit version; or none, for an entity the home got or created for a registration naming no bucket group, whose dictionary entries the round's dictionary section carries; every other empty |
| 3 | `tenant`, `catalog`; every other field 0 | 3 one operation, 6 or 7; every other empty |

| Operation offset | Width | Field | Type |
|---|---|---|---|
| 0 | 2 | `operation` | u16: 1 drop a predicate, 2 drop a graph, 3 drop a tenant's data, 4 drop everything in a tenant, 6 a catalog version takes effect, 7 complete a build, 8 delete an entity, 9 purge an instance; 5 is unassigned |
| 2 | 6 | zero | 0 |
| 8 | 16 | `argument` | bytes: for 1 and 2, the dropped predicate's or graph's system ID; for 6, the catalog version taking effect, a u64 in bytes 8–15, and bytes 16–23 zero; for 7, the catalog version that started the build, a u64 in bytes 8–15, byte 16 its outcome, 1 a swap and 2 an undo, and bytes 17–23 zero; for 3 and 4, zero; for 8 and 9, the entity's system ID |

| Registration offset | Width | Field | Type |
|---|---|---|---|
| 0 | 16 | `entity` | system ID (section 1) |
| 16 | 4 | `group` | bucket group (section 1) |
| 20 | 4 | zero | 0 |
| 24 | 8 | `generation` | u64: entry kind 1, the generation the home's reply named; entry kind 2, the member's generation the registration leaves, at least 1 |

- **A drop** (operations 1 to 4, the Drop command's kinds 1 to 4) removes what section 6.5 says of its kind in the
  partition, which derives the key removals on apply and on replay. Its record holds the conflict ranges it covers in section
  8, so a transaction reading or writing into them below the drop's version conflicts; it is the last record of its
  round.
- **An entity delete** (operation 8), which UGS brings with stage 6.6 (`../schema/SCHEMA.md` section 8.5) and a transaction
  asks for by Operation kind 5 (section 7.3), is part of its transaction's record at the entity's home, P24's rule 8: sections 1 and 2 remove the entity's statements and
  statement records, those of the relation instances its `@on_delete` actions reach in the partition among them, and
  the operation writes the entity's tombstone (`../storage/STORAGE.md` section 9.3, kind 8), which
  `../schema/SCHEMA.md` section 12.4 keeps at the home under its ID. The tombstone fences the ID for good: a later commit writing it as a subject aborts with code 30
  (EntityDeleted), one naming it as a participant is refused with code 7 and `UGS1019` (`schema/SCHEMA.md` section 8.5), a
  registration naming it is answered with outcome 30 or, in get-only mode, as deleted (section 13.15), and a write
  creating its IRI again allocates a new ID, whose ID-to-term record holds the ID it replaced in its value
  (`../storage/STORAGE.md` section 9.3, kind 2), and deletes the IRI's term-to-ID record of the replaced ID at its
  version (kind 1, class M); the tombstone holds the delete's version, and a read naming the IRI at a version below the
  new ID's creation resolves the replaced ID (`../schema/SCHEMA.md` section 8.5; T48, C16). A delete whose entity's referrer set names a bucket group another partition
  holds is cross-partition, code 23, until stage 8 sends each member a sequenced share that fences the ID there.
- **A retraction**, which a transaction asks for by Operation kind 6 (section 7.3), is statements alone and has no
  operation of its own: sections 0 and 2 hold the `asserted` end it writes on the instance and on each relation
  instance its `@on_retract` actions retract in the partition (`../schema/SCHEMA.md` sections 8.4 and 12.4). A relation
  instance's delete is statements alone likewise, in sections 1 and 2.
- **A purge** (operation 9), a Purge's record (section 8.5), removes at the instance's home every version of its
  attributes, its statement record, its index and derived entries, its dictionary entries and its referrer set, and at a
  member of its referrer set every edge naming it, with their index entries and marks (`../schema/SCHEMA.md` section 8.8). It
  writes a purge marker (`../storage/STORAGE.md` section 9.3, kind 20), keyed by the instance's system ID and holding the
  purge's version, at the home and, through the purge's share, at each member of the referrer set: reads at every version
  treat every entry naming the instance as absent, rows, neighbours, columns and records alike, and compaction removes
  them, class I entries included (T48, C2). It is the last record of its round, as a drop's is.
- **A catalog version takes effect**, operation 6, in a round of its own at the first version of its effective epoch:
  the partition declares the version's declarations, checked against its data, and removes the predicates, entity
  types and relations the version no longer names, and on replay does the same from the version the metadata group
  holds, so every later record is derived under the catalog its first apply had. A version removing a tokenizer of an
  index, and starting no build of that index whose swap removes it, removes that tokenizer's index families as a drop
  removes families: its round writes a drop marker naming
  operation 6 and the version, which covers them (`../storage/STORAGE.md` sections 9.3, kind 14, and 15.5; T48). A
  version adding an index to a
  predicate holding data starts that index's build there, from the data at that version, and the commit step records
  the nodes later writes change during it. A build's completion, operation 7, names the version that started it and
  its outcome. A swap makes the built index family (`../storage/STORAGE.md` section 16.7) live at the build's version
  and removes the index families of the tokenizers the version no longer declares; an undo, after a failed build,
  removes the built family and restores the previous declaration; replay undoes the build again, whatever its backfill
  does then. A round
  holds at most one record of entry kind 3 and closes after a drop and before the first transaction of an epoch at which a
  version takes effect; versions taking effect at one epoch take effect in the order of their versions, each in a
  round of its own, and one waits while a build another started runs.
- **A registration a home accepts** (entry kind 2) adds or renews its member in the entity's referrer set at the entry's
  commit version, a new member with the round's epoch as the epoch it was added at. The entity's dictionary entries
  it created are in the round's section 3. One holding no registration records an entity the home got or created for
  a registration naming no bucket group, a predicate's or a graph's: its dictionary entries are its whole effect.
- **A session-table entry** is an entry-kind-1 record's `session` and `request` with its stored reply, the reply's
  command, its 128-byte command header and its body: a Commit reply, or a Drop reply for a record holding a drop, rebuilt
  from the record, its commit version, its round's position and, for a Drop of a predicate or graph, the dropped IRI's
  dictionary entry. An Ingest share's is its resolution's `session`, `request` and `share` with the share's Ingest reply
  (section 12.1). The table holds one entry per session, keyed by the session alone, its last request replacing the one
  before (section 11.5; T48, C3).

**Conflict keys** name what a write changes and a read depends on in logical conflict families, never in the store's
tiles or its families, so a tile's split or move changes no conflict (T38; T48, C48). A conflict key is the tenant as a
big-endian u64, the graph's system ID, the predicate's system ID, the conflict family's byte, then the conflict family's
key; a conflict family with no graph or no predicate holds 16 bytes of `0xFF` in its place, which no system ID holds, so
it orders after every graph and every predicate.
Keys compare byte by byte, so a tenant's keys, a graph's in a tenant and a predicate's in a graph are each one range.

| Conflict family | Byte | Graph | Predicate | Key |
|---|---|---|---|---|
| Forward row | 1 | the statement's | the statement's | the subject's bucket group as a big-endian u32, then its system ID, so a bucket group's rows are one range (T38) |
| Reverse row | 2 | the statement's | the statement's | the object's canonical key: a system ID, or a literal's term encoding |
| Index token | 3 | the statement's | the indexed predicate | the token (`../storage/STORAGE.md` section 5.3) |
| Count | 4 | the statement's | the counted predicate | the direction, 1 a subject's count or 2 an object's, then the counted node's canonical key |
| Statement | 5 | the statement's | the statement's | the statement's ID |
| Mark | 6 | `0xFF` × 16 | `0xFF` × 16 | the entity's system ID, then the referring bucket group as 3 big-endian bytes |
| Claim | 7 | a key claim's and a `@unique` claim's graph; else `0xFF` × 16 (T48, C15) | a `@unique` claim's predicate; else `0xFF` × 16 | the claim's kind, 1 a key, 2 a `@unique` value, 3 an idempotency value, 4 a `@set` combination, 5 a `@limit` aggregate (T48, C21), then its key as the store's record of that claim holds it, a `@unique` claim's after its predicate (`../storage/STORAGE.md` section 9.3, kinds 4, 5, 13, 12 and 19) |
| Session | 8 | `0xFF` × 16 | `0xFF` × 16 | the session's 16 bytes (T48, C3) |

A `@limit` check records its aggregate's key, claim kind 5, in the transaction's reads, and a write changing the
aggregate writes that key among its conflict points (T48, C21).

A range's past key is the least key above every key its prefix begins: the prefix with its last byte below `0xFF`
raised by one and the bytes after it dropped, or empty when every byte is `0xFF`. A drop's conflict ranges are: a
tenant's data and everything in a tenant, the tenant's prefix; a graph, the tenant and the graph; a predicate, the tenant,
each graph holding it in the partition and the predicate, which holds its `@unique` claims too (T48, C15).

A key-value section is entries back to back, each 8-aligned within the section:

| Entry offset | Width | Field |
|---|---|---|
| 0 | 4 | `key_len` u32 |
| 4 | 4 | `value_len` u32, `ABSENT` for a removal, which holds no value |
| 8 | `key_len` | the key |
| 8 + `key_len` | `value_len` | the value |
| | to a multiple of 8 | zero |

Order: the fixed part and table (section 4.1); `entry_kind` 1 to 3 (`FieldValue` `"entry kind"`); bytes 1–7 zero
(`PaddingNotZero`); the fixed fields the entry kind leaves 0 (`FieldValue` `"record"`); sections 0, 1, 2 and 6 to 8 in turn,
each entry inside its section (`EntryOutOfBounds`) with zero padding (`PaddingNotZero`); every value of sections 0 and 1
present, empty or of 16 bytes (`FieldValue` `"statement id"`); every conflict point's value present and empty
(`FieldValue` `"conflict point"`); every conflict statement's and conflict range's value present (`FieldValue`
`"conflict statement"`, `"conflict range"`); section 3 a whole number of 24-byte operations (`SectionLength`), each
`operation` 1 to 4 or 6 to 9 (`FieldValue` `"operation"`), bytes 2–7 zero (`PaddingNotZero`), its `argument` as the
table states (`FieldValue` `"argument"`); section 4 a whole number of 32-byte registrations (`SectionLength`), each
`entity` a valid system ID, `group` below 2¹⁸, bytes 20–23 zero and `generation` at least 1 (`FieldValue`
`"registration"`); every key of sections 6 to 8, and every value of section 8 not empty, a conflict key: a known
conflict family byte at offset 40 and, for conflict families 1 to 5, a graph and a predicate that are not `0xFF` × 16,
for conflict families 6 and 8, both `0xFF` × 16 (`FieldValue` `"conflict key"`); section 9 a whole number of 16-byte
results, each checked as section 6.3 checks a result, and the term table (10, 11) as section 6.3 checks it; the sections
each entry kind holds, as the table above states (`CountMismatch` `"record sections"`).

The keys' and values' contents, the quads' and the statement records' encodings and the commit record are the store's:
the server alone writes and reads them, and the derivation version and the store's mapping contract version them. This
file states their framing.

Recovery replays the log's rounds above the position of the store's last checkpoint (`../storage/STORAGE.md` section
17.2), in order. Each committed entry is applied at its commit version through the
derivation its round names: a transaction's quads, statement records and drop with every entry they derive, and its
committed request with its stored reply into the partition's session table; an Ingest share's statements, from its body
and its resolution, likewise, skipping what the resolution lists skipped; a registration into its entity's referrer
set; the round's dictionary entries; its derivations, the count rows' degrees, to which the count changes add, and the
`@limit` aggregate and statistics deltas, written as their records; and its evictions, before its
entries. A build whose start the log holds without its completion resumes from its build record as the checkpoint
holds it (`../storage/STORAGE.md` section 9.3, kind 15), reading its source at its start version, which the fold
horizon stays at or below while the build runs (`../storage/STORAGE.md` sections 14.2 and 16.7), so the log stays the only
synced stream (T48, C4). Each undeclared predicate a replayed round
first wrote is asked of the metadata group for its default entry again (section 11.12).

Checkpoints, the log's trim and recovery's refusals are `../storage/STORAGE.md` section 17's.

## 13. Bodies of role commands

Roles exchange these on role connections (section 11.12), in the frame of section 2 and the body layout of section 4.1;
a client never sends or receives one. A role kind is 1 the frontend, 2 compute, 3 storage; from stage 5.9 kind 1 is
retired and compute holds the client endpoint (section 11.12a). Each body's checks run in
the order its "Order" line gives, as for a client's commands. The side that accepted answers a body's refusal as a stream
error, and the side that dialled closes the connection for any frame it refuses, as section 2.6 has a client do.
System IDs appear here, between roles, and in the log alone; a role command naming a term by its text names it through
a request term table (section 4.4), interned in record order.

### 13.1 Join (512)

A storage or compute process registers with the frontend at the frontend's role address: its kind, its own role
address and, for storage, the partitions it hosts. The frontend answers with the same fields. A Join from a role
address already registered replaces that registration.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 1 | `kind` | u8 | 2 compute or 3 storage (`FieldValue` `"kind"`) |

| Section | Contents |
|---|---|
| 0 | the role's role address, UTF-8: `tcp://host:port` or `unix:///path` |
| 1 | the partitions it hosts, u32s, ascending; empty for compute |

Order: unused bytes 1–127 zero; `kind`; sections; section 0 UTF-8, beginning `tcp://` or `unix://` (`FieldValue`
`"address"`, the address's length in bytes); section 1 a whole number of u32s (`SectionLength`); its partitions at least
1 and ascending without repeat (`FieldValue` `"partition"`, the first partition breaking the rule); empty for kind 2 and
not empty for kind 3 (`CountMismatch` `"partitions"`, the partitions sent against 0 for kind 2 and 1 for kind 3).

From stage 5.8 a Join is sent to the system group (section 11.12a). From stage 5.9 a compute node's Join names what
the node list holds of it (section 11.14), in one more command-header field and three more sections:

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 1 | 1 | `state` | u8 | for compute, 1 serving or 2 draining; 0 for storage (`FieldValue` `"state"`) |

| Section | Contents |
|---|---|
| 0 | for storage, its role address as before; empty for compute, which nothing dials |
| 2 | for compute, the address it advertises to clients, UTF-8, `tcp://host:port` (section 5.4), at most 265 bytes; empty for storage |
| 3 | for compute, its tags: string list `ends` (section 4.3); empty for storage |
| 4 | its tags: string list `text` |

The order from stage 5.9, whole: unused bytes 2–127 zero; `kind`; `state`, 1 or 2 for kind 2 and 0 for kind 3; sections;
section 0 for kind 3 as before, and empty for kind 2 (`CountMismatch` `"address"`); section 1 as before; section 2 for
kind 2 an advertised address as section 5.4 states it (`FieldValue` `"advertised"`, its length), and empty for kind 3
(`CountMismatch` `"advertised"`); the string list (3, 4): for kind 2, one to eight tags, each 1 to 32 bytes of `a` to
`z`, `0` to `9` and `_`, ascending by their bytes without repeat (`FieldValue` `"tag"`, the tag's index), and none for
kind 3 (`CountMismatch` `"tags"`). A compute node's Join replaces the registration of its advertised address, whichever
connection made it, and its entry belongs to the connection of its latest Join (section 11.14). The system group
refuses, with code 15 (Limit), `detail` the bound, a Join that would take the list past 1,024 nodes or the cluster past
256 distinct tags. A Join is answered once it is durable in the metadata state machine.

### 13.2 Members (513)

The roles that joined: per storage partition, the role address of the process hosting it, and each compute process's
address. The requester sends three empty sections.

The command header is all zero.

| Section | Contents |
|---|---|
| 0 | members, 12 bytes each; empty in a request |
| 1 | addresses: string list `ends` (section 4.3) |
| 2 | addresses: string list `text` |

| Member offset | Width | Field | Type |
|---|---|---|---|
| 0 | 1 | `kind` | u8: 2 compute, 3 storage |
| 1 | 3 | zero | 0 |
| 4 | 4 | `partition` | u32: for storage, a partition it hosts; 0 for compute |
| 8 | 4 | `address` | u32: the role address's index in the string list |

Members come compute first, ordered by their addresses' UTF-8 bytes, then storage by partition, one member per storage
partition; addresses are interned in member order, so compute members' `address` indices ascend.

Order: command header bytes 0–127 zero; sections; the string list (1, 2); section 0 a whole number of 12-byte members
(`SectionLength`); each member in order: `kind` 2 or 3 (`FieldValue` `"kind"`), bytes 1–3 zero (`PaddingNotZero`),
`partition` 0 for kind 2 and at least 1 for kind 3 (`FieldValue` `"partition"`, the partition), `address` below the
string count (`IndexOutOfRange`, section 0, the member), and the member after the one before in the order above:
compute before storage, compute members' addresses ascending without repeat by their UTF-8 bytes, storage members'
partitions ascending without repeat (`FieldValue` `"member order"`, the member's index). A request and a reply share the
layout, and a body without members reads as either: the frontend, receiving a request, refuses one holding a member,
or any string, with `CountMismatch` (`"members"`, the members, 0).

From stage 5.9 Members holds storage members alone: a compute node has no role address, and Roster (section 13.26) is
the one list of compute nodes. A reply holding a member of kind 2 is refused with `FieldValue` (`"kind"`).

### 13.3 Subscribe (514)

Storage asks for the sequencer's epoch closes on a stream of its own (section 11.12). The frontend answers with
EpochClose frames on that stream, never flagged `LAST`, until the subscriber cancels the stream or the connection
closes. EpochClose frames spend no credit.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 8 | `from` | epoch (section 1) | the last epoch whose close the subscriber holds; 0 for none |

No sections. Order: unused bytes 8–127 zero; `from` at most 2⁴² − 1 (`FieldValue` `"from"`); sections. A `from` above
the latest epoch the sequencer closed is refused with Error 1 (Protocol), the message naming both epochs. A subscription
whose next close the sequencer no longer holds, at its Subscribe or at a later close, is ended with Error 1 on its stream,
the message naming the close it asked for: no frame can carry what is gone, and a storage process told so stops, as it
does for a frontend lost past its deadline, since asking again finds the same. A connection holds one subscription: a
second Subscribe while one is open is refused with Error 1 on its stream.

From stage 5.8 the role holding the client endpoint subscribes too (section 11.12a), and the command header holds one
more field: `latest`, a u8 boolean at offset 8 (`FieldValue` `"latest"`), checked after `from`, the unused bytes then
9–127. That role alone sends `latest` 1, and only with `from` 0; any other use is refused with `FieldValue`
(`"latest"`). With it the first close on the stream is the latest the sequencer holds durable, whatever its epoch, or
the close of epoch 1 when none exists yet, and each later close names the epoch one above the one before. The system
group ends a subscription whose frames have gone unacknowledged by the transport for twice `epoch_ms_max`, closing its
connection, so a subscriber that fell silent does not count as connected.

### 13.4 EpochClose (515)

One of the sequencer's closes, on a Subscribe's stream, as its durable close record holds it: the epoch; the wall-clock
time the sequencer recorded for its close; the catalog versions it assigned, each taking effect at the epoch one above
it; the placement version it assigned, at most one, taking effect at that epoch too (the subscriber reads its map with a
Placement request, section 13.6); and the sessions the metadata group's registry evicted since the previous close (section 11.12, *Epochs*). The
first close on a stream names the epoch one above the Subscribe's `from`, and each later one the epoch one above the one
before. A close whose lists do not fit one frame within the subscriber's `max_frame` continues in further EpochClose
frames of the same epoch and time, each but the last with `more` 1, whose lists the subscriber reads as one.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 8 | `epoch` | epoch (section 1) | 1 to 2⁴² − 1 (`FieldValue` `"epoch"`) |
| 8 | 8 | `time` | u64 | the close's time, in microseconds of Unix time |
| 16 | 1 | `more` | u8 | boolean: another frame of this close follows (`FieldValue` `"more"`) |
| 24 | 8 | `placement` | u64 | the placement version taking effect at `epoch + 1`; 0 for none |

| Section | Contents |
|---|---|
| 0 | catalog announcements, 16 bytes each |
| 1 | session evictions, 16 bytes each: each a session's identity (section 1) |

| Announcement offset | Width | Field | Type |
|---|---|---|---|
| 0 | 8 | `tenant` | u64 |
| 8 | 8 | `version` | u64: the catalog version taking effect at `epoch + 1`, at least 1 |

Order: unused bytes 17–23 and 32–127 zero; `epoch`; `more`; sections; sections 0 and 1 each a whole number of 16 bytes
(`SectionLength`); each announcement: `version` at least 1, and after the one before by tenant, then by version
(`FieldValue` `"announcement"`, the announcement's index in the frame); each eviction not all zero and above the one
before, compared byte by byte (`FieldValue` `"eviction"`, the eviction's index in the frame); both orders run on across
the frames of one close, which the subscriber checks across frames, as it checks the epochs below. The subscriber refuses, as connection
errors, a close whose `epoch` is not one above the previous close's, a continuing frame whose `epoch` or `time` is not
its predecessor's (`FieldValue` `"epoch"`), a continuing frame whose `placement` is not its predecessor's (`FieldValue`
`"placement"`), and a `time` below the previous close's (`FieldValue` `"time"`).

From stage 5.9 the command header holds one more field: `nodes`, a u8 boolean at offset 17 (`FieldValue` `"nodes"`), 1
when the list of compute nodes changed since the previous close. The list's version is then this close's `epoch`
(section 11.14). It is the same in every frame of one close (`FieldValue` `"nodes"`). The order is then: unused bytes
18–23 and 32–127 zero; `epoch`; `more`; `nodes`; sections, and the rest as before. A storage process reads the flag and
does nothing for it.

### 13.5 Lease (516)

A partition leases a block of sequences of one type from the metadata group: the half-open range `start` to `end` of
`size` sequences, durable in the metadata group before the reply, never leased again. The requester sends `start` and
`end` 0.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 2 | `type` | u16 | the type prefix, at least 1 (`FieldValue` `"type"`) |
| 4 | 4 | `partition` | u32 | at least 1 (`FieldValue` `"partition"`) |
| 8 | 8 | `size` | u64 | the sequences asked for, at least 1 (`FieldValue` `"size"`) |
| 16 | 8 | `start` | u64 | the block's first sequence, at least 1 |
| 24 | 8 | `end` | u64 | one past the block's last sequence, `start + size` |

No sections. Order: unused bytes 2–3 and 32–127 zero; `type`; `partition`; `size`; a body with `start` and `end` both 0
is a request; any other is a reply, with `start ≥ 1` and `end = start + size` within a u64 (`FieldValue` `"block"`,
`start`); sections. A `size` above the
metadata group's `block_size_max` is refused with code 7 (Refused), `detail` the maximum, the message naming the size
and the maximum.

### 13.6 Placement (517)

The placement map in effect at an epoch, read at a position of the metadata group's log: every bucket's partition,
each tenant's run and each partition's preferred leader, with the epochs between which the map applies, since each
change to the map names the epoch from which it applies. The requester names the epoch it resolves buckets at, 0 for
the latest map, with `position`, `from` and `until` 0 and three empty sections. A role resolving a bucket group at an
epoch, as a reverse read does, uses the map whose `from` and `until` hold that epoch.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 8 | `position` | u64 | in a reply, the metadata group's log position the map was read at |
| 8 | 8 | `epoch` | epoch (section 1) | the epoch the map is asked for; 0 for the latest |
| 16 | 8 | `from` | epoch (section 1) | in a reply, the epoch from which the map applies, at least 1 |
| 24 | 8 | `until` | epoch (section 1) | in a reply, the epoch from which the next change applies; 0 for none |

| Section | Contents |
|---|---|
| 0 | bucket ranges, 16 bytes each |
| 1 | tenant runs, 16 bytes each: one per tenant the metadata group holds, tenants sharing a run each listed with it |
| 2 | preferred leaders, 8 bytes each |

| Range offset | Width | Field | Type |
|---|---|---|---|
| 0 | 4 | `start` | u32: the range's first bucket |
| 4 | 4 | `end` | u32: one past its last bucket |
| 8 | 4 | `partition` | u32: the partition owning the range, from 1 |
| 12 | 4 | zero | 0 |

| Run offset | Width | Field | Type |
|---|---|---|---|
| 0 | 8 | `tenant` | u64 |
| 8 | 1 | `run` | u8: the top 8 bits of the tenant's buckets, `run × 65,536` to `run × 65,536 + 65,535` |
| 9 | 7 | zero | 0 |

| Leader offset | Width | Field | Type |
|---|---|---|---|
| 0 | 4 | `partition` | u32 |
| 4 | 4 | `replica` | u32: the partition's preferred leader, a replica numbered from 1 |

The ranges cover every bucket once, in order: the first starts at 0, each starts where the one before ends, the last
ends at 2²⁴ (16,777,216), and two neighbours name different partitions. Runs come by tenant, leaders by partition,
one leader per partition the ranges name.

A body with `from` 0 is a request; a reply's `from` is at least 1.

Order: unused bytes 32–127 zero; `epoch`, `from` and `until` each at most 2⁴² − 1 (`FieldValue` naming the field); in a
request, `position` and `until` 0 (`FieldValue` `"placement"`, the first nonzero value) and every section empty, the
check run after the sections are read (`CountMismatch` `"placement"`, the sections' bytes, 0); in a reply, `from` at most
a nonzero `epoch`, and `until` 0 or above both `from` and `epoch` (`FieldValue` `"placement epoch"`, `from` or `until`);
sections; each section a whole number of its records (`SectionLength`); each range in order: zero bytes
(`PaddingNotZero`), `start` where the one before ended and `end` above `start`, `partition` at least 1 and not its
neighbour's (`FieldValue` `"range"`, the range's index); the last range ending at 2²⁴, with no range ending at 0
(`FieldValue` `"range"`, the ranges' count); each run: zero bytes (`PaddingNotZero`), its tenant above the one before
(`FieldValue` `"run"`, the run's index); each leader: `partition` above the one before and `replica` at least 1
(`FieldValue` `"leader"`, the leader's index); the leaders' partitions exactly the distinct partitions the ranges name
(`CountMismatch` `"leaders"`, the leaders, the ranges' distinct partitions; refused too when the counts agree and the
partitions differ).

### 13.7 Catalog (518)

A tenant's catalog version from the metadata group: its number, the epoch it applies from and its bytes, which
`../schema/CATALOG.md` lays out and the protocol carries opaque. The requester names the tenant and the version, 0 for the tenant's current one, with
`epoch` and `position` 0 and section 0 empty. A version's bytes, at most `catalog_bytes_max` (section 11.9), that do not
fit one frame within the requester's `max_frame` continue in further Catalog frames on the stream, each with the first's
command header, the last alone flagged `LAST`; the requester reads section 0 as the concatenation of every frame's, in
order, and refuses a frame breaking the first's command header as `FieldValue` (`"catalog"`), and a stream whose
section 0 passes `catalog_bytes_max` likewise, reading no further frame of it. The frames spend credit as
Result frames do (section 11.3). A storage process reads with it each version an EpochClose announces
before it takes the close as delivered, and on recovery each version its partitions' untrimmed logs name, before they
replay (section 11.12).

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 8 | `tenant` | u64 | |
| 8 | 8 | `version` | u64 | the catalog version; at least 1 in a reply (`FieldValue` `"version"`) |
| 16 | 8 | `epoch` | epoch (section 1) | in a reply, the epoch from which the version applies; 0 while the version is pending |
| 24 | 8 | `position` | u64 | in a reply, the metadata group's log position the version was read at, from 1; 0 in a request |

| Section | Contents |
|---|---|
| 0 | the version's bytes; empty in a request |

A body with `position` 0 is a request.

Order: unused bytes 32–127 zero; in a request, `epoch` 0 (`FieldValue` `"catalog"`, the epoch); in a reply, `version` at
least 1 (`FieldValue` `"version"`) and `epoch` at most 2⁴² − 1 (`FieldValue` `"epoch"`); sections; in a request, section
0 empty (`CountMismatch` `"catalog"`, its bytes, 0). A version the tenant never held is refused with code 7 (Refused), `detail` the version.

From stage 5.8 a request with `version` 0 may name an `epoch`, which the order above otherwise refuses: it asks for the
version a client may read at that closed epoch, the latest in effect at it, leaving out a version being checked and one
withdrawn (section 11.13, *A tightening's check*). The role holding the client endpoint asks so for a Describe. With
`epoch` 0 a request of `version` 0 still asks for the tenant's current version, pending or not. An `epoch` above the
latest close is refused with Error 1 (Protocol), the message naming both.

### 13.8 Challenge (519)

The first half of verifying a principal's SCRAM-SHA-256 proof: a frontend authenticating a client asks the metadata
group's host for the principal's salt and iteration count, which its server-first-message carries. A name no principal
holds is answered with a salt and a count derived from a secret the metadata group draws at the cluster's first start and
holds durably, so one name takes one salt, as a held one is (`../../openspec/notes/SECURITY.md`, *Failures*).
No endpoint serves it in this stage: the one frontend is both the endpoint and the metadata group's host, and asks
itself without a frame; stage 8's frontends send the command over a role connection.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 4 | `iterations` | u32 | 0 in a request; 4,096 to 10,000,000 in a reply (`FieldValue` `"iterations"`) |

| Section | Contents |
|---|---|
| 0 | the principal's name, UTF-8, prepared with SASLprep |
| 1 | the salt; empty in a request, not empty in a reply |

Order: unused bytes 4–127 zero; `iterations`; sections; section 0 UTF-8; section 1 empty in a request and not empty in
a reply (`CountMismatch` `"salt"`: its length against 0 in a request, 0 against 1 in a reply). A request is the body
with `iterations` 0.

### 13.9 Verify (520)

The second half: the frontend sends the principal's name, the SCRAM AuthMessage (RFC 5802 §3) and the client's
ClientProof, and the metadata group's host checks the proof against the principal's `StoredKey`. It answers whether the
proof holds, the principal, and, when it holds, the ServerSignature the frontend sends in its server-final-message, so
the stored key and the server key stay in the metadata group. An unknown name and a wrong proof are answered alike,
`verified` 0, and the frontend refuses either with code 19. Like Challenge, no endpoint serves it in this stage.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 1 | `verified` | u8 | boolean; 0 in a request (`FieldValue` `"verified"`) |
| 8 | 8 | `principal` | u64 | the principal in a verified reply; 0 otherwise (`FieldValue` `"principal"`) |

| Section | Contents |
|---|---|
| 0 | the principal's name, UTF-8, prepared with SASLprep |
| 1 | the AuthMessage, UTF-8 |
| 2 | the ClientProof, 32 bytes |
| 3 | the ServerSignature, 32 bytes in a verified reply; empty otherwise |

Order: unused bytes 1–7 and 16–127 zero; `verified`; `principal`; sections; sections 0 and 1 UTF-8; section 2 of 32
bytes (`CountMismatch` `"proof"`); section 3 of 32 bytes in a verified reply, else empty (`CountMismatch`
`"signature"`). A request and the reply of a proof that does not hold are the same body, `verified` 0.

### 13.10 Bind (521)

The frontend binds a read-write transaction, or a Drop, to a partition (section 11.12): a new stream of its connection
to the storage process hosting the partition becomes the transaction's there, as a Begin's stream does on a client
connection (section 11.2). The stream stays open. For a transaction, the frontend then sends the client's Write,
Operation, Commit and Abort frames on it, and a Cancel on it aborts the transaction; for a Drop, it sends the client's
Drop, whose reply ends the stream. A partition answers each Write and Operation on the stream with Held (section 13.20) once
it holds the request. The partition answers the Bind with every field as sent and, for a transaction,
`read_version` its visibility watermark when it binds, the version the transaction reads at: every commit the
partition acknowledged before it is visible to the transaction, and none after.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 4 | `partition` | u32 | at least 1 (`FieldValue` `"partition"`) |
| 4 | 1 | `kind` | u8 | 1 a read-write transaction, 2 a Drop (`FieldValue` `"kind"`) |
| 8 | 8 | `tenant` | u64 | the transaction's tenant |
| 16 | 8 | `read_version` | u64 | 0 as sent (`FieldValue` `"read_version"`); as answered, the partition's visibility watermark for kind 1, 0 for kind 2 |
| 24 | 8 | `epoch` | epoch (section 1) | the client's token's epoch, 0 for none: the partition commits the transaction at that epoch or later, holding its commit until the close of `epoch − 1` has been delivered to it |
| 32 | 8 | `principal` | u64 | the client's principal |
| 40 | 16 | `transaction` | bytes | the transaction's identity, 128 random bits the frontend draws, not all zero (`FieldValue` `"transaction"`); Scans of the transaction name it |

No sections. Order: unused bytes 5–7 and 56–127 zero; `partition`; `kind`; `read_version`; `epoch`; `transaction`;
sections. A Bind naming a partition the role address does not serve is refused with Error 1 (Protocol), the message naming
the partition, one naming a tenant the partition's placement map lists no run for with Error 1, the message naming the
tenant, and one naming a transaction the partition holds with Error 1, the message naming the transaction.

### 13.11 Recall (522)

The frontend asks a partition for a session's stored reply, for a retried Commit it cannot answer itself (section 11.5). The partition answers on the Recall's stream with the stored reply, a Commit or Drop frame whose
command header and body are the stored reply's byte for byte, when its table holds `request` as the session's last;
code 14 (SessionBusy) when `request` is in its commit step, not yet decided; code 13 (RequestOutOfOrder), `detail` its
last, when its table holds a last above `request`; and code 18 (NotCommitted) when it holds neither, its table holding
no entry of the session or a last below `request`.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 4 | `partition` | u32 | at least 1 (`FieldValue` `"partition"`) |
| 8 | 16 | `session` | session (section 1) | not all zero (`FieldValue` `"session"`) |
| 24 | 8 | `request` | u64 | at least 1 (`FieldValue` `"request"`) |

No sections. Order: unused bytes 4–7 and 32–127 zero; `partition`; `session`; `request`; sections. A Recall naming a partition the
role address does not serve is refused with Error 1 (Protocol), the message naming the partition.

### 13.12 ComputeRead (523)

The frontend sends a client's read to compute (section 11.12): a Read, or an Execute with its statement's parameters
bound into the pattern, with what compute needs of the read's transaction. Compute answers with Result frames in the
client's layout (section 9.1), each batch's `epoch` and `partition` filled, the last flagged `LAST`, which the frontend
relays; or with an Error: code 23 for a read of a read-write transaction reaching another partition than the
transaction's; code 25 for a read at level 1 reaching another partition than `partition`, or more than one; code 24
when a partition has not closed the read's epoch by the deadline, or a Scan or Frontier of it stalled before giving a row
(sections 13.13 and 13.21), `detail` that partition; code 3 when the read's version is below a partition's latest logged
fold horizon (section 11.9); code 27 for a named read
version above a partition's watermark; Error 1 (Protocol) for a resume key it refuses (section 11.8).

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 4 | `partition` | u32 | the transaction's partition: a read-write transaction's, or a read-only transaction's at level 1 once its first read bound it; 0 for none |
| 4 | 1 | `mode` | u8 | 0 read-write, 1 read-only (`FieldValue` `"mode"`) |
| 5 | 1 | `level` | u8 | the transaction's level, 0 to 2 (`FieldValue` `"level"`) |
| 6 | 1 | `named` | u8 | boolean: `read_version` is a version the client named (section 6.2) (`FieldValue` `"named"`) |
| 8 | 8 | `tenant` | u64 | the transaction's tenant |
| 16 | 8 | `read_version` | u64 | the version the read reads at: the Bind's answer for a read-write transaction; the client's named version; at level 0, the snapshot's version bound; else 0 |
| 24 | 8 | `epoch` | epoch (section 1) | at level 0, the snapshot's epoch, which every partition read closes before it answers; at level 1, the token's epoch, likewise; at level 2, the read's floor; 0 for a read-write transaction, a named version or none |
| 32 | 8 | `principal` | u64 | the client's principal |
| 40 | 16 | `transaction` | bytes | a read-write transaction's identity, as its Bind named it; all zero for a read-only transaction |
| 56 | 4 | `limit` | u32 | as Read's |
| 60 | 4 | `deadline_ms` | u32 | the milliseconds left until the transaction's deadline when the frontend sends the read |
| 64 | 4 | `max_frame` | u32 | the `max_frame` of the client's Hello, 524,288 to 1,048,576 (`FieldValue` `"max_frame"`): every Result frame of the read is at most this |
| 68 | 4 | `credit` | u32 | the read's stream's starting credit, the `initial_credit` of the frontend's Welcome to the client, at least 1 (`FieldValue` `"credit"`) |

| Section | Contents |
|---|---|
| 0 | the pattern (section 4.5), no parameter slot |
| 1 | the term table's records (section 4.4) |
| 2 | the term table's text |
| 3 | the client's resume key (section 11.8) |

Order: unused bytes 7 and 72–127 zero; `mode`; `level`; `named` 0 or 1, and 1 only with `mode` 1 and `level` 0
(`FieldValue` `"named"`, the byte); `read_version` 0, a version or a version bound (section 1) (`FieldValue`
`"read_version"`); `epoch`; `partition` and `transaction` both not zero for `mode` 0, and
`transaction` zero for `mode` 1 (`FieldValue` `"transaction"`, the mode); `max_frame`; `credit`; sections; the term table;
the pattern.

Compute reads by Scans (section 13.13): a read-write transaction's at `at` 1, `read_version` the Bind's version and
`epoch` 0, naming the transaction; at level 0, `at` 1 with the snapshot's version bound and epoch; at level 1, `at` 2
with the token's epoch; at level 2, `at` 1 with the version bound and epoch it answers at, after Closed (section
13.17) when the read reaches several partitions, or `at` 3 with the floor when it reaches one, as a reverse read's
referrer set is read at the home; with a named version, `at` 4. A read reads one version of a partition: its first Scan
there chooses it by the `at` above, and each later Scan of that partition sets `at` 1, `read_version` that Scan's Entries'
`version` and `epoch` 0.

Compute reads by Frontiers (section 13.21) in the lane and at the version the read takes (T48, C40). Before it sends
any lane S Frontier to a partition, compute fixes the partition's version with a Scan of it (`at` 1, 2 or 3, waiting
for the epoch as Scan does), of the ranges the read needs there first or of one record of space 1 when it needs none,
and every Frontier of the read to that partition names that Scan's Entries `version`; a version the client named is
fixed already, as a Scan's `at` 4 reads it without waiting. A lane S Frontier therefore never waits for an epoch (T48).

| The read | Frontier `lane` | `read_version` |
|---|---|---|
| A read-write transaction's | 3, X | the Bind's version |
| Read-only, level 0 | 2, S | the `version` of the Entries of its first Scan of the partition, the snapshot's version bound; or the version the client named |
| Read-only, level 1 | 2, S | the `version` of the Entries of its first Scan of the partition, the version that Scan chose |
| Read-only, level 2 | 2, S | the `version` of the Entries of its first Scan of the partition, the version bound it chose |
| A read the classification makes monotonic-fast (`../schema/SCHEMA.md` section 9.6), at level 2 | 1, L | 0, the partition's latest applied state |

A batch stops at `max_frame`. A row whose terms do not fit an otherwise empty batch is refused with code 6 (TooLarge), `detail`
`max_frame`. A resume key that does not fit beside its rows goes alone in a last batch of `rows` 0, which spends credit as any
batch does. A key too large for that batch, or for a Read within the Welcome's `max_frame`, refuses the read with code 6. An
Execute whose ComputeRead, its parameters bound into the pattern, would not fit a frame within compute's Welcome is refused by
the frontend with code 6, `detail` that `max_frame`, and no compute process sees it.

An object record gives rows as section 13.14 states. A system ID or literal hash a row holds that its partition's dictionary
answers no entry for is corruption: compute answers the read with code 17 (Unavailable), the message naming the row holding
it, and the connection stays open.

Each pattern takes its kernel by the first row that fits, at the partitions section 11.12 asks (T48):

| The pattern | The kernel |
|---|---|
| A bound subject that is a reifier's skolem IRI, or the handle IRI of an edge-encoded instance | Scan of space 1, kind 18, the statement's record at the partition its bucket names |
| Any other bound subject | Frontier at the subject's partition: kinds 1 and 2, forward edges and values; 6, an attribute's column; 5, a version run |
| A bound IRI or blank-node object, the subject unbound | the object's referrer set, a Scan of space 1, kind 7, at the object's home; then Frontier kind 3 at the set's members |
| A bound vector object, the subject unbound | Scan of space 2 of the predicate's forward families at every partition, compute keeping the objects equal to the vector's encoding, since no reverse key names a vector |
| Any other bound literal object, the subject unbound | Frontier kind 3 at every partition |
| Neither subject nor object bound | Scan of space 2 |
| An unbound predicate, with any row above | that row's Frontier with `predicate` 16 bytes of `0xFF`, or its Scan naming every predicate |
| An unbound graph, with any row above | that row's Frontier with `graph` 16 bytes of `0xFF`, or its Scan of space 2 naming every graph |

An unbound graph with a bound subject is therefore read by a Frontier naming every graph (section 13.21), not by a
Scan of a whole family. A Frontier or a range of space 2 naming every graph answers rows naming their graphs (sections
13.13 and 13.22), and in a transaction records the tenant's whole range of conflict keys, so a graph appearing later
conflicts.

### 13.13 Scan (524)

One of storage's two read kernels, beside Frontier (section 13.21): records of the partition at a role address by their
keys, and families of it read whole, at one version, batched per address. A Scan reads in lane S, or in lane X when it
names a transaction (`../storage/STORAGE.md` section 18.1); lane L, and a read of named rows, are a Frontier's. Compute
reads the dictionary, a referrer set, a claim and a statement's record by Scans, each one record by its key, and reads a
predicate's family whole where a pattern binds none of its rows: an unbound subject, an unbound graph. Every key a Scan
names is logical, the key of a record as `../storage/STORAGE.md` section 9.3 gives it, made of system IDs, hashes, term
encodings and sortable values; no range names a slot, and neither does a resume key, whose logical form `../storage/STORAGE.md` section 18.3 gives
(sections 14.6 and 18.3 there, T44, T46).

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 1 | `at` | u8 | 1 at `read_version`, once the partition has closed `epoch`; 2 at the partition's latest applied version, once it has closed `epoch`; 3 at the version bound of the partition's latest closed epoch, once that is at least `epoch`; 4 at `read_version`, a version the client named, without waiting (`FieldValue` `"at"`) |
| 8 | 8 | `read_version` | u64 | with `at` 1 and 4, the version; 0 otherwise (`FieldValue` `"read_version"`) |
| 16 | 8 | `epoch` | epoch (section 1) | the epoch the partition closes before it answers; 0 for none, and for `at` 4 |
| 24 | 8 | `principal` | u64 | the client's principal |
| 32 | 16 | `transaction` | bytes | a read-write transaction's identity, the partition's own, whose reads record the ranges; all zero for none |
| 48 | 4 | `limit` | u32 | the most rows per range of space 2; 0 for no limit |
| 52 | 4 | `deadline_ms` | u32 | how long the partition waits for the close of `epoch`: the time left until the deadline of the read's transaction (section 6.2); 0 when `epoch` is 0 (`FieldValue` `"deadline_ms"`) |
| 56 | 8 | `tenant` | u64 | the tenant every range reads |

| Section | Contents |
|---|---|
| 0 | ranges, 64 bytes each |
| 1 | the ranges' bytes, back to back: a range of space 1's key, a range of space 2's resume key when it has one |
| 2 | with `transaction` not zero, the IRIs the read names: a request term table's records (section 4.4), interned in the order of the ranges naming them; empty otherwise |
| 3 | that term table's text |

| Range offset | Width | Field | Type |
|---|---|---|---|
| 0 | 4 | `partition` | u32: the partition read, the one the role address serves |
| 4 | 1 | `space` | u8: 1 a record by its key; 2 a family read whole |
| 5 | 1 | `kind` | u8: for space 1, the record kind (`../storage/STORAGE.md` section 9.3): 1 term to ID, 2 ID to term, 3 hash to text, 4 a key claim, 5 a `@unique` claim, 7 a referrer set, 8 a tombstone, 9 statistics, 12 a `@set` combination, 13 an idempotency claim, 18 a statement's record, 19 a `@limit` aggregate; for space 2, as a Frontier's `kind` (section 13.21): 1 forward edges, 2 forward values, 3 reverse entries, 4 an index relation, 5 a `@bitemporal` attribute's or role's version runs (T48, C39), 6 an attribute's column chunks |
| 6 | 1 | `tokenizer` | u8: space 2 with kind 4, the index's tokenizer identifier byte, not `0xFF`; 0 otherwise |
| 7 | 1 | zero | 0 |
| 8 | 4 | `bytes` | u32: the offset in section 1 of the range's bytes, where the range before's end, 0 for the first |
| 12 | 4 | `key_len` | u32: space 1, its key's length, at least 1; 0 for space 2 |
| 16 | 4 | `resume_len` | u32: space 2, the length of its resume key, the one an Entries frame of an earlier Scan of the same range at the same version gave, unchanged; 0 to start, and for space 1 |
| 20 | 12 | zero | 0 |
| 32 | 16 | `graph` | system ID (section 1): space 1 with kinds 4, 5 and 9, the graph whose record it is (`../storage/STORAGE.md` section 9.2; T48, C15), for kind 4 the graph its key holds, the key's bytes 2–17 (T48); space 2, the graph read, 16 zero bytes for the default graph, or 16 bytes of `0xFF` for every graph the partition holds the family in; 16 zero bytes otherwise |
| 48 | 16 | `predicate` | system ID (section 1): space 2, the predicate, an attribute's for kind 6, or 16 bytes of `0xFF` for every predicate of the kind the partition holds (T48, C38); 16 zero bytes for space 1 |

**A range of space 1** answers the record of its kind and key in the tenant at the version, in whichever of the
partition's bucket groups holds it (`../storage/STORAGE.md` section 9.2): one entry, its key the range's and its value
the record's, or no entry when the version holds none. A run of such ranges is answered in one frame (section 13.14),
so resolving a batch of terms costs one frame and not one per term.

**A range of space 2** reads one predicate's family of its kind, or every predicate's, the tokenizer's for an index
relation and every type's declaring the attribute for kind 6, in its graph or in every graph, in every bucket group the
partition holds and in both storage classes: the families in the order of their family keys (`../storage/STORAGE.md`
section 13.1), and each in its own order, its rows by their keys and a row's entries as section 7.4 there orders them, a
forward row and a column chunk's by slot, so no order holds across bucket groups or partitions (T38). It answers each
row holding an entry at the version as one entry of the answer, at most `limit` rows. A row of kind 5 is an instance's
version run, its entries the revisions, each its asserted start and its value, from which compute selects the revision
in effect at the time it reads as of (T48, C39). A range that stops, at its `limit` or because
its Scan ended before it did, continues in a new Scan at the same version naming the resume key the range's last
Entries frame gave (section 13.14), and answers strictly after it; a resume key that does not decode as `../storage/STORAGE.md` section 18.3 gives it,
or names a family the range does not read, is refused with Error 1 (Protocol), the message naming the range.

A Scan chooses its version once, when any wait for `epoch` has ended, and every range of it reads that version. With
`at` 1 and 4, a `read_version` at or below the version bound of an epoch the partition has closed, or of epoch 0, reads
what the partition applied at or below that version, since every round of such an epoch is applied, so an idle
partition answers a version it never reached. Any other `read_version` above the partition's visibility watermark is
refused with code 27 (AheadOfWatermark), `detail` the watermark: with `at` 4 at once, with `at` 1 once the partition has
closed `epoch`. One below the partition's latest logged fold horizon (`../storage/STORAGE.md` section 14.2) is refused
with code 3 (TooOld).

The partition counts `deadline_ms` from the Scan's arrival. A Scan whose `epoch` the partition has not closed by then is
refused on its stream with code 24 (TimedOut), `detail` the partition, having sent no frame; with `deadline_ms` 0 it is
refused so at once. The version a Scan reads holds the partition's fold horizon until the Scan ends on the partition's
primary alone (section 11.9): on a follower it holds nothing, and a Scan there whose version a logged horizon passes is
refused with code 3 (TooOld) for the rest of it (T48, C34). Once any wait is over, the setting
`scan_stall_ms` (section 11.9) bounds how long a Scan stays open with no frame able to leave, because its stream holds no
credit or its connection takes no bytes: past it the partition refuses the Scan with code 24, `detail` the partition, the
message saying it stalled, and releases what it held, so a reader that stopped holds a version for no longer than that.
Each frame that leaves restarts it. A Scan naming a transaction is bounded by it as well as by the transaction's limit.

A range of space 1 of the session table, the marks, the slot maps, the drop markers, the builds, the manifest or the
purge markers, which are the replica's own (`../storage/STORAGE.md` section 9.3, kinds 6, 10, 11, 14 to 17 and 20), is
refused by its `kind`. A
store that fails under a range refuses the Scan with code 17 (Unavailable), the message naming the range, so a failed
read never reads as absence.

**A Scan naming a transaction** is a read of that transaction, and what it answers and records is what makes the
transaction's commit serializable:

- It sets `at` 1, `read_version` the Bind's answer (section 13.10), `epoch` 0, and `principal` and `tenant` the
  transaction's. A Scan whose version, principal or tenant is not the transaction's is refused with Error 1 (Protocol),
  the message naming the field.
- It reads the transaction's partition alone, at that version with the transaction's own uncommitted writes over it, so
  a read sees what the transaction wrote in an earlier request: a row holds what the transaction added and lacks what it
  removed, a row it emptied is absent, a record holds what it wrote and a record it removed is absent, and a term it
  created is found. Those writes are the ones the partition acknowledged before the Scan arrived; the frontend orders
  them (section 11.12, *Reads*).
- It reads ranges of space 1 of kinds 1 to 5, 12, 13 and 18, and ranges of space 2. A range of space 1 of another kind
  is refused with Error 1 (Protocol): a referrer set, a tombstone, statistics and a `@limit` aggregate are what the
  partition reads itself when a write needs them, and a read-write transaction reads no other partition.
- Before a frame leaves, the partition records in the transaction's reads what the frame answers, so no entry reaches
  compute unrecorded. No read records an entry or record it finds of storage class I (`../storage/STORAGE.md` section
  14.3), which only a purge changes, and a purge at version `v` aborts every open read-write transaction of its
  partition whose read version is below `v`; every read records what it finds absent (T48, C43). A range of space 1 of a
  claim, kinds 4, 5, 12 and 13, records its claim's conflict key (section 12.2, conflict family 7) as a range holding
  that key alone, absent or found, a found claim of class I aside, so a later claim of it conflicts (conflict kind Range,
  section 9.4); a range of kind 18 that finds a record of class M records the statement's conflict key (conflict family
  5) as a statement tested (conflict kind Statement), and one finding none records the statement's ID as an absent term
  (conflict kind AbsentTerm), so a later commit of the statement conflicts; a range of kinds 1 to 3 records nothing but
  what the term rule below records. A range of space 2 records, when the partition takes it and whether or not a frame
  follows, its family's whole range of conflict keys: the tenant, the graph and the predicate, then the conflict family's
  byte, 1 forward rows for kinds 1, 2, 5 and 6, 2 reverse rows for kind 3, 3 index tokens for kind 4; for every
  predicate, the graph's whole range, and for every graph, the tenant's whole range, since a predicate or a graph may yet
  appear (conflict kind Range; T48, C38). So an insert anywhere the range could answer, a new row among them, conflicts
  however the range ends (T38, T44).
- It resolves terms by their text. The partition gets or creates an IRI's ID from its text, which the term table
  carries, a dictionary key holding the IRI's hash and then its name bytes (`../storage/STORAGE.md` section 9.3, kind
  1), and a term a read finds absent is recorded by its ID. So the Scan's term table names each IRI the read needs an ID for, and its first
  ranges name, in the table's order, one range each of space 1 and kind 1 holding that IRI's dictionary key; a key that
  is not the text's is refused with Error 1 (Protocol). The partition resolves each as a read in the partition does: got
  or created, the same ID any concurrent writer of the IRI takes. One it created is recorded as an absent term (conflict
  kind AbsentTerm), so a later commit of a statement of that term conflicts. The range answers the entry with its ID. An
  unheld skolem IRI, which no read can hold, answers no entry and records nothing. An IRI another partition owns is read
  by the partition's copy of its entries, as the partition's own reads do: with no copy, the partition first takes the
  ID from the IRI's home by Reference, naming no group (section 11.12, *Registration*), and answers the range once the
  home has replied, so the Scan waits on that reply within the transaction's limit. A Scan arriving while another of the
  transaction's waits so waits with it, and asks for its own terms once that reply is in; one arriving while a Write or
  Operation of the transaction waits so is refused with Error 1 (Protocol), since the frontend sends a read only once the
  writes before it are answered. A Write, Operation or Commit arriving while a cancelled read's Scan waits so takes its
  turn once that reply is in. A home's outcome 7 for a skolem IRI says it holds none, so the range answers no entry, as
  an unheld skolem IRI of the partition's own does, the transaction goes on and its reads ask for that IRI no more,
  wherever the IRI is homed. Any other refusal from the home ends the transaction, a failed store's outcome 17 among
  them, which no read takes for absence (section 13.15). Compute names such an IRI as a predicate, a graph or an object;
  a bound subject another partition owns never reaches the table, since compute answers it with code 23.
- What the read set has no room for ends the transaction with code 6 (TooLarge), `detail` the limit in bytes, and no
  entry of that range is sent. A Scan that ends its transaction, for this or any reason, answers so on its stream and
  sends the same Error on the Bind stream, which ends it.

Commit validates what these record as it does a read the partition made itself, by the table of *What a transaction's
reads record* in the transactions specification: a write above the transaction's read version whose conflict key lies
inside a recorded range, or names a recorded statement, aborts the commit. Every conflict key a commit writes counts
(section 12.2).

Order: unused bytes 1–7 and 64–127 zero; `at`, and 1 when `transaction` is not zero (`FieldValue` `"at"`);
`read_version` 0 for `at` 2 and 3, and a version or version bound (section 1) for `at` 1 and 4 (`FieldValue`
`"read_version"`); `epoch` an epoch, and 0 for `at` 4 and when `transaction` is not zero (`FieldValue` `"epoch"`);
`deadline_ms` 0 when `epoch` is 0 (`FieldValue` `"deadline_ms"`); sections; section 0 a whole number of 64-byte ranges
(`SectionLength`) and not empty (`CountMismatch` `"ranges"`, 0 against 1); sections 2 and 3 empty when `transaction` is
zero (`CountMismatch` `"terms"`, their bytes against 0), otherwise a term table (section 4.4), then at most as many terms
as ranges (`CountMismatch` `"terms"`, the terms against the ranges), then every entry of kind 1 (`FieldValue`
`"scan term"`, the entry's index); each range in order: bytes 7 and 20–31 zero (`PaddingNotZero`, the range's index);
`partition` at least 1, the same in every range when `transaction` is not zero (`FieldValue` `"partition"`, the
partition); `space` 1 or 2, and `kind` one its space lists (`FieldValue` `"kind"`, the range's index); `tokenizer`
nonzero and not `0xFF` exactly for space 2 with kind 4 (`FieldValue` `"tokenizer"`, the range's index); for space 1,
`key_len` at least 1, 16 for kinds 2, 3, 7, 8, 9 and 18, whose keys are a system ID or a hash, and `resume_len` 0; for
space 2, `key_len` 0 (`FieldValue` `"range"`, the range's index); `bytes` where the range before's end, 0 for the
first (`FieldValue` `"range"`); its bytes within section 1 (`IndexOutOfRange`, section 0, the range's index, their end,
section 1's length); `graph` 16 zero bytes for space 1 but with kinds 4, 5 and 9, and for space 2 a valid system ID, 16
zero bytes or 16 bytes of `0xFF` (`FieldValue` `"graph"`, the range's index); for space 1 with kind 4, `graph` equal to
the key's bytes 2–17, the graph the key holds, a key shorter than 18 bytes holding none (`FieldValue` `"graph"`, the
range's index); `predicate` a valid system ID or 16 bytes
of `0xFF` for space 2 and 16 zero bytes for space 1 (`FieldValue` `"predicate"`, the range's index); section 1 ending with the last range's
bytes (`CountMismatch` `"range keys"`, section 1's length against the last range's end). A range of a partition the
role address does not serve is refused with Error 1 (Protocol), the message naming it; a `transaction` the partition
does not hold with Error 1, `detail` 1, the message naming it, which compute relays with that `detail` (section 11.12,
*Reads*). The partition refuses all of these before it sends a frame.

### 13.14 Entries (525)

A batch of one range's entries, or of a run of ranges of space 1's, so that resolving a batch of terms costs one frame
and not one per term. A batch of one range holds its entries in the range's order (section 13.13), and a run holds them
in its ranges' order, at most one per range: each with its value visible at the version the range was read at; a record
removed at that version, and a row holding no entry at it, is absent. A Scan's answer gives its ranges in order, each in
one or more Entries frames, the last of which says how the range ended; the stream's last frame, the last range's last,
carries `LAST`. Entries frames spend credit as Result frames do (section 11.3). Every Entries frame of a Scan names the
one version the Scan chose, as its lane S or X has it; frames each naming the version of their own turn are a lane L
Frontier's (section 13.22).

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 4 | `entries` | u32 | the batch's entry count |
| 4 | 4 | `range` | u32 | the range's index in the Scan, from 0 |
| 8 | 1 | `end` | u8 | 0 more of the range follows; 1 the range ended; 2 the range of space 2 stopped at the Scan's `limit` with a row of it remaining, so a range holding exactly `limit` rows ends with 1, unless 256 rows holding nothing at the version follow them, where the partition stops looking (`FieldValue` `"end"`) |
| 9 | 1 | `continues` | u8 | 1 the batch's last entry's value continues in the next frame, which needs a last entry; 0 otherwise (`FieldValue` `"continues"`) |
| 12 | 4 | `through` | u32 | the batch's last range: `range`, or above it when the batch answers each range from `range` through this one, each a range of space 1, each with `end` 1 and `continues` 0, a range finding nothing holding no entry (`FieldValue` `"through"`) |
| 16 | 8 | `epoch` | epoch (section 1) | the epoch the range was read at: with `at` 1 and 4, `read_version`'s; with `at` 3, the latest closed epoch the partition chose; with `at` 2, the higher of that and the epoch of the version it chose; 0 when the partition has closed no epoch and read at the version bound of none (`FieldValue` `"epoch"`) |
| 24 | 8 | `version` | u64 | the version the Scan chose, a version or version bound (section 1), the same in every frame of the Scan (`FieldValue` `"version"`) |
| 32 | 8 | `widths` | bytes | for a range of space 2, byte `c` the width of the family's inline column `c`, 1 to 18, and 0 past its last column; all zero for a family with no inline column, for a range naming every predicate and for a range of space 1 (T48, C37) |

| Section | Contents |
|---|---|
| 0 | the entries, in the key-value layout of section 12.2, every value present |
| 1 | for a range of space 2 whose frame has `end` 0 or 2, the resume key strictly past what the frame answered, its last object record included (`../storage/STORAGE.md` section 18.3); empty otherwise |

An entry of a range of space 1 is its record: its key the record's key, its value the record's value as
`../storage/STORAGE.md` section 9.3 gives it. An entry of a range of space 2 is a row: its key the graph's system ID,
then, for a range naming every predicate, the row's predicate's system ID (T48, C38), then the row's logical key, a
forward row's, a version run's and a column chunk's the subject's system ID, a reverse row's in a generic-layout family
the object's term encoding, an IRI or a blank node as tag `0x01` or `0x02` followed by its system ID and a literal as its
term encoding, and in a compiled layout's the participant's system ID (T48), an index row's its token
(`../storage/STORAGE.md` sections 5.1, 5.3 and 5.4); its value the row's
entries visible at the version, in the row's order, each an object record:

| Object record offset | Width | Field | Type |
|---|---|---|---|
| 0 | 4 | `encoding_len` | u32: the neighbour's length |
| 4 | 4 | `statements` | u32: its statement IDs' count |
| 8 | `encoding_len` | `encoding` | the neighbour, as a Neighbours entry names it (section 13.22): a system ID for kinds 1, 3 and 4, the subject's for kinds 3 and 4; kind 2's a term encoding in the generic layout or a sortable value in a compiled one; kind 5's the revision's asserted start as a big-endian u64, then its value's sortable encoding; kind 6's the attribute's value, its sortable encoding |
| | 16 × `statements` | statement IDs | the IDs the neighbour is held under, in ascending order; none for kind 6; compute drops them from what it returns |
| | 1 | `valid` | u8, for a family with inline columns only: bit `c` set when inline column `c` holds a value (T48, C37) |
| | the sum of `widths` | inline values | for a family with inline columns only: each inline column's value in its width, in column order, zero where absent, so compute has the value that orders the family (`../storage/STORAGE.md` section 7.4) |

A neighbour is held either once, with `statements` 0, which a Result gives as one result row with `ABSENT` in the reifier
column, or under one statement ID per statement, which is one result row each with that statement's reifier. A parallel edge gives the statement
already held an ID of its own in the same write.

A row or record larger than a frame continues across frames, as a hub's reverse row does. A frame with `continues` 1
ends between two object records, or inside a record's value, and has `end` 0; the next frame's first entry has the same
key, and its value is the next object records or the value's next bytes. The value is the bytes of every such frame in
order. The partition keeps the place it reached at the same version, so a row is read once however many frames it
takes; a frame stops at the receiver's `max_frame`, or once it holds an object record or value at what its turn may yet
build, so a large row takes many frames. A turn of the partition builds at most one frame per Scan, the Scans taking
turns round-robin, within a budget of bytes per turn the partition chooses so that a commit waiting on the turn waits
less than one sync of its log, and a Scan the budget leaves waits for the next turn. A turn also reads at most 256 rows
holding nothing at the version, so a family of emptied rows takes many turns, and a turn that found no entry sends no
frame. The partition builds a stream's next frame once the one before has left, so a reader's stalled stream holds at
most one frame in memory and the row that frame stopped inside. A frame always takes at least one object record, or one
record's value or its first `max_frame` share.

An object record running past the value is refused with `EntryOutOfBounds` (section 0, the record's index, its offset in
the value), and one whose `valid` has a bit set at or past the family's inline columns with `FieldValue` `"entry"`, the
record's index. A record is at most 16 + `value_bytes_max` + 16 × `statements_per_object_max` + 145 bytes, a version
run's neighbour holding 8 bytes of asserted start before its value (section 11.9).

Order: unused bytes 10–11 and 40–127 zero; `end`; `continues` 0 or 1, 0 when `end` is not 0; `through` not below
`range`, and when above it `end` 1 (`FieldValue` `"through"`); `epoch` at most 2⁴² − 1; `version` a version or version
bound (section 1) (`FieldValue` `"version"`); `widths` each 0 to 18, none nonzero after a 0, all zero when `through` is
above `range` (`FieldValue` `"widths"`); sections; each entry inside the section (`EntryOutOfBounds`) with zero
padding (`PaddingNotZero`); every value present (`FieldValue` `"entry"`, the entry's index); the count equals `entries`
(`CountMismatch` `"entries"`); at least one entry when `continues` is 1 (`FieldValue` `"continues"`, 1); section 1 not
empty exactly when the range is of space 2 and `end` is 0 or 2 (`CountMismatch` `"resume key"`).

The receiver, the side that dialled, compares each frame with those before it, and refuses in this order, closing the
connection as section 2.6 has a client do, since a partition answering so cannot be trusted with the connection's other
streams:

1. In a frame naming the previous frame's range after a frame with `continues` 1: no entry or a first key not equal to
   that frame's last key (`FieldValue` `"entry order"`, 0). So a value promised to continue cannot stop.
2. A `range` that is not the one expected: 0 in the first frame; the previous frame's after `end` 0; one above the
   previous frame's `through` after `end` not 0; and a `through` below its Scan's range count, every range of a run a
   range of space 1 (`FieldValue` `"range"`, the frame's range).
3. `LAST` on any frame but the one whose `through` is the Scan's last range with `end` not 0, or its absence from that
   frame (`FieldValue` `"end"`, the frame's range). So an answer ending early cannot read as whole.
4. A `version` that is not the Scan's first frame's (`FieldValue` `"version"`, the frame's range), or `widths` that are
   not the first frame's of the same range (`FieldValue` `"widths"`, the frame's range).
5. An entry that does not belong where the frame puts it: in a run, one whose key is not the key of a range from `range`
   through `through`, after the range of the entry before; in a batch of a range of space 1, a key other than the
   range's; in a batch of a range of space 2, a key not beginning with the range's graph, when it names one
   (`FieldValue` `"entry order"`, the entry's index).

### 13.15 Reference (526)

A partition registers its references at an entity's home, or gets or creates a predicate's or graph's ID there, in one
batch per home's role address within each write request (section 11.12, *Registration*); the frontend sends it to
learn a unique predicate's system ID. The home, for each registration, gets or creates the entity's ID, adds or renews
the bucket group in the entity's referrer set when the registration names one, makes both durable in a round of its
partition's log, and only then answers with every field as sent and each registration's reply fields filled. A repeat
returns the same ID and changes nothing; concurrent registrants receive one ID.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 1 | `reply` | u8 | boolean: 0 the request, 1 the home's reply (`FieldValue` `"reply"`). A reply's fields can be 0, a close of none among them, so the flag tells the two apart |
| 1 | 1 | `mode` | u8 | 0 get or create; 1 get-only, for participant checks (below) (`FieldValue` `"mode"`) |
| 8 | 8 | `tenant` | u64 | the tenant of the batch's entities, whose dictionary the home resolves each IRI in |

| Section | Contents |
|---|---|
| 0 | registrations, 48 bytes each |
| 1 | the term table's records (section 4.4): the IRIs registrations name by their text, kind 1 |
| 2 | the term table's text |

| Registration offset | Width | Field | Type |
|---|---|---|---|
| 0 | 4 | `partition` | u32: the entity's home, the partition owning its bucket |
| 4 | 4 | `group` | u32: the referring subject's bucket group (section 1); `ABSENT` for a predicate or graph, or for the frontend, which join no referrer set |
| 8 | 4 | `iri` | u32: the entity's IRI, or a blank node's skolem IRI, as an index into the term table; `ABSENT` when `entity` names it |
| 12 | 2 | `outcome` | u16: 0 in a request; in a reply 0 accepted, else the error code refusing this registration: 26 (WrongPartition) for a bucket the home does not own, 7 (Refused) for a skolem or handle IRI the home's dictionary does not hold, 17 (Unavailable) for a store that failed at the home under the registration, 30 (EntityDeleted) with `mode` 0 for an entity named by its ID whose tombstone the home holds |
| 14 | 1 | `status` | u8: 0 in a request and with `mode` 0; with `mode` 1, in an accepted reply, 1 absent, 2 live, 3 deleted, 4 retracted |
| 15 | 1 | zero | 0 |
| 16 | 8 | `generation` | u64: in a request, the generation the registrant last saw for the entity and `group`, 0 for none; in an accepted reply, the member's generation; 0 when `group` is `ABSENT` |
| 24 | 8 | `close` | epoch (section 1): 0 in a request; in an accepted reply, the latest close delivered to the home, `c`, below the epoch the home stamped the registration at |
| 32 | 16 | `entity` | system ID (section 1): in a request, the entity's ID, all zero when `iri` names it; in an accepted reply, the entity's ID; in a refused reply, as sent |

A registration naming the member's current generation, or any generation for a group not in the referrer set, adds or
renews the member at one generation above the one named; one naming an older generation changes nothing and is
answered with the current generation. The registrant commits a transaction referencing the entity only once close `c`
has been delivered to it. A write's registration refused with outcome 7 ends its transaction with code 7, the forged IRI
of section 4.4; a Scan's reads outcome 7 as no entry (section 13.13). Outcome 17 ends the registrant's transaction with
code 17 in either case, so a failed store never reads as a skolem IRI nobody holds. Outcome 26 ends the registrant's transaction
with code 26 in either case, `detail` 0, since the entity's owner is one the registrant's map does not know: the client runs the
transaction again on a new Begin (section 9.4). Outcome 30 ends the registrant's transaction with code 30.

**Get-only.** With `mode` 1 the home gets each entity and never creates one, for a participant check (`../schema/SCHEMA.md`
section 12.3), and answers its `status`: 1 absent, an IRI its dictionary does not hold or an ID it holds nothing of
included; 2 live; 3 deleted, its tombstone held (section 12.2); 4 retracted, an
instance of a `@bitemporal` type whose `asserted` has ended (`../schema/SCHEMA.md` section 8.4). For 2, 3 and 4 `entity` holds the ID, whose bytes 0–1 are
the entity's type prefix. A live entity's registration naming a bucket group adds or renews the member as with `mode` 0;
any other changes nothing and is logged in no round, answered at once with `generation` and `close` 0 and, for 1,
`entity` all zero. The registrant decides what a status means for its write: a participant absent, retracted or deleted refuses it
with the schema's refusal (`schema/SCHEMA.md` section 8.7, `UGS1017`, `UGS1019`). A get-only reply's `outcome` is 0, 17 or 26.

Order: unused bytes 2–7 and 16–127 zero; `reply`; `mode` 0 or 1; sections; the term table, every entry of kind 1
(`FieldValue` `"reference term"`, the entry's kind); section 0 not empty and a whole number of 48-byte registrations
(`SectionLength`, `CountMismatch` `"registrations"`); each registration in order: byte 15 zero
(`PaddingNotZero`, the body offset), then its fields, each refused as `FieldValue` `"registration"` naming the
registration's index: `partition` at least 1, `group` below 2¹⁸ or `ABSENT`, `iri` `ABSENT` or below the term table's
length, `entity` all zero or a valid system ID, `close` at most 2⁴² − 1, and `generation` 0 when `group` is `ABSENT`;
in a request, `outcome`, `status` and `close` 0, and `entity` all zero exactly when `iri` is not `ABSENT`; in a reply,
`outcome` 0, 7, 17, 26 or 30, and 0, 17 or 26 with `mode` 1; `status` 0 with `mode` 0 and in a refused reply, and 1 to
4 in an accepted one with `mode` 1; an accepted registration's `entity` a valid system ID, all zero for `status` 1, and,
with a `group` and a `status` of 0 or 2, its `generation` at least 1, `generation` and `close` 0 for a `status` of 1, 3
or 4; a refused one's `generation` and `close` 0 and `entity` all zero exactly when `iri` is not `ABSENT`. A registration
naming a partition the role address does not serve is answered with outcome 26.

### 13.16 CloseRequest (527)

A partition whose open epoch's sequence would run out asks the sequencer to close that epoch now (section 11.12,
*Epochs*). The sequencer closes it once `epoch_ms_min` has passed since the previous close, and the close, arriving on
the partition's Subscribe stream, is the answer; no frame answers on the request's stream, which ends with it, a
refusal included. A request naming an epoch already closed changes nothing. The frontend drops, unanswered, a request it
refuses: one naming an epoch above the open one, one breaking a body's rule, and one arriving while it stops.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 4 | `partition` | u32 | the asking partition, at least 1 (`FieldValue` `"partition"`) |
| 8 | 8 | `epoch` | epoch (section 1) | the partition's open epoch, one above the latest close delivered to it, at least 1 (`FieldValue` `"epoch"`) |

No sections. Order: unused bytes 4–7 and 16–127 zero; `partition`; `epoch`; sections.

From stage 5.8 the role holding the client endpoint sends it too, for a snapshot read waiting on the open epoch, with
`partition` 0, which no other sender may use (section 11.12a): one arriving on a connection whose role is `storage` is
dropped unanswered, as this section's other refusals are.

### 13.17 Closed (528)

Compute asks each partition a monotonic-fast read reaches, at its role address, for its latest closed epoch, so the
read answers at the lowest of them (section 11.11). The partition answers with every field as sent and its latest closed
epoch filled.

The command header is all zero.

| Section | Contents |
|---|---|
| 0 | partitions, 16 bytes each |

| Partition offset | Width | Field | Type |
|---|---|---|---|
| 0 | 4 | `partition` | u32: the partition the role address serves |
| 4 | 4 | zero | 0 |
| 8 | 8 | `epoch` | epoch (section 1): 0 in a request; in a reply, the partition's latest closed epoch, 0 for none |

Order: command header bytes 0–127 zero; sections; section 0 not empty and a whole number of 16 bytes
(`SectionLength`, `CountMismatch` `"partitions"`); each partition in order: bytes 4–7 zero (`PaddingNotZero`),
`partition` above the one before, the first at least 1 (`FieldValue` `"partition"`), `epoch` at most 2⁴² − 1
(`FieldValue` `"epoch"`). A reply whose every epoch is 0 holds the bytes of its request; the partition, which
receives the request, also refuses a request whose `epoch` is not 0 (`FieldValue` `"epoch"`). A partition the role
address does not serve is refused with Error 1 (Protocol), the message naming it.

### 13.18 DefaultEntry (529)

A partition whose commit first wrote a predicate the catalog version in effect does not declare asks the metadata group
to record the predicate's default entry, keyed by the tenant and the predicate (section 11.13). The frontend answers
once the record is durable in the metadata group, with every field as sent and `found` 1 when the record already
existed. Recording one creates no catalog version, stales no prepared statement and aborts no transaction.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 8 | `tenant` | u64 | |
| 8 | 1 | `found` | u8 | boolean; 0 in a request (`FieldValue` `"found"`) |

| Section | Contents |
|---|---|
| 0 | the term table's records (section 4.4): one IRI, the predicate's |
| 1 | the term table's text |

Order: unused bytes 9–127 zero; `found` 0 or 1; sections; the term table; one entry (`CountMismatch`
`"default entry"`), of kind 1 (`FieldValue` `"default entry"`, the entry's kind). A reply of `found` 0 holds the bytes of
its request; the frontend, which receives the request, also refuses a request whose `found` is not 0 (`FieldValue`
`"found"`).

### 13.19 Report (530)

A partition reports to the metadata group what it found at a catalog version's effective epoch (section 11.12,
*Default entries, checks and builds*): a type change's check, passed or refused by the data, or an index build that
failed. A version it refused no declaration of is reported as passed, whether or not it tightens anything, so the partition
need not know which versions the frontend waits on. The frontend answers once the metadata group holds the report, with
every field as sent; the group records a report only of a version being checked (section 11.13). A partition sends
again a report left unanswered when its connection closed, and a repeated report changes nothing, the first of a
partition standing.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 4 | `partition` | u32 | at least 1 (`FieldValue` `"partition"`) |
| 4 | 1 | `kind` | u8 | 1 the check passed, 2 the data refused the change, 3 a build failed (`FieldValue` `"kind"`) |
| 8 | 8 | `tenant` | u64 | |
| 16 | 8 | `version` | u64 | the catalog version reported on, at least 1 (`FieldValue` `"version"`) |

| Section | Contents |
|---|---|
| 0 | the term table's records (section 4.4): for kind 2, the predicate, then the subject or object at fault, an IRI, a blank node or a literal, the one entry naming both when the term at fault is the predicate's IRI; for kind 3, the predicate whose build failed; none for kind 1 |
| 1 | the term table's text |

Order: unused bytes 5–7 and 24–127 zero; `partition`; `kind`; `version`; sections; the term table; one or two entries
for kind 2, one for kind 3, none for kind 1 (`CountMismatch` `"report terms"`, `expected` the most the kind takes); the
first entry of kind 1, and a second of kind 1 to 4 (`FieldValue` `"report term"`, the entry's kind). The report's reply
holds the bytes of its request.

### 13.20 Held (531)

A partition answers each Write and Operation on a Bind stream with Held once it holds the request: its references registered and
its changes in the transaction. A refusal is an Error on the stream, which ends the transaction, as on a client's stream. The
frontend takes Held as the request's answer (section 11.12, *Reads*) and relays nothing, a client's Write and Operation having no
reply. The stream stays open.

The command header is all zero. No sections. Order: command header bytes 0–127 zero; sections.

### 13.21 Frontier (532)

Storage's second read kernel, beside Scan: compute sends a frontier, many rows of one relation in one direction, and
the partition answers each row's surviving entries in its family's order (`../storage/STORAGE.md` section 18.5), in
Neighbours frames on the request's stream. A request names its lane (`../storage/STORAGE.md` section 18.1): L, the
partition's latest applied state, each frame one turn's and naming the version it read; S, one version for every
frame, which holds the partition's fold horizon on its primary until the request ends (section 11.9); X, a read-write
transaction's reads at its read version, with its own writes over them, recorded in its reads. Every family allows the
three lanes (T48, C41); section 13.12 gives the lane and version of each read. A row is named by its logical key, never
by a slot: a forward row by its subject's system ID; a reverse row of a generic-layout family by its object's term
encoding, an IRI or a blank node as tag `0x01` or `0x02` followed by its system ID and a literal as its term encoding,
and a compiled layout's by the participant's system ID (T48); an index row by its token; a version run's row and an
attribute's row by its instance's system ID (`../storage/STORAGE.md` sections 5.1, 5.3, 5.4, 7.1 and 8.1). A kind 3 key
is read in each family whose layout names rows in its form, so a key of the other form names no row there.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 1 | `lane` | u8 | 1 L, 2 S, 3 X (`FieldValue` `"lane"`) |
| 1 | 1 | `kind` | u8 | what is read: the adjacency kind (`../storage/STORAGE.md` section 7.1), 1 forward edges, 2 forward values, 3 reverse entries, 4 an index relation, 5 a `@bitemporal` attribute's or role's version runs (T48, C39); or 6 a single-valued attribute's column chunks (section 8 there, kinds 1 and 2), each row an instance and its one entry the attribute's value (`FieldValue` `"kind"`) |
| 2 | 1 | `tokenizer` | u8 | for kind 4, the index's tokenizer identifier byte, not `0xFF`; 0 otherwise (`FieldValue` `"tokenizer"`) |
| 3 | 1 | `filters` | u8 | the inline-column filters, 0 to 8 (`FieldValue` `"filters"`) |
| 4 | 4 | `partition` | u32 | the partition read, at least 1 (`FieldValue` `"partition"`) |
| 8 | 8 | `tenant` | u64 | |
| 16 | 8 | `read_version` | u64 | lane S: the version or version bound read at; lane X: the Bind's answer (section 13.10); lane L: 0 (`FieldValue` `"read_version"`) |
| 24 | 8 | `principal` | u64 | the client's principal |
| 32 | 16 | `transaction` | bytes | lane X: the transaction's identity, as its Bind named it; all zero otherwise (`FieldValue` `"transaction"`) |
| 48 | 2 | `prefix_first` | u16 | kind 1: the first type prefix of the neighbours' type range; 0 with no range |
| 50 | 2 | `prefix_past` | u16 | kind 1: one past its last, above `prefix_first`; 0 with no range (`FieldValue` `"prefix"`) |
| 52 | 4 | `top` | u32 | the most entries answered per row, read in the row's answer order below; 0 for every entry |
| 56 | 8 | zero | 0 | |
| 64 | 16 | `graph` | system ID (section 1) | the graph's; 16 zero bytes for the default graph; or 16 bytes of `0xFF` for every graph the partition holds the family in (T48) (`FieldValue` `"graph"`) |
| 80 | 16 | `predicate` | system ID (section 1) | the relation's predicate, a valid system ID, or 16 bytes of `0xFF` for every predicate of the kind the partition holds (T48, C38) (`FieldValue` `"predicate"`) |

| Section | Contents |
|---|---|
| 0 | the rows' keys' ends: u32 per row, the offset just past its key in section 1, at least one row |
| 1 | the rows' keys, back to back, each once: kinds 1, 2, 5 and 6, a system ID; kind 3, the object's term encoding for a generic-layout family, an IRI or a blank node as tag `0x01` or `0x02` followed by its system ID, or the participant's system ID for a compiled layout's (T48); kind 4, a token |
| 2 | the filters, 48 bytes each |
| 3 | the resume key: empty to start; else the one a Neighbours frame of the same request gave, unchanged |

| Filter offset | Width | Field | Type |
|---|---|---|---|
| 0 | 1 | `column` | u8: the inline column, 0 to 7, as the layout numbers them (`../storage/STORAGE.md` section 7.3) |
| 1 | 1 | `bounds` | u8: bit 0 `low` holds a bound, bit 1 `high` does; bits 2 to 7 zero |
| 2 | 1 | `width` | u8: the column's width, 1 to 18 (T48, C25) |
| 3 | 5 | zero | 0 |
| 8 | 18 | `low` | the least value passing, its sortable encoding in the first `width` bytes, the rest zero; all zero without bit 0 |
| 26 | 18 | `high` | the greatest value passing, likewise; all zero without bit 1 |
| 44 | 4 | zero | 0 |

An entry passes a filter when its column holds a value at or above `low` and at or below `high`, compared byte by byte
as its sortable encoding compares, a date or time by its first 16 bytes, the UTC instant (`../storage/STORAGE.md`
section 5.2), each bound present; an entry with no value in the column passes none. A block whose column's values
cannot pass is skipped unread. An entry survives when its neighbour's type prefix lies in the type range, it passes
every filter, and it is among the first `top` of its row that do.

The partition answers the rows in the request's order, and each row's entries in its answer order (T48, C35). A row
of kinds 1, 2 and 5 is read by its family in its subject's bucket group, and a row of kind 6 in the column chunk family
of the attribute and of the type its system ID's prefix names, which holds no inline column; a row of kinds 3 and 4 is
read in every bucket group of the partition and in both storage classes, groups ascending, each in its family's order,
so no order holds across its groups (T38; T48, C36). A predicate held in both storage classes is read in both families,
merged in the family's order (`../storage/STORAGE.md` sections 13.1 and 14.3). With `predicate` every predicate, a row
is read in each predicate's family in the order of their family keys, each entry's neighbour bytes begin with its
predicate's system ID, and the request names no filter and is answered without inline values, since its families'
inline columns differ (T48, C38). With `graph` every graph, a row is read in the family of each graph the partition
holds it in, in the order of their family keys, and each entry's neighbour bytes begin with its graph's system ID; with
both, the graph's system ID comes first, then the predicate's, as family keys order them (T48). A row of kind 5 is an instance's version run, its entries the revisions, from which
compute selects the revision in effect at the time it reads as of (T48, C39). A request holding a resume key answers
only what lies strictly after it. A resume key is the store's, `../storage/STORAGE.md` section 18.3's logical bytes,
which name after the statement the row's index in the request and the survivors the row has answered under `top`
(T48, C35): compute sends back the one a frame gave, or one it builds from a client's resume key (section 11.8), to
the same partition.
Each frame is one turn's work, within `lane_entries_max` entries examined and `lane_bytes_max` bytes of body
(`../storage/STORAGE.md` section 3.1); one that stops with entries left ends with its resume key, and the next frame,
when its stream holds credit, continues strictly after it. In lane L each frame reads the latest applied state of its own
turn, so an entry inserted behind the resume key between frames is not answered; a lane L request sent back to loading
twice is answered in lane S at the latest applied version at that moment, its frames naming that version, and from then
pins that version as lane S does, holding `H` on the primary (section 11.9; section 18.6 there; T48). `scan_stall_ms` (section 11.9) bounds how long a Frontier stays open with no frame able to leave, as it bounds a
Scan (section 13.13): past it the partition refuses the request with code 24 (TimedOut), `detail` the partition, the
message saying it stalled, and releases the version it held, so a stalled reader holds the fold horizon for no longer
than that (T48, C42).

In lane X the partition records, before a frame leaves, the range of the conflict key (section 12.2) of each row the
frame reaches, entries or none, a row of kinds 5 and 6 a forward row's, so an insert into it conflicts; a frame ending
inside a row records instead its family's range in that row's bucket group, a forward row's, or its whole family's
otherwise (T38); with every predicate, it records the graph's whole range (T48, C38), and with every graph the tenant's
whole range, as a Scan of every graph does (section 13.13; T48). A row of kind 6 whose value it
finds of storage class I records nothing, and one holding no value always records, as section 13.13 states (T48,
C43). A request of lane S or X whose `read_version` is below the partition's latest logged fold horizon is refused with
code 3 (TooOld), and on a follower, where a lane S request's version holds nothing, one whose version a logged horizon
passes is refused so for the rest of the request (section 11.9; T48, C34); one whose `read_version` is above the
partition's visibility watermark in an epoch it has not closed is refused with code 27 (AheadOfWatermark), `detail` the
watermark.

Order: unused bytes 56–63 and 96–127 zero; `lane`; `kind` 1 to 6; `tokenizer`, nonzero exactly for kind 4;
`filters`, 0 for kind 6 and with `predicate` every predicate;
`partition`; `read_version` 0 for lane 1 and a version or version bound for lanes 2 and 3; `transaction` not zero exactly
for lane 3; `prefix_first` and `prefix_past` both 0, or `prefix_first` below `prefix_past` with kind 1 (`FieldValue`
`"prefix"`); `graph` 16 zero bytes, a valid system ID or 16 bytes of `0xFF` (`FieldValue` `"graph"`; T48); `predicate` a valid
system ID or 16 bytes of `0xFF`; sections; section 0 a whole number of u32s
(`SectionLength`) and not empty (`CountMismatch` `"rows"`); each end at least the one before and above it, every key
holding at least one byte, the last end section 1's length (`FieldValue` `"rows"`, the row); kinds 1, 2, 5 and 6 each key
16 bytes and a valid system ID; kind 3 each key 16 bytes and a valid system ID, or a term encoding (section 10.1, as
`../storage/STORAGE.md` section 5.1 stores it) filling the key, an IRI's or a blank node's tag `0x01` or `0x02` followed
by a valid system ID (T48); and, of every kind, no key equal to an earlier row's (`FieldValue` `"row"`, the row; T48,
C35); section 2
a whole number of 48-byte filters (`SectionLength`), `filters` of them (`CountMismatch` `"filters"`), each: bytes 3–7 and
44–47 zero and the bytes of `low` and `high` past `width`, and an absent bound's, zero (`PaddingNotZero`), `column` at
most 7, `bounds` bits 2 to 7 zero, `width` 1 to 18 (`FieldValue` `"filter"`, the filter). A request naming a partition
the role address does not serve is refused with Error 1 (Protocol), the message naming it, and a `transaction` the
partition does not hold with Error 1, `detail` 1, as a Scan's is.

### 13.22 Neighbours (533)

A batch of a Frontier's answer: the rows the turn reached and their surviving entries. Neighbours frames spend credit as
Entries frames do (section 11.3), and the stream's last frame carries `LAST`.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 4 | `rows` | u32 | the batch's row records |
| 4 | 4 | `entries` | u32 | the batch's entries |
| 8 | 8 | `version` | u64 | the version the frame read: in lane L the partition's latest applied version at its turn, which the frames of one request may each name differently; in lanes S and X the request's `read_version` (`FieldValue` `"version"`) |
| 16 | 1 | `end` | u8 | 1 the request's answer ends with this frame; 0 a resume key follows in section 4 and the answer goes on (`FieldValue` `"end"`) |
| 24 | 8 | `widths` | bytes | byte `c` the width of the family's inline column `c`, 1 to 18 (T48, C25), and 0 past its last column; all zero for a request naming every predicate |

| Section | Contents |
|---|---|
| 0 | the rows, 16 bytes each, in the request's order (T48, C35) |
| 1 | the entries, 32 bytes each, row by row, each row's in its answer order (section 13.21) |
| 2 | the neighbours' bytes, back to back |
| 3 | the inline values: per entry, each inline column's value in `width` bytes, zero where absent |
| 4 | the resume key when `end` is 0; empty otherwise |

| Row offset | Width | Field | Type |
|---|---|---|---|
| 0 | 4 | `row` | u32: the row's index in the request |
| 4 | 4 | `first` | u32: its first entry's index in section 1, where the row before's entries end |
| 8 | 4 | `count` | u32: its entries in this frame, 0 for a row holding none that survive |
| 12 | 1 | `more` | u8: 1 the row continues in the next frame; 0 otherwise |
| 13 | 3 | zero | 0 |

| Entry offset | Width | Field | Type |
|---|---|---|---|
| 0 | 4 | `neighbour` | u32: its neighbour's offset in section 2, where the entry before's ends |
| 4 | 4 | `neighbour_len` | u32: its length: 16 for kinds 1, 3 and 4, whose neighbours a frame names by system ID, the subject's for kinds 3 and 4; kind 2's a term encoding in the generic layout or a sortable value in a compiled one; kind 5's the revision's asserted start as a big-endian u64, then its value's sortable encoding (T48, C39); kind 6's the attribute's value, its sortable encoding; for a request naming every predicate, 16 more, the neighbour's bytes following the predicate's system ID (T48, C38); for a request naming every graph, 16 more, the graph's system ID first, before the predicate's when both are named (T48) |
| 8 | 16 | `statement` | system ID (section 1): the statement's ID; 16 zero bytes for none, and for kind 6 |
| 24 | 1 | `valid` | u8: bit `c` set when inline column `c` holds a value |
| 25 | 7 | zero | 0 |

Order: unused bytes 17–23 and 32–127 zero; `version` a version or version bound; `end` 0 or 1; `widths` each 0 to 18,
none nonzero after a 0 (`FieldValue` `"widths"`); sections; section 0 a whole number of 16-byte rows, `rows` of them,
and section 1 of 32-byte entries, `entries` of them (`SectionLength`, `CountMismatch` `"entries"`); each row: bytes 13–15
zero (`PaddingNotZero`), `row` above the row before's, `first` where the row before's entries end and `first + count`
within the entries, `more` 0 or 1, and 1 only on the frame's last row (`FieldValue` `"row"`, the row); the rows' entries ending with the last entry
(`CountMismatch` `"entries"`); each entry: bytes 25–31 zero (`PaddingNotZero`), `neighbour` where the entry before's
ends and within section 2, `statement` zero or a valid system ID, `valid` without bits past the family's columns
(`FieldValue` `"entry"`, the entry); section 2 ending with the last neighbour (`CountMismatch` `"neighbours"`); section 3
the entries times the columns' widths (`CountMismatch` `"inline values"`); section 4 empty exactly when `end` is 1
(`CountMismatch` `"resume key"`). The receiver, the side that dialled, refuses as section 13.14 has it for Entries,
closing the connection: in lanes S and X a `version` other than the first frame's; `widths` other than the first
frame's; a frame after one with `end` 1; a row continued by a frame whose first row is not that row, and a frame's first
row not above the previous frame's last when that one did not continue; `LAST` on a frame without `end` 1, or `end` 1
without `LAST`.

### 13.23 Session (534)

From stage 5.8. The role holding the client endpoint registers a session for a principal in the metadata state machine's
registry (section 11.12a): the identity it drew and the principal the session belongs to. The system group answers with
the same bytes once the registration is durable. A Session is never sent again: a requester whose answer was lost
answers its client code 17, and the client registers again under a new identity (section 11.12a), since a repeat could
register an identity whose eviction has not yet reached the partitions.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 16 | `session` | session (section 1) | not all zero (`FieldValue` `"session"`) |
| 16 | 8 | `principal` | u64 | the principal the session belongs to |

No sections. Order: unused bytes 24–127 zero; `session`; sections. A full registry holding no session it may evict
refuses with code 14 (SessionBusy), registering nothing (section 11.5). A session the registry holds, for any principal,
is refused with Error 1 (Protocol), the message naming the session. A principal the metadata state machine does not hold
is refused with code 7 (Refused), the message naming it.

### 13.24 Check (535)

From stage 5.8. The owner checks of one turn of an endpoint core (section 11.12a): for each, whether the registry holds
the session and the principal is the one it belongs to. A check of kind 2 that passes marks the session in flight, until
an Ended names it or, its connection closed, `transaction_ms` has passed (section 11.12a); a check of kind 1 marks
nothing. A frame holds one check per session. The system group decides the checks in their order and answers the frame
whole, with every field as sent and each `outcome` filled.

The command header is all zero.

| Section | Contents |
|---|---|
| 0 | checks, 32 bytes each, at least one |

| Check offset | Width | Field | Type |
|---|---|---|---|
| 0 | 16 | `session` | session (section 1), not all zero |
| 16 | 8 | `principal` | u64: the connection's principal |
| 24 | 1 | `kind` | u8: 1 a request that records nothing in a partition's table, a Recall, or the question after every partition answered a Recall with 18; 2 one that may, a Commit, a Drop, a one-frame write or an Ingest |
| 25 | 1 | `outcome` | u8: 0 in a request; in a reply, 1 passed, or the code section 11.5 refuses the request with: decided in this order: 12 the registry does not hold the session, 21 it belongs to another principal, 14 it has a request in flight, of either kind |
| 26 | 6 | zero | 0 |

Order: command header bytes 0–127 zero; sections; section 0 a whole number of 32-byte checks (`SectionLength`), at least
one (`CountMismatch` `"checks"`); each check in order: bytes 26–31 zero (`PaddingNotZero`), `session` not all zero and
above the one before, compared byte by byte, `kind` 1 or 2, `outcome` 0, 1, 12, 14 or 21 (`FieldValue` `"check"`, the
check's index). The system group, which receives the request, also refuses a check whose `outcome` is not 0, and the
requester a reply holding an `outcome` of 0 or a check other than the one it sent (`FieldValue` `"check"`, the check's
index).

### 13.25 Ended (536)

From stage 5.8. The sessions whose requests in flight have ended (section 11.12a): each request ended before reaching a
partition, was answered by its partition, or lost the role connection carrying it. The system group takes each mark away
and answers nothing: no frame answers on the request's stream, which ends with it, and a body the system group refuses
is dropped unanswered, as a CloseRequest's is (section 13.16). An Ended ends a mark begun on its own connection, or one
whose connection has closed (section 11.12a); a session with no such mark is passed over, so an Ended sent again changes
nothing.

The command header is all zero.

| Section | Contents |
|---|---|
| 0 | sessions, 16 bytes each (section 1), at least one |

Order: command header bytes 0–127 zero; sections; section 0 a whole number of 16 bytes (`SectionLength`), at least one
session (`CountMismatch` `"sessions"`); each not all zero and above the one before, compared byte by byte (`FieldValue`
`"session"`, the session's index).

### 13.26 Roster (537)

From stage 5.9. A compute node reads the list of compute nodes from the system group (section 11.14), when an
EpochClose names a change and at its start. The requester sends `version` 0 and six empty sections; the system group
answers with the command header and the sections of Nodes (section 5.4), under the same rules, the list as the latest
close that named a change left it and `version` that close's epoch.

A body with `version` 0 and no node is a request. Order: unused bytes 8–127 zero; in a request, every section empty
(`CountMismatch` `"roster"`, the sections' bytes, 0); a reply as section 5.4 states, with `self` `ABSENT` and with two
exceptions: before any close has named a change, the reply holds the bytes of its request, which a compute node takes as
no list yet; and a reply may hold no node, at a `version` of 1 or more, when the last node has left the list. A compute
node reads Roster at its start and for a close whose `epoch` is above the version it holds, and for no other.

### 13.27 Declare (538)

From stage 5.8. The role holding the client endpoint asks the system group for a catalog version (section 11.12a). Of
`kind` 1 it is the version a transaction's declarations make, built on the version current when the transaction began
(section 11.13, *A new version*); of `kind` 2, the version a committed Drop calls for (section 11.13, *Drops*). The
system group builds the version, commits it as pending and answers with every field as sent and `version` and
`commit_version` filled: for `kind` 1 once the version's epoch is assigned and, for a tightening, once every partition
has passed its check; for `kind` 2 once the version is pending and durable, or at once, with `version` 0, when the Drop
changes nothing in the catalog; such a reply holds the bytes of its request. For `drop` 4 the same command removes the
tenant's default-entry records, with the version or alone when no version is built, durable before the reply. The
requester sends a Declare for a Drop of kind 1 or 4 alone; kinds 2 and 3 call for no version, and one sent for them is
answered `version` 0. The stream stays open until then. A connection that closes under it changes nothing about a check,
which is the metadata state machine's.

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 1 | `kind` | u8 | 1 a transaction's declarations, 2 a Drop's removal (`FieldValue` `"kind"`) |
| 1 | 1 | `drop` | u8 | for `kind` 2, the Drop's `kind`, 1 to 4 (section 6.5); 0 for `kind` 1 (`FieldValue` `"drop"`) |
| 8 | 8 | `tenant` | u64 | the tenant |
| 16 | 8 | `built_on` | u64 | for `kind` 1, the tenant's catalog version when the transaction began, 0 for a tenant holding none; 0 for `kind` 2 (`FieldValue` `"built_on"`) |
| 24 | 8 | `principal` | u64 | the client's principal |
| 32 | 8 | `version` | u64 | 0 in a request; in a reply, the catalog version committed as pending, 0 for a `kind` 2 that changed nothing |
| 40 | 8 | `commit_version` | u64 | 0 in a request and for `kind` 2; in a `kind` 1 reply, the first version of the effective epoch, its low 22 bits zero (`FieldValue` `"commit_version"`) |

The sections are a Write's (section 7.2). For `kind` 1 section 0 holds the transaction's declarations, records of kinds
8 to 10 in the order its Writes held them. For `kind` 1 the term table holds what the declarations name, as a Write's
does. For `kind` 2 section 0 is empty, and the term table holds the dropped predicate's or graph's IRI, one entry, for a
Drop of kind 1 or 2 and none for kinds 3 and 4. `principal` is the client's, recorded with the version for stage 7's
audit and deciding nothing here.

A body with `version` and `commit_version` 0 is a request. Order: unused bytes 2–7 and 48–127 zero; `kind`; `drop`;
`built_on`; `commit_version`; sections as section 7.2 states; for `kind` 1 at least one record, each of kind 8 to 10
(`FieldValue` `"declaration"`, the record's index); for `kind` 2 no record (`CountMismatch` `"declarations"`) and the
term table's entries one for `drop` 1 or 2 and none for `drop` 3 or 4 (`CountMismatch` `"drop terms"`), that one an IRI
(`FieldValue` `"drop term"`, as section 6.5 names it); in a `kind` 1 reply `version` and `commit_version` at least 1
(`FieldValue` `"version"`). A tenant the metadata state machine does not hold is refused with code 7 (Refused), `detail`
the tenant. A Cancel on a Declare's stream changes nothing the system group does. The system group refuses as section
11.13 states, and the requester passes each to its client: code 2, kind 5, for a version built on one no longer current
or on one being checked; code 7 for a declaration the catalog refuses, a tightening with a loosening, a version past
`catalog_bytes_max` and a change the data contradicts; code 24 for a partition silent past `catalog_check_ms`.

### 13.28 Defaults (539)

From stage 5.8. The role holding the client endpoint reads the default-entry records the metadata state machine holds
for a tenant (section 11.13, *Default entries*), which a Describe names as recorded (section 8.3). The requester sends
`position` 0 and two empty sections. The system group answers with the predicates holding a record, each an IRI of the
term table, in the order of their texts' UTF-8 bytes. An answer that does not fit one frame within the requester's
`max_frame` continues in further Defaults frames on the stream, each with the first's command header, the last alone
flagged `LAST`. Each frame holds a whole term table, checked by itself; the entries' order and an entry's index run on
across the frames. The requester refuses a frame breaking the first's command header with `FieldValue` (`"defaults"`),
and reads no further once the answer's bytes pass `catalog_bytes_max`. A Defaults stream starts with the Welcome's
`initial_credit`, as a Catalog's does. The frames spend credit as Result frames do (section
11.3).

| Command header | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 8 | `tenant` | u64 | the tenant |
| 8 | 8 | `position` | u64 | 0 in a request; in a reply, the metadata state machine's log position the records were read at, from 1 |

| Section | Contents |
|---|---|
| 0 | the term table's records (section 4.4): one IRI per predicate holding a record; empty in a request |
| 1 | the term table's text |

A body with `position` 0 is a request. Order: unused bytes 16–127 zero; sections; the term table; in a request, no entry
(`CountMismatch` `"defaults"`, the entries, 0); in a reply, each entry of kind 1 (`FieldValue` `"default"`, the entry's
kind) and above the one before by its text's bytes, across frames too (`FieldValue` `"default order"`, the entry's
index). A tenant the metadata state machine does not hold is refused with code 7 (Refused), `detail` the tenant.
