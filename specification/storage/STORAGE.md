# UltraGraphDB tile store

The tile store is the storage engine of one partition replica: it holds a replica's graph as tiles, bounded and
checksummed blocks in a grid file of its own, with a delta in memory above them. This file specifies it completely: the
settings that shape its bytes, every on-disk byte, the in-memory state those bytes are derived from, the deterministic
algorithms that decide which bytes are written where, the reads it answers, the faults it detects and how it answers
them, and what its simulator checks. Two implementations built from this file alone, fed the same rounds under the same
settings, write the same grid byte for byte, and `../conformance/storage` holds the vectors that hold them to it.

The design and its reasoning are in `../../openspec/notes/UltraGraphDB Scalable Tile Store.md` (decisions T1–T41) and
the study of it against the source in `../../openspec/notes/UltraGraphDB Tile Store Feasibility.md`; where this file and
the notes differ, this file is the specification and section 20 records why. `../binary_protocol/PROTOCOL.md` specifies
the rounds the store applies and the frames it answers in; `../schema/SCHEMA.md` and `../schema/CATALOG.md` the catalog
whose layout it stores; the write-ahead log is the `write-ahead-log` capability's.

## Contents

1. Conventions
2. The model
3. Settings and the derivation version
4. Primitive encodings
5. Value and key encodings
6. Tiles
7. Adjacency blocks
8. Column chunks
9. Record runs
10. The free set
11. Digests and the checkpoint identity
12. The grid
13. The manifest and the directory
14. Versions, the fold horizon and the delta
15. Apply
16. Compaction
17. Checkpoints, recovery and repair
18. Reads and lanes
19. Faults, invariants and refusals
20. Decisions
21. What this specification asks of the others
22. Conformance
23. Sources

## 1. Conventions

| Rule | Statement |
|---|---|
| Byte order | Every integer field of a grid structure is little-endian, read byte by byte with no alignment assumed in memory. Three exceptions are big-endian, because they sort as bytes: a system ID (16 bytes, `PROTOCOL.md` section 1), a canonical key (section 5.4) and a sortable value (section 5.2) |
| Integers | `u8`, `u16`, `u32`, `u64`, `u128` unsigned, `i8` to `i128` two's complement. Arithmetic on counts and offsets never wraps; where this file says *wrapping*, it is modulo 2^width |
| Block | 4,096 bytes, the unit of every grid I/O and of placement |
| `align8(n)` | The least multiple of 8 at or above `n`; `blocks(n)` the least number of blocks holding `n` bytes |
| `ABSENT` | The u32 `0xFFFF_FFFF`: no slot, no statement |
| Zero | Every padding, reserved and unused byte is zero, and a reader refuses a nonzero one (`Padding`) |
| XXH3-64, XXH3-128 | As `doc/xxhash_spec.md` of the xxHash repository specifies them, seed 0 unless a rule names another. A checksum is an XXH3-64 stored as a u64. An XXH3-128 value `h` is stored as 16 bytes: `h mod 2^64` as a u64, then `h div 2^64` as a u64 |
| `bitlen(x)` | 0 for `x = 0`, otherwise the position of `x`'s highest set bit plus one: `bitlen(1) = 1`, `bitlen(255) = 8` |
| `esc(x)` | The order-preserving form of a byte string `x`: every byte of `x`, each `0x00` written as `0x00 0xFF`, then `0x00 0x00`. For byte strings `a` and `b`, `esc(a) ‖ c < esc(b) ‖ d` as bytes exactly when `a < b`, or `a = b` and `c < d`, so a concatenation of escaped components compares as the tuple of its components |
| Integers in keys and values | Every integer inside a key is big-endian, so keys sort as their numbers; every integer inside a value is little-endian unless its row says big-endian |
| Version | `PROTOCOL.md` section 1's u64, `epoch << 22 \| sequence`; version 0 precedes every commit |
| Position | A round's place in its partition's log, gapless from 1 (`PROTOCOL.md` section 12.1) |
| One encoding | Every structure below has exactly one encoding of its content. An encoder writes it; a decoder refuses every other byte sequence with the refusal section 19 names, so decoding and re-encoding gives the bytes read |
| Requirement words | SHALL and MUST state requirements; MAY states a permission. Everything else is definition |

| Term | Meaning |
|---|---|
| Replica | One copy of one partition, on one core of one storage node. Every structure of this file belongs to exactly one replica |
| Grid | The replica's data file, a whole number of extents (section 12) |
| Extent | `extent_bytes` contiguous bytes of the grid, the unit of allocation and reclamation |
| Tile | An immutable, self-describing, checksummed structure in the grid: an adjacency block, a column chunk, a record run or a free set |
| Reference | The 24 bytes naming a tile: its address, its length and its checksum (section 6.4) |
| Slot | A `u32`, the dense position the replica gives an entity of one (bucket group, type), from 0 (section 14.6) |
| Delta | The replica's in-memory changes above its tiles, built by apply and consumed by compaction (section 14) |
| Fold horizon `H` | The version at or below which every replica folds versions away, logged in every round (section 14.2) |
| Round boundary | The moment after a round's apply completes and before the next round's begins; every deterministic decision of this file is taken at one |

## 2. The model

A replica's state is a function of its log. Apply turns each committed round into delta entries (section 15);
compaction, at round boundaries, turns delta entries and existing tiles into new tiles (section 16); a checkpoint
publishes the tiles under a superblock and lets the log be trimmed (section 17). Every decision those steps take, which
tile to rewrite, how to cut it, where to place it, when to checkpoint, reads only the rounds applied, the settings of
section 3 and state derived from both. No decision reads a clock, a random source, a read, the cache's contents, the
order in which I/O completes, or another replica's load. Hence:

- **Determinism.** Two replicas of one partition that have applied the same rounds under the same derivation version
  (section 3.2) hold the same tiles at the same addresses, and their checkpoints at the same position have the same
  identity (section 11.3). Replay after a crash rebuilds the same tiles again.
- **Out of place.** No byte a durable superblock reaches is overwritten (section 12.4). A torn write damages only bytes
  nothing durable references.
- **The log is the only stream synced per commit.** The grid is synced at checkpoints (section 17); between them, the
  delta holds what the tiles lack, and the log above the checkpoint rebuilds it.
- **Shared nothing.** A replica's engine runs on its core's thread. It holds no lock and no atomic on the data plane;
  other threads reach it only by messages.

The engine is a state machine with no I/O of its own. Its inputs are rounds to apply, read requests, the completions
of the I/O it asked for, and the epoch closes the sequencer delivers; its outputs are answers and I/O requests: read
`length` bytes at an address, write blocks at an address, sync the grid, allocate an extent, punch an extent, write the
WAL's superblock, trim the WAL. Its compio shell performs them on the core's ring, except allocation and punching, which
go through rustix on compio's blocking pool (T10); its simulator performs them under faults (section 19.5).

## 3. Settings and the derivation version

### 3.1 The `[tiles]` table

Every setting below decides a byte the store writes or an answer it gives, so every storage instance of a cluster holds
the same values: the config service checks them equal across the cluster's storage instances and refuses a revision
changing any of them before stage 9.5 (`fixed`). They are integers.

| Setting | Default | Bounds | Meaning |
|---|---|---|---|
| `extent_bytes` | 1,048,576 | a power of two, 131,072 to 67,108,864 | Bytes of an extent |
| `grid_extents` | 16,384 | 16 to 16,777,216 | Extents of the grid; the grid holds `extent_bytes × grid_extents` bytes |
| `tile_bytes_max` | 65,536 | a multiple of 4,096, 16,384 to 1,048,576, at most `extent_bytes − 4,096` | The largest tile |
| `tile_entries_max` | 4,096 | 256 to 65,536 | The most entries compaction puts into one adjacency block or record run |
| `chunk_slots` | 4,096 | a power of two, 1,024 to 65,536 | The most slots a column chunk covers (section 8.3) |
| `filter_bits_per_key` | 10 | 0 to 32; 0 writes no filter | A filter's bits per key |
| `theta_percent` | 50 | 10 to 400 | Delta entries, as a percentage of a tile's entries, that make it due |
| `delta_entries_max` | 5,242,880 | 65,536 to 268,435,456 | The delta pool's capacity in entries |
| `delta_watermark_percent` | 75 | 50 to 95 | The pool's share in use above which compaction runs on pressure and admission stops |
| `delta_row_entries_max` | 1,024 | 64 to 65,536 | The delta entries one row may hold before its tile is due first |
| `delta_age_rounds` | 4,096 | 64 to 1,048,576 | Rounds after which a delta entry makes its tile due |
| `beat_factor` | 4 | 1 to 64 | Compaction work each applied delta change earns, in entries (section 16.4) |
| `cold_after_rounds` | 65,536 | 1,024 to 2^32 | A tile whose predecessor lived this many rounds or more is written to the cold stream |
| `cleaner_free_percent` | 15 | 5 to 50 | The share of free extents below which the cleaner runs |
| `checkpoint_bytes` | 268,435,456 | a multiple of 4,096, 16,777,216 to 2^34 | Log bytes between checkpoints (`C`) |
| `retention_ms` | 30,000 | at least the read-write transaction bound plus the longest read deadline the cluster allows | How far back a snapshot read is answerable |
| `lane_entries_max` | 65,536 | 1,024 to 1,048,576 | Entries one lane L request examines in a turn |
| `lane_bytes_max` | 262,144 | 16,384 to the smallest `max_frame` of the cluster minus 256 | Bytes one lane L request answers in a turn |

Settings that change no byte stay per node, in `[storage]`: the tile cache's size, I/O depths and
`power_loss_protection`, which tunes the log's round batching and never removes a sync.

### 3.2 The derivation version

The derivation version names everything that decides a written byte: this file's format and algorithms, and the
settings. It is the XXH3-64 of 4 + 8 × 18 bytes: the u16 format `1`, the u16 algorithm `1`, then the eighteen settings
of section 3.1 as u64s in the table's order. Format 1 and algorithm 1 are this file; a change to any byte layout raises
the format, a change to any decision rule of sections 14 to 17 raises the algorithm. A round names the derivation
version it was applied under (`PROTOCOL.md` section 12.1, `derivation`, which section 21 widens to a u64); replay
refuses a round naming another (`DerivationMismatch`), and a replica whose settings give another version than its
superblock's refuses to open.

### 3.3 Limits that follow

A stored value, a term's encoding, a row key or a neighbour's bytes, is at most `tile_bytes_max / 4` bytes; a write
holding a longer one is refused at execution (`ValueTooLarge`), so every row and every single-slot column chunk fits a
tile. Format 1 keeps no value outside the tiles.

## 4. Primitive encodings

Every tile section is one of six encodings, named by a u16 code in its directory entry. Each starts at a multiple of 8
from its tile's first byte and its length is a multiple of 8.

### 4.1 Packed integers: FOR (1) and DELTA (2)

A vector of `n` u64 values is written in one of two encodings.

**FOR**, frame of reference, for any vector:

| Offset | Width | Field | Value |
|---|---|---|---|
| 0 | 4 | `count` | `n` |
| 4 | 1 | `width` | `bitlen(max − min)`; 0 when `n` is 0 |
| 5 | 3 | zero | |
| 8 | 8 | `base` | `min`; 0 when `n` is 0 |
| 16 | `align8(ceil(n × width / 8))` | `packed` | `n` fields of `width` bits, field `i` holding `v[i] − base` |

**DELTA**, for a nondecreasing vector of at least two values:

| Offset | Width | Field | Value |
|---|---|---|---|
| 0 | 4 | `count` | `n`, at least 2 |
| 4 | 1 | `width` | `bitlen(dmax − dmin)`, where `d[i] = v[i+1] − v[i]` for `i` from 0 to `n − 2` |
| 5 | 3 | zero | |
| 8 | 8 | `first` | `v[0]` |
| 16 | 8 | `dbase` | `dmin` |
| 24 | `align8(ceil((n − 1) × width / 8))` | `packed` | `n − 1` fields of `width` bits, field `i` holding `d[i] − dbase` |

**Packing.** Fields are laid out back to back as a bit stream, least significant bit first: bit `j` of field `i` is
stream bit `i × width + j`, and stream bit `b` is bit `b mod 8` (value `2^(b mod 8)`) of byte `b div 8`. Bits past the
last field are zero.

**Choice.** An encoder writes DELTA when the vector is nondecreasing, has at least two values and DELTA's length is
strictly less than FOR's; otherwise FOR. A decoder refuses the other (`NotChosen`), a width other than the rule's
(`NotMinimal`), and a DELTA vector that is not nondecreasing, which cannot occur since its fields are unsigned.

A section holding a vector says how its values are bounded (a slot is a u32, a type prefix a u16); a decoder refuses a
value past the bound (`Range`).

**Checks, in order,** for FOR and DELTA: `count` against the section's rule (`Count`); the zero bytes and the packed
bits past the last field (`Padding`); `width` against the rule (`NotMinimal`); the choice (`NotChosen`); each value's
bound (`Range`).

### 4.2 BITMAP (3)

| Offset | Width | Field | Value |
|---|---|---|---|
| 0 | 4 | `count` | bits |
| 4 | 4 | zero | |
| 8 | `8 × ceil(count / 64)` | `words` | bit `i` is bit `i mod 64` of the u64 word `i div 64` |

Bits at or past `count` are zero. Checks: `count` (`Count`), then the zero bytes and the bits past `count` (`Padding`).

### 4.3 BYTES (4)

A vector of `n` byte strings:

| Offset | Width | Field | Value |
|---|---|---|---|
| 0 | 4 | `count` | `n` |
| 4 | 4 | `total` | the strings' total length |
| 8 | `4 × n` | `ends` | u32 per string: the offset just past it within `data`; nondecreasing, the last equal to `total` |
| `8 + 4n` | `total` | `data` | the strings back to back |
| | to a multiple of 8 | zero | |

Checks: `count` (`Count`); each end at or above the one before (`Order`); the last end equal to `total`, 0 when `n` is 0
(`Count`); the padding (`Padding`).

### 4.4 FIXED (5)

A vector of `n` values of `w` bytes each:

| Offset | Width | Field | Value |
|---|---|---|---|
| 0 | 4 | `count` | `n` |
| 4 | 4 | `width` | `w`, 1 to 256 |
| 8 | `n × w` | `values` | back to back |
| | to a multiple of 8 | zero | |

Checks: `count` (`Count`); `width` against the section's rule (`Encoding`); the padding (`Padding`).

### 4.5 FILTER (6)

A Bloom filter over a set of keys, each a byte string.

| Offset | Width | Field | Value |
|---|---|---|---|
| 0 | 4 | `keys` | the keys inserted, duplicates counted |
| 4 | 1 | `k` | probes per key |
| 5 | 3 | zero | |
| 8 | 4 | `bits` | `m` |
| 12 | 4 | zero | |
| 16 | `m / 8` | `words` | the bit array as BITMAP's words |

With `b = filter_bits_per_key`: `m = max(64, 64 × ceil(keys × b / 64))`, and `k = min(16, max(1, (b × 693 + 500) div 1000))`,
7 at the default. A key `x` sets, for `j` from 0 to `k − 1`, bit `(h + j × g) mod m` with `h = XXH3-64(x)`,
`g = rotl64(h, 32) | 1`, and `h + j × g` wrapping in 64 bits. A key may be present when all its `k` bits are set, and is
absent otherwise. With `filter_bits_per_key` 0 no filter section is written, and every key may be present. Checks:
`keys` equal to the count the section's rule gives (`Count`); `k` and `bits` equal to the formulas (`Parameter`); the
zero bytes (`Padding`).

## 5. Value and key encodings

### 5.1 The term encoding

A term in the generic layout, a literal object of a quad or a reverse row's key in it, is stored in the store's term
encoding: `PROTOCOL.md` section 10.1's tags and payloads, with one difference that section 10 already states: where a
Result names an IRI, a blank node or a datatype by the hash of its text, the store names it by its 16-byte system ID.
So tag `0x01` and `0x02` carry the term's system ID, and tags `0x32` and `0x33` the datatype's system ID in place of its
IRI's hash. Every other tag, the hashed lexical forms of `0x21`, `0x23`, `0x24`, `0x26`, `0x27`, `0x29`, `0x2A`,
`0x31` and `0x33` among them, is exactly as section 10.1 gives it; the record run of section 9.3, kind 3, maps each such
hash to its text. Equal terms encode equally; the encoding orders for equality, not by value.

### 5.2 The sortable encoding

A value of a declared attribute in a compiled layout (section 8, and inline columns of section 7.3) is stored in its
descriptor's sortable encoding, which compares, byte by byte, as the values compare. Its width is fixed except where
the table says variable. Tags are `CATALOG.md` section 3.1's.

| Tag | Type | Width | Encoding |
|---|---|---|---|
| 1 | `Bool` | 1 | `0x00` false, `0x01` true |
| 2–6 | `Int8` … `Int128` | 1, 2, 4, 8, 16 | big-endian two's complement with the sign bit inverted |
| 7–10 | `UInt8` … `UInt64` | 1, 2, 4, 8 | big-endian |
| 11, 12 | `Float32`, `Float64` | 4, 8 | the IEEE 754 bits, after mapping −0.0 to +0.0 and every NaN to the quiet NaN `0x7FC0_0000` or `0x7FF8_0000_0000_0000`: when the sign bit is set, every bit inverted; otherwise the sign bit set; then big-endian |
| 13 | `Decimal(p, s)` | 16 | the value × 10^s, an exact i128, as `Int128` |
| 14, 15 | `String`, `Text` | variable | the UTF-8 bytes |
| 16 | `Bytes` | variable | the bytes |
| 17–20 | `Date`, `Time`, `DateTime`, `LocalDateTime` | 18 | `PROTOCOL.md` section 10.5's payload of `xsd:date`, `xsd:time`, `xsd:dateTime` and `xsd:dateTime` without an offset: `T` as `Int128`, then the offset in minutes as `Int16`, its absence as `0x7FFF` before inversion |
| 21 | `Duration` | 24 | months as `Int64`, then seconds × 10^18 as `Int128` |
| 22 | `Interval<T>` | — | stored as its bound attributes, `SCHEMA.md` section 12.2; no value of this tag is stored whole |
| 23, 35 | `Iri`, an instance | 16 | the system ID |
| 24 | an enum | 4 | the member's index in declaration order, from 0, as `UInt32`; members only append before stage 9.5 |
| 25, 26 | `List<T>`, `Set<T>` | variable | each element's encoding, in list order for a list and ascending byte order for a set, each preceded by its length as a big-endian u32 |
| 27 | a record | variable | each field's encoding in declaration order, each preceded by its length as a big-endian u32 |
| 28 | `Vector<E, n>` | `4n` or `2n` | the elements little-endian, as `PROTOCOL.md` tags `0x40` and `0x41` hold them; a vector compares by bytes only |
| 29 | `Geo` | variable | the canonical `geo:wktLiteral` lexical form's UTF-8 bytes |
| 30 | `Measure` | 12 | the number as `Float64`, then the unit's number as `UInt32` |
| 31, 32 | `Unit`, `QuantityKind` | 4 | the unit's or quantity kind's number as `UInt32` |
| 33, 34 | `schema.EntityType<T>`, `schema.RelationType<T>` | 2 | the type prefix as `UInt16` |
| 36 | `Id128` (T26) | 16 | big-endian |

A unit's and a quantity kind's number is assigned when it is first declared in a tenant, from 1, and never reused, as
type prefixes are; tag 36 and the numbers are `CATALOG.md`'s to add (section 21). A fixed-width value is *small* when
its width is at most 16 bytes; only small values sit inline in an adjacency entry (section 7.3).

### 5.3 Tokens

An index row's key is a token. A token of a `range`, `interval` or `count` index over a declared attribute is the
tokenizer's identifier byte (`tokenizers` capability) followed by the value's sortable encoding, so a range of values
is a range of tokens; any other token is the tokenizer's identifier byte and token bytes as that capability gives them.
A token of more than 256 bytes, its identifier byte included, is stored as the byte `0xFF` followed by the XXH3-128 of
all its bytes as 16 big-endian bytes, 17 bytes in all, and is lossy: a reader confirms every match against the value it
indexes. No tokenizer's identifier byte is `0xFF`.

### 5.4 Canonical keys

A key of a row, a neighbour or a record compares as bytes. Its canonical form:

| Key | Canonical bytes |
|---|---|
| A slot | 4 bytes, big-endian |
| A system ID | its 16 bytes |
| A term | its term encoding (section 5.1) |
| A token | its bytes (section 5.3) |
| A record run's key | as its kind gives it (section 9.3) |

Within one tile every row key has the same form, and so does every neighbour. Where a row key and a neighbour are
joined into one byte string, in a fence (section 13.1) or a resume key (section 18.3), each component that is not of
fixed width (a term, a token, a record key, a value's bytes) is written as `esc` of it, so the string compares as the
pair; a slot (4 bytes) and a system ID (16 bytes) are written as they are.

## 6. Tiles

### 6.1 Layout

A tile is a 128-byte header, a section directory of 16 bytes per section, and its sections:

```
0        header (128)
128      directory: sections × 16
128+16s  section 0 ... section s−1, back to back, each a multiple of 8 long
length   end of the last section
```

Its length is at most `tile_bytes_max`. In the grid a tile starts at a block boundary and the bytes from its end to the
next block boundary are zero.

### 6.2 The header

| Offset | Width | Field | Value |
|---|---|---|---|
| 0 | 4 | `magic` | the bytes `55 47 54 4C` ("UGTL") |
| 4 | 2 | `format` | 1 |
| 6 | 1 | `shape` | 1 adjacency block, 2 column chunk, 3 record run, 4 free set |
| 7 | 1 | `kind` | the shape's kind (sections 7 to 10) |
| 8 | 8 | `checksum` | XXH3-64 of the tile's `length` bytes with this field zeroed |
| 16 | 4 | `length` | the tile's bytes |
| 20 | 2 | `sections` | the directory's entries |
| 22 | 2 | `flags` | bit 0 `versioned`: the version sections are present; bit 1 `class_i`: the tile holds storage class I (section 14.3); every other bit zero |
| 24 | 8 | `tenant` | the tenant; 0 for a tile not of one tenant (the free set, the manifest, the session table) |
| 32 | 16 | `graph` | the graph's system ID; 16 zero bytes for the default graph and for a tile not of one graph |
| 48 | 16 | `subject` | an adjacency block's predicate, a column chunk's attribute predicate; zero otherwise |
| 64 | 2 | `type` | a column chunk's, slot map's or identity column's type prefix; 0 otherwise |
| 66 | 1 | `direction` | an adjacency block's: 1 forward, 2 reverse; 0 otherwise |
| 67 | 1 | `order` | an adjacency block's row order (section 7.4): 0 by neighbour; `1 + i` ascending, `0x81 + i` descending by inline column `i`; 0 otherwise |
| 68 | 4 | `group` | the bucket group, 0 to 2^18 − 1; `ABSENT` for a tile not of one group |
| 72 | 8 | `built` | the position of the round at whose boundary the tile was built: a rewrite's deciding boundary (section 16.3), a checkpoint's position for the manifest and the free set |
| 80 | 8 | `previous` | the `built` of the tile this one replaced: a rewrite's first input; for a manifest leaf the leaf it replaces, for the root and the free set the previous checkpoint's; 0 for a tile replacing none |
| 88 | 8 | `horizon` | the fold horizon at `built` |
| 96 | 4 | `entries` | the tile's entries (sections 7 to 10 define an entry per shape) |
| 100 | 4 | `rows` | an adjacency block's rows; a column chunk's base slot; 0 otherwise |
| 104 | 16 | `digest` | the tile's digest, XXH3-128 storage (section 11.1) |
| 120 | 4 | `span` | a column chunk's slots, a power of two at most `chunk_slots`; 0 otherwise |
| 124 | 1 | `tokenizer` | an index block's (kind 4) tokenizer identifier byte; 0 otherwise |
| 125 | 3 | zero | |

A tile moved by the cleaner or by a bucket group's move (sections 16.6, 16.8) keeps every byte, its header included.

### 6.3 The directory

| Entry offset | Width | Field | Value |
|---|---|---|---|
| 0 | 2 | `id` | the section's identifier in its shape |
| 2 | 2 | `encoding` | 1 FOR, 2 DELTA, 3 BITMAP, 4 BYTES, 5 FIXED, 6 FILTER |
| 4 | 4 | zero | |
| 8 | 4 | `offset` | from the tile's first byte |
| 12 | 4 | `length` | the section's bytes, a multiple of 8 |

Entries are in ascending `id` order, each `id` at most once. Section 0 of the directory starts at `128 + 16 × sections`;
each later one at the end of the one before; the tile ends where the last ends. A shape's table states each section's
`id`, the encodings it may take, its count, whether it is present, and the bounds of its values; a decoder refuses a
section a shape does not list (`Section`), a missing required section (`Section`), an encoding the row does not allow
(`Encoding`), a count other than the row's (`Count`), and a value past its bound (`Range`).

### 6.4 A reference

| Offset | Width | Field | Value |
|---|---|---|---|
| 0 | 8 | `address` | the tile's first byte in the grid, a multiple of 4,096, in extents 1 to `grid_extents − 1` |
| 8 | 4 | `length` | the tile's `length` |
| 12 | 4 | zero | |
| 16 | 8 | `checksum` | the tile's `checksum` |

A reader reads `length` bytes at `address`, checks the tile's own checksum, and compares it with the reference's: a
tile whose checksum holds but differs from the reference's is stale or misdirected (`Misdirected`).

### 6.5 Reading a tile

A decoder checks, in order: `magic` (`Magic`); `format` (`Format`); `length` at least `128 + 16 × sections`, at most
`tile_bytes_max`, equal to the reference's (`Length`); `checksum` (`Checksum`); the reference's checksum
(`Misdirected`); `shape` and `kind` (`Shape`); `flags` (`Flags`); the header's zero bytes and fields its shape leaves 0
(`Padding`); the directory as section 6.3 states; every section's own layout as section 4 states; then its shape's rules.
Nothing of a tile is used before every check has passed.

## 7. Adjacency blocks

An adjacency block holds a slice of one relation's sparse matrix in one graph, one direction and one bucket group:
rows, and in each row its entries.

### 7.1 Kinds

| Kind | Holds | Row key | Neighbour |
|---|---|---|---|
| 1 | Forward edges whose objects have system IDs: entities, IRIs and blank nodes | the subject's slot | the object's system ID |
| 2 | Forward values: a generic-layout predicate's literal objects, or a compiled set-valued attribute's elements | the subject's slot | a literal's term encoding (generic) or an element's sortable value (compiled) |
| 3 | Reverse entries: the partition's own subjects of an object | the object's system ID, or a literal's term encoding in the generic layout | the subject's slot |
| 4 | An index relation: the subjects holding a token | a token | the subject's slot |
| 5 | A version run of a `@bitemporal` attribute or role (section 14.3) | the instance's slot | the revision's asserted start, the commit version of the transaction that wrote it (`SCHEMA.md` section 12.4), as a big-endian u64, then the value's sortable encoding |

A generic-layout predicate whose objects are both IRIs and literals has a family of each kind.

### 7.2 An entry

An entry is a row key, a neighbour, a statement slot or `ABSENT`, the values of the block's inline columns, and, when
the block is `versioned`, a created version and a deleted version (0 for live). No two entries of a block agree in
row key, neighbour, statement and created version.

### 7.3 Sections

`r` is the block's rows, `n` its entries.

| `id` | Name | Encodings | Count | Present | Holds |
|---|---|---|---|---|---|
| 1 | `rows` | FOR or DELTA for slot rows (kinds 1, 2, 5); BYTES for every other row key (kinds 3, 4) | `r` | always | each row's key, ascending, all distinct; a slot below 2^32 − 1 |
| 2 | `ends` | FOR or DELTA | `r` | always | each row's entries' end, cumulative: row `i` holds entries `ends[i−1]` (0 for the first) to `ends[i]`; strictly increasing, the last `n` |
| 3 | `prefix` | FOR or DELTA | `n` | kind 1 | each neighbour's type prefix, below 2^16 |
| 4 | `sequence` | FOR or DELTA | `n` | kind 1 | each neighbour's sequence |
| 5 | `bucket` | FOR or DELTA | `n` | kind 1 | each neighbour's bucket, below 2^24 (the reserved bytes are zero and not stored) |
| 6 | `slots` | FOR or DELTA | `n` | kinds 3, 4 | each neighbour slot, below 2^32 − 1 |
| 7 | `values` | BYTES | `n` | kinds 2, 5 | each neighbour's bytes |
| 8 | `statements` | FOR or DELTA | `n` | when any entry has a statement | each statement slot, `ABSENT` for none |
| 9 | `created` | FOR or DELTA | `n` | `versioned` | each created version |
| 10 | `deleted` | FOR or DELTA | `n` | `versioned` and not `class_i` | each deleted version, 0 for live; above its created version when nonzero |
| 11 | `filter` | FILTER | | `filter_bits_per_key` above 0 | every entry's row key's canonical bytes followed by its neighbour's |
| 16 + 2c | `inline` c | FIXED | `n` | inline column `c` exists | column `c`'s value per entry, zero bytes where absent |
| 17 + 2c | `valid` c | BITMAP | `n` | inline column `c` exists | bit set where column `c` holds a value |

Inline columns are numbered from 0 in the order the layout lists them (section 13.4), at most 8, each small (section
5.2). A block's neighbour key is the system ID composed from sections 3 to 5 (prefix, sequence, bucket, three zero
bytes), a slot, or the bytes of section 7.

### 7.4 Order

Rows ascend by their canonical keys. Within a row, entries are ordered by, in turn: with `order` `1 + i`, column `i`'s
bytes ascending, and with `0x81 + i` descending; then the neighbour's canonical key ascending; then the statement slot
ascending, `ABSENT` last; then the created version ascending. A column that orders a row is required (`SCHEMA.md`
section 5.2), so every entry holds it. A decoder refuses any other order (`Order`).

### 7.5 What a block's entries cover

A block covers the entries from its fence, its first entry's (row key, neighbour), up to the next block's fence of the
same family (section 13.2). A row may span blocks: a hub's row is cut between entries, never inside one.

## 8. Column chunks

A column chunk holds one attribute of one type in one graph and one bucket group for `span` slots from its base, `rows`
in the header, a multiple of `span`.

### 8.1 Kinds and sections

| Kind | Values |
|---|---|
| 1 | fixed width, the attribute's sortable encoding |
| 2 | variable width, the attribute's sortable encoding |
| 3 | the identity column: per slot the entity's sequence as a big-endian u64 and its bucket as a big-endian u24, 11 bytes; `subject` zero |

An entry is a folded value, one per slot holding one at the horizon, or an overlay record: a slot, a version, and a
value or its absence from that version on.

| `id` | Name | Encodings | Count | Present | Holds |
|---|---|---|---|---|---|
| 1 | `present` | BITMAP | `span` | always | bit `i` set when slot `base + i` holds a folded value |
| 2 | `values` | FIXED (kinds 1, 3) or BYTES (kind 2) | the bits set in `present` | always | the folded values in slot order |
| 3 | `overlay_slots` | FOR or DELTA | `o` | `versioned` | each record's slot offset from `base`, below `span` |
| 4 | `overlay_versions` | FOR or DELTA | `o` | `versioned` | each record's version, above `horizon` |
| 5 | `overlay_present` | BITMAP | `o` | `versioned` | bit set where the record holds a value |
| 6 | `overlay_values` | FIXED or BYTES, as `values` | the bits set in `overlay_present` | `versioned` | the records' values |

Overlay records ascend by (slot, version), no two equal. At version `v` at or above `horizon`, slot `s` holds the value
of its latest overlay record with a version at or below `v`, or nothing when that record holds no value; with no such
record, its folded value, or nothing when its `present` bit is clear. `entries` counts folded values and overlay
records together.

### 8.2 Which chunk holds a slot

The chunks of one family cover disjoint slot ranges, each `[base, base + span)` with `base` a multiple of `span`. A
change to slot `s` belongs to the chunk whose range holds `s`; when none does, to a new chunk of `span` `chunk_slots`
and base `s − (s mod chunk_slots)`.

### 8.3 Halving

A chunk whose encoding exceeds `tile_bytes_max` is replaced by the two chunks of half its span, the lower first, each
holding the entries of its range, and each halved again while it exceeds; section 3.3's bound on values makes a span of
1 always fit.

## 9. Record runs

### 9.1 Layout

A record run is a sorted run of records, each a key and a value, both byte strings. `group` names the run's bucket
group, or `ABSENT` for a run of the whole replica.

| `id` | Name | Encodings | Count | Present | Holds |
|---|---|---|---|---|---|
| 1 | `keys` | BYTES | `n` | always | the keys |
| 2 | `values` | BYTES | `n` | always | the values |
| 3 | `created` | FOR or DELTA | `n` | `versioned` | each record's created version |
| 4 | `deleted` | FOR or DELTA | `n` | `versioned` and not `class_i` | each record's deleted version, 0 for live |
| 5 | `filter` | FILTER | | `filter_bits_per_key` above 0 | every key |

Records ascend by key, then by created version; a run that is not `versioned` holds each key once. At version `v`, a
key holds the value of its record visible at `v` (section 14.4), of which there is at most one.

### 9.2 Classes and scope

Kinds 1 to 3, 11, 13, 16 and 17 are class I and never versioned beyond their created version; the others are class M.
The header names the run's scope: the table gives each kind's `tenant`, `graph` and `group`; a field the table leaves
out is zero.

| Kinds | `tenant` | `graph` | `group` |
|---|---|---|---|
| 1, 2, 4, 6, 7, 8, 11, 12, 13, 18 | the tenant | zero | the bucket group of the first system ID the record names, in its key or, for kind 1, its value; for kind 11 the group whose slots it maps |
| 3 | the tenant | zero | `ABSENT` |
| 5, 9 | the tenant | the graph | `ABSENT` |
| 10 | 0 | zero | `ABSENT` |
| 14, 15 | the tenant | zero | `ABSENT` |
| 16, 17 | 0 | zero | `ABSENT` |

A bucket group's move (section 16.8) also sends the records of kinds 3 and 5 its tiles' terms and values name.

### 9.3 Kinds

Inside keys and values, an ID is its 16 bytes and a term encoding is section 5.1's. A term's *name bytes* are its kind
(1 an IRI, 2 a blank node's skolem IRI, 3 a reifier's name, 4 a handle IRI) followed by its UTF-8 text.

| Kind | Run of | Key | Value |
|---|---|---|---|
| 1 | Term to ID | the XXH3-64 of the term's name bytes as a u64, then the name bytes | the system ID |
| 2 | ID to term | the system ID | the term's name bytes |
| 3 | Hash to text | the 16-byte hash a term encoding holds (section 5.1) | the text |
| 4 | Key claim | the type prefix as a u16, then the key's canonical encoding (`SCHEMA.md` section 7.2) | the instance's system ID |
| 5 | `@unique` claim | the predicate's system ID, then the value's sortable encoding | the instance's system ID |
| 6 | Mark | the entity's system ID, then the referring bucket group as 3 bytes | the generation as a u64, big-endian |
| 7 | Referrer set | the entity's system ID | the members in group order, each the group as 3 big-endian bytes, the epoch it was added at and its generation as big-endian u64s |
| 8 | Tombstone | the entity's system ID | the delete's version as a u64 |
| 9 | Statistics | the predicate's system ID | the statistics' format of section 9.4 |
| 10 | Session | the session's 16 bytes, then the share as a u32 (0 for a request that is not an Ingest share) | the request as a u64, the commit version as a u64, the round's position as a u64, the reply's length as a u32, four zero bytes, then the stored reply's body as `PROTOCOL.md` section 6.3 frames it |
| 11 | Slot map | the sequence as a u64 | the slot as a u32 |
| 12 | `@set` combination | the relation's type prefix as a u16, then the participants' system IDs in role order (a `@symmetric` relation's in ID order) | the instance's system ID |
| 13 | Idempotency claim | the type prefix as a u16, then the `Id128` as 16 big-endian bytes | the instance's system ID |
| 14 | Drop marker | the drop's version as a u64 | the operation as a u16 (`PROTOCOL.md` section 12.2, operations 1 to 4), six zero bytes, then its 16-byte argument |
| 15 | Build | the build's starting position as a u64 | the catalog version as a u64, the predicate's system ID, then the last key the backfill reached, its canonical bytes |
| 16 | Manifest leaf | a tile key (section 13.1) | the tile's reference, then its `entries` as a u32 and four zero bytes |
| 17 | Manifest root | the first tile key of a leaf | the leaf's reference |
| 18 | Statement record | the statement's system ID | the quad, the graph, subject and predicate as system IDs and the object's term encoding, then its properties as a BYTES encoding (section 4.3) of entries each a predicate's system ID followed by a term encoding |

A slot map and an identity column are per (bucket group, type): `group` and `type` in the header name them.

### 9.4 Statistics

A statistics value is: the byte `1`; the statement count as a big-endian i64; the count of kinds as a big-endian u16;
per kind, ascending by its name's bytes, the name's length as a big-endian u16, the name as UTF-8 (`iri`, `blank`, or a
literal's datatype IRI) and its count as a big-endian i64, no count zero; then the 64 subject registers and the 64 object
registers, a byte each. A value `x` raises register `XXH3-64(x) mod 64` to
`max(register, trailing_zeros((XXH3-64(x) >> 6) | 2^58) + 1)`; a subject's value is its system ID, an object's its
system ID or, for a literal, its term encoding (section 5.1). One record holds a predicate's statistics in one graph at
one round (section 15.4). Merging statistics adds the statement counts, adds each kind's count, drops kinds whose count
becomes zero, and takes each register's maximum.

## 10. The free set

The free set is a tile of shape 4, kind 1, `tenant`, `graph` and `subject` zero and `group` `ABSENT`, with one
section, `id` 1, a BITMAP of `grid_extents` bits: bit `e` set when extent `e` is allocated (section 12.4). Bit 0 is
always set. `entries` is the count of bits set.

## 11. Digests and the checkpoint identity

### 11.1 A tile's digest

An entry's hash is the XXH3-128 of its digest bytes; a tile's digest is the wrapping u128 sum of its entries' hashes,
so it depends on its entries and neither their order nor their encoding.

| Shape | An entry's digest bytes |
|---|---|
| Adjacency block | the row key's canonical bytes, the neighbour's canonical bytes, the statement slot as a big-endian u32 (`ABSENT` for none), then per inline column in order a byte 0 when absent or 1 followed by the value |
| Column chunk, folded value | the slot (the base plus its offset) as a big-endian u32, then the value |
| Column chunk, overlay record | the slot (the base plus its offset) as a big-endian u32, the version as a big-endian u64, a byte 0 when the record holds no value or 1 followed by the value |
| Record run | the key's length as a big-endian u32, the key, then the value |
| Free set | the extent's number as a big-endian u32, one entry per bit set |

Versions are no part of an adjacency entry's or a record's digest bytes, so folding them away (section 14.2) changes no
digest.

### 11.2 Conservation

Compaction rewriting tiles `T1 … Tk` with delta changes into tiles `U1 … Um` keeps two lists while it merges (section
16.3): the entries *added*, every entry an output holds that no input tile holds, and the entries *removed*, every entry
an input tile holds that no output holds. An entry is identified by its digest bytes (section 11.1), so folding an
adjacency entry's versions neither adds nor removes it, while folding a column chunk's overlay record into a folded
value removes the record and the folded value it replaces and adds the new folded value. Before writing any output it
SHALL find `Σ digest(Ui) = Σ digest(Tj) + Σ hash(added) − Σ hash(removed)`, wrapping, with each `digest(Ui)` computed
from the encoded output; a difference is the invariant `DigestConserved` (section 19.3).

### 11.3 The checkpoint identity

A checkpoint's identity is the XXH3-128 of 50 bytes: the superblock's `format` as a u16, then as u64s its `derivation`,
`position`, `horizon`, `watermark`, the manifest root's `checksum` and the free set's `checksum`. It holds nothing a
replica's own history sets, its `sequence` and `parent` among them, so two replicas' checkpoints at one position have
one identity (section 2), which stage 8 carries on `Prepare`, `PrepareOk` and the commit heartbeat (T29).

## 12. The grid

### 12.1 The file

A replica's grid is the file `grid` in its data directory, `<data_dir>/p{partition}/r{replica}/`, of exactly
`extent_bytes × grid_extents` bytes. It is created whole at first open (section 17.3), with `fallocate` where the file
system supports it, and never changes size. On Linux it is opened with `O_DIRECT`; every read and write is a whole
number of blocks at a block boundary.

### 12.2 Extent 0

Extent 0 holds the superblock's four copies in blocks 0 to 3; its other blocks are zero and never written. Extents 1 to
`grid_extents − 1` hold tiles.

### 12.3 The superblock

| Offset | Width | Field | Value |
|---|---|---|---|
| 0 | 8 | `magic` | the bytes `55 47 44 42 47 52 49 44` ("UGDBGRID") |
| 8 | 2 | `format` | 1 |
| 10 | 1 | `copy` | 0 to 3, the block it is written to |
| 11 | 5 | zero | |
| 16 | 8 | `checksum` | XXH3-64 of the 4,096 bytes with `copy` and `checksum` zeroed, so the four copies hold one checksum |
| 24 | 8 | `parent` | the previous checkpoint's `checksum`; 0 for the first |
| 32 | 8 | `sequence` | 1 for the grid's first checkpoint, one more for each after |
| 40 | 8 | `cluster` | the cluster's identifier |
| 48 | 4 | `partition` | the partition, from 1 |
| 52 | 4 | `replica` | the replica's number, from 1 |
| 56 | 8 | `derivation` | the derivation version (section 3.2) |
| 64 | 8 | `position` | the last round the checkpoint holds; 0 for none |
| 72 | 8 | `horizon` | the fold horizon at that round |
| 80 | 8 | `watermark` | the commit version of the checkpoint's last committed entry; 0 for none |
| 88 | 4 | `extent_bytes` | the setting |
| 92 | 4 | `grid_extents` | the setting |
| 96 | 24 | `manifest` | the manifest root's reference |
| 120 | 24 | `free_set` | the free set's reference |
| 144 | 8 | `mapping` | the mapping contract's version (`mapping-contract` capability) the replica's terms follow |
| 152 | 16 | `identity` | the checkpoint identity (section 11.3) |
| 168 | 3,928 | zero | |

The superblock is authoritative (T32): the WAL's own superblock records only its segments.

### 12.4 Allocation, release and reuse

**Extents.** At every moment an extent is *free*, *open* in one of three streams, *closed*, or *released*. Extent 0
is never any of them. The free set written at a checkpoint has bit `e` set for every extent that is not free at that
checkpoint.

**Streams.** Stream 0 holds manifest tiles and the free set, stream 1 hot data tiles, stream 2 cold data tiles. A data
tile goes to stream 2 when `previous` is nonzero and `built − previous ≥ cold_after_rounds`, otherwise to stream 1. A
tile the cleaner moves goes to stream 2.

**Placing a tile** of `length` bytes, needing `b = blocks(length)` blocks, in stream `s`: when `s` has an open extent
whose cursor plus `b` is at most `extent_bytes / 4,096`, the tile is placed at the cursor and the cursor advances by
`b`. Otherwise the open extent, if any, becomes closed, and the lowest-numbered free extent opens in `s` with its cursor
at 0, and the tile is placed there. With no free extent the store is full (`GridFull`, section 19.3). Tiles are placed
in the order section 16.5 gives, so placement is deterministic.

**Live blocks.** Each closed or open extent counts the blocks of the live tiles in it: the data tiles the directory
references, and the manifest leaves, root and free set of the latest checkpoint. A tile stops being live when the
directory drops it, or, for a stream 0 tile, when a later checkpoint replaces it.

**Release and reuse.** A closed extent whose live block count reaches 0 during the interval between checkpoints `N` and
`N + 1` becomes *released*. Checkpoint `N + 1`'s free set still marks it allocated, so recovery from checkpoint `N`,
which may reference its tiles, finds them intact; it becomes free once checkpoint `N + 1` is durable (section 17.1, step
6). Its blocks are then punched (`fallocate` with `FALLOC_FL_PUNCH_HOLE | FALLOC_FL_KEEP_SIZE`) where the file system
supports it; where it does not, the bytes stay and are never read again. An open extent is never released; every
checkpoint closes every open extent (section 17.1).

**Free space.** `free` is the count of free and released extents. At a round boundary with `free × 100 <
cleaner_free_percent × grid_extents`, the cleaner is due (section 16.6). A placement that finds no free extent cannot
occur while admission holds (section 15.6): admission also stops while fewer than 8 extents are free, and the partition
refuses commits with `GridFull` until the cleaner or a checkpoint frees one.

## 13. The manifest and the directory

### 13.1 Tile keys

A tile key orders and identifies a live tile: the bytes `shape` (u8), `kind` (u8), `tenant` (u64 big-endian),
`graph` (16), `subject` (16), `type` (u16 big-endian), `direction` (u8), `class` (u8: 1 when `flags` holds `class_i`,
else 0), `tokenizer` (u8), `group` (u32 big-endian), then its fence: an adjacency block's first row key followed by its
first entry's neighbour, each canonical and joined as section 5.4 states; a column chunk's base slot as a big-endian
u32; a record run's first key as `esc` of it; nothing for the free set. A *family* is the tiles sharing every byte but
the fence; the 51 bytes before the fence are its family key.

### 13.2 The directory

The directory is the in-memory map from tile key to reference and entries of every live data tile, rebuilt at open from
the manifest. An entry of an adjacency block or a record run, its (row key, neighbour) or its key joined as a fence is,
belongs to the tile of its family with the greatest fence at or below it, or to the family's first tile when it
precedes every fence; a family with no tile holds nothing, and its first entry makes a new tile. A column chunk's slot
belongs to its chunk as section 8.2 states.

### 13.3 The manifest

The manifest is the directory on disk, in two levels of record runs, written by every checkpoint for the leaves that
changed. Its leaves are runs of kind 16, the directory's entries in tile-key order; its root is one run of kind 17
holding each leaf's first key and reference. Leaves are cut in tile-key order, greedily: a record joins the current leaf
unless the leaf would then hold more than `tile_entries_max` records or encode to more than `tile_bytes_max` bytes, in
which case it starts the next leaf. At a checkpoint the leaves are cut afresh from the whole directory; a leaf equal,
byte for byte apart from `built`, `previous` and `horizon`, to a leaf of the previous checkpoint at the same position
in the order keeps that leaf's reference and is not written; the root is always written. A root that would exceed
`tile_bytes_max` stops admission (`GridFull`): at the defaults it holds about 600 leaves of about 600 tiles each.

### 13.4 The layout a tile follows

Which families exist, and for each the kind, direction, inline columns, order and class, comes from the layout of the
tenant's catalog version (section 21): the generic layout, for a tenant with no schema, has for every (graph, predicate)
a forward family of kind 1 for entity objects, of kind 2 for other terms, a reverse family of kind 3, the index
families its tokenizers declare, and no inline column; a governed tenant's layout is compiled once by the schema
compiler, as the catalog version's entries, and decoded, never recompiled, by every replica (T35).

## 14. Versions, the fold horizon and the delta

### 14.1 The delta

The delta is the replica's in-memory record of every committed change not yet in a tile. It is a set of delta rows,
each the changes to one row of one family, and of overlay changes to column chunks and records. Each change is an entry
of its tile shape with an operation, insert or delete, and the commit version of its transaction. Its capacity is
`delta_entries_max` entries, counted from a pool allocated at startup; every change counts one entry. A row's changes
are in the order apply appended them, which is commit order, so every delta row is sorted by version.

### 14.2 The fold horizon

The primary of a partition stamps the fold horizon into every round (`PROTOCOL.md` section 12.1, `horizon`):

```
H = min( the read version of its oldest open read-write transaction,
         the version of its oldest pinned snapshot,
         R )
R = the version bound of the latest epoch e whose close time ≤ closeTime(round's epoch − 1) − retention_ms,
    or 0 when there is none
H = max(H, the horizon of the round before)
```

Close times are the sequencer's logged wall-clock times (`sequencer` capability), so `R` is computed from the log; the
first two terms are the primary's, and stage 5.5's single replica is its own primary. Every replica, and replay, folds at
the logged value, never a local one: a pin on a follower keeps nothing on disk, and a read below the latest logged
horizon answers `TooOld` (section 18.6).

### 14.3 Storage classes

| Class | Holds | Versions in tiles |
|---|---|---|
| I | Instances of `@append_only` types with their attributes and keys; `@bitemporal` content as version runs (kind 5); the dictionary and the claims of class I instances | the created version only, while above `H`; a class I entry is deleted only by a purge (section 15.5) |
| M | Everything else, an `@immutable` attribute of a deletable instance among it | created and deleted versions while either is above `H` |

The layout gives each family its class, and a predicate shared by types of both classes is two families (T36).

### 14.4 Visibility

At version `v`, an entry is visible when its created version is at or below `v` and its deleted version is 0 or above
`v`. An entry of a tile that is not `versioned` was created at or below the tile's `horizon` and is live; written into
a `versioned` tile, such an entry takes created version 0, visible at every version a read may take.

A delta insert is an entry. A delta delete names an entry by its row key, neighbour and statement slot (a column
record by its slot, a record by its key) and sets, as seen by a reader at or above the delete's version, the deleted
version of the one entry with those values visible at the version before it, in the delta or in a tile. Execution never
logs an insert of an entry already visible, nor a delete of an entry not visible, so each names exactly one entry. A
delete of a statement without a statement ID (`PROTOCOL.md` section 12.2, an empty value in section 1) deletes every
visible entry of its row key and neighbour, whatever their statement slots.

### 14.5 Folding

When compaction writes an entry with created version at or below `H`, the entry is written without versions if every
entry of its output tile qualifies; otherwise the tile is `versioned` and the entry keeps its created version. An entry
deleted at or below `H` is removed, its hash counted as removed (section 11.2). A tile is `versioned` exactly when at
least one of its entries has a created version above `H` or, in class M, a nonzero deleted version.

### 14.6 Slots

A slot is given to an entity of the replica's own buckets, per (bucket group of its bucket, type of its prefix), the
next after the highest given, from 0, the first time a committed record names it as a subject or as an instance: in
round order, then in the round's record order, then in the order of the record's sections 0 to 2 and their entries.
Slots are never reused, and a deleted entity's slot stays as a hole. An entity of another partition takes no slot: it
is named by its system ID. The slot map (kind 11) maps the
entity's sequence to its slot, the identity column (column kind 3) maps back. A slot never leaves the replica: rounds,
frames and the WAL name system IDs. A bucket group moved in from another partition keeps its slot maps and identity
columns unchanged; its tiles keep their bytes (section 16.8).

## 15. Apply

### 15.1 What apply reads

Apply turns a committed round into delta changes. It reads the round and the delta and nothing else: never a tile.
Every derivation that needs a tile, an absence proof, a duplicate check for statistics, a count's degree, a referrer
derivation, runs during execution, which may wait for tiles, and its result is in the round's record (T33).

### 15.2 A statement inserted

For a statement `(g, s, p, o)` with statement ID `t` or none, committed at version `v`, in the subject's partition:

| Change | Family | Row key | Neighbour |
|---|---|---|---|
| forward insert | (g, p, forward) of kind 1 when `o` is an entity, else kind 2 | `s`'s slot | `o`'s system ID, or `o`'s term (generic) or value (compiled) |
| reverse insert | (g, p, reverse), kind 3 | `o`'s system ID or term | `s`'s slot |
| index inserts | each index family the layout declares for `p` | each token of `o` | `s`'s slot |

each carrying `t`'s slot or `ABSENT`, the inline values the layout places, and version `v`. A compiled attribute of a
single value is a column overlay record `(slot, v, value)`; a `@bitemporal` revision appends to its version run
(kind 5); a role declared `-> _` keeps its reverse change, joining no referrer set. A deleted statement is the same
changes as deletes.

### 15.3 Records

Dictionary entries the round creates, claims, marks, referrer members, tombstones, statement records, session entries,
drop markers and build records are record changes of their kinds at version `v`. A slot given (section 14.6) is a slot
map record and an identity overlay record.

### 15.4 Statistics

At the end of each round, apply writes one statistics record (kind 9) per (graph, predicate) the round changed, created
at the round's last commit version, holding the predicate's statistics after the round: those of its latest record, or
none, merged (section 9.4) with the round's inserts, each observed in the registers, and its deletes, each lowering the
counts only. The same change deletes the predicate's previous record at that version, so one record is visible at any
version. A read
at a version inside a round, which only a transaction beginning at a version it names can take, adds that round's
versioned entries up to its version.

### 15.5 Purge, drop and build

A purge's round deletes every entry of the instances it names at every version: their changes are deletes whose version
is the purge's, applied to class I entries too, and compaction removes them. A drop writes a drop marker (kind 14);
reads at every version treat entries the drop covers as absent, and compaction removes them; a graph drop removes the
graph's families from the directory at the round. A build record (kind 15) starts an index build of the generic layout
at its position (section 16.7).

### 15.6 Admission

At a round boundary where the delta holds `delta_watermark_percent` of `delta_entries_max` or more, the partition admits
no new commit until compaction brings it below; a commit waiting past its deadline is refused `Busy`. A round that would
take the delta past `delta_entries_max` cannot be built, since admission stopped below it with every round bounded by the
log's round bound.

## 16. Compaction

### 16.1 When a tile is due

At every round boundary, after apply, the engine examines every family with delta changes. Each delta change belongs to
one tile by section 13.2, or to its family's new tile; per tile, `d` is the changes belonging to it, `b` its entries, and
a *delta row* the changes of one row key belonging to it. A tile is due when any of:

1. one of its delta rows holds `delta_row_entries_max` changes or more;
2. `d × 100 ≥ theta_percent × max(b, 1)`;
3. its oldest delta change was applied `delta_age_rounds` rounds ago or more;
4. a drop marker or a purge covers any of its entries;
5. the delta holds `delta_watermark_percent` of `delta_entries_max` or more, and the tile is selected: tiles not
   already due are taken by `d` descending, then tile key ascending, until the delta less the selected tiles' `d` is
   below `delta_watermark_percent` of `delta_entries_max`.

A tile's reason is the first of these it meets in the order 1, 4, 5, 2, 3.

### 16.2 The queue

Due tiles join one queue, ordered by reason (1 first, then 4, then 5, then 2, then 3), then by `d` descending, then by
tile key ascending, evaluated at the boundary they become due. A tile already queued, or taken by a rewrite not yet
complete, is not queued again.

### 16.3 A rewrite

A rewrite starts at the boundary its tile leaves the queue's head, its *deciding boundary*. Its inputs are fixed there:
the tile, the delta changes belonging to it at that boundary, and the fold horizon `H` logged at that boundary's round;
changes applied later stay in the delta for a later rewrite. It also takes the next tile of the family, in tile-key
order, with that tile's delta changes, when the first tile holds fewer than `tile_entries_max / 4` entries and the two
tiles hold at most `tile_entries_max` entries together, both counted before the merge; the next tile then leaves the
queue. The family's last tile takes no next tile. A column chunk takes none. The rewrite:

1. merges the tiles' entries and the delta changes in the order of section 7.4 (or 8.1, or 9.1), applying each delete to the
   entry it names, and lists the entries added and removed (section 11.2);
2. folds (section 14.5) at `H`, removing entries deleted at or below it and entries a drop or a purge covers;
3. cuts the result into outputs, greedily, in order: a whole row joins the current output while the output then holds
   at most `tile_entries_max` entries; a row that does not fit starts the next output; a row of more than
   `tile_entries_max` entries starts the next output and is cut into pieces of exactly `tile_entries_max` entries, its
   last piece staying open for the rows after it. A record run is cut so, a row being one key with all its records; a
   column chunk is one output, halved as section 8.3 states;
4. encodes each output; an output longer than `tile_bytes_max`, of `n` entries, is cut in two at the row boundary `b`
   nearest `m = n div 2`, the lower `b` on a tie, or at entry `m` when it holds one row, and each half is encoded and
   cut again in the same way, the lower half first;
5. checks conservation (section 11.2), and decodes each output once, checking it as a reader would (section 6.5);
6. stamps `built` with the deciding boundary's position, `previous` with the first input tile's `built`, 0 for a new
   tile, and `horizon` with `H`;
7. completes at the boundary its units are spent (section 16.4): it places its outputs (section 12.4) in tile-key order,
   and the outputs replace the inputs in the directory at that boundary; the delta changes it merged are freed from the
   pool. A rewrite with no output removes its inputs and places nothing.

### 16.4 Pacing

Each applied round earns `beat_factor × c` units, `c` the delta changes it applied, every change of every kind counting
one. A rewrite costs its input tiles' entries, plus its delta changes, plus its outputs' entries. At each boundary the
beat spends the units held on the rewrites in progress and then on the queue's head, in queue order, a rewrite starting
at the boundary it receives its first unit and completing at the boundary its cost is paid; units left when the queue is
empty and no rewrite is in progress are discarded. While any rewrite of reason 5 is queued or in progress, every
rewrite in progress or queued ahead of it completes at that boundary, whatever the units. A rewrite completing at a
boundary whose input tiles are not yet read holds that boundary until they are: the next round is not applied before,
so no I/O timing changes what is written or where.

### 16.5 Placement order

Within one boundary, rewrites complete in queue order, and each places its outputs in tile-key order; the cleaner's
moves follow; the free set and manifest tiles of a checkpoint come last (section 17.1).

### 16.6 The cleaner

When due (section 12.4), the cleaner takes the closed extent with the fewest live blocks, the lowest-numbered among
equals, among those holding no tile a rewrite in progress takes as input and no stream 0 tile, and moves each of its
live tiles, in address order, to stream 2: the bytes unchanged, only the reference's address new. It moves one extent
per boundary, after the boundary's rewrites have placed their outputs; the extent becomes released (section 12.4).

### 16.7 Builds

A build of the generic layout's index family walks its source family in key order, `beat_factor × tile_entries_max`
entries per boundary, beside the compaction beat and drawing no unit from it, writing index entries as compaction
outputs created at the build's starting version; changes
committed meanwhile reach the index through apply. Its record (kind 15) carries the last key reached and is updated at
each checkpoint; its completion is a logged operation, so every replica finishes at one position. Governed tenants build
no index before stage 9.5.

### 16.8 Moving a bucket group

A move of a bucket group is a logged operation at a position of the source's log. At its round boundary the source makes
every family of the group due under reason 1, rewrites them before any other, and sends the group's live tiles, bytes
unchanged, with its slot maps and identity columns. The destination installs them at a position of its own log: it
places each in stream 2 in tile-key order and adds it to its directory; no tile's bytes change, since no header names a
partition.

## 17. Checkpoints, recovery and repair

### 17.1 When and how

Let `T(p)` be the total length of the Round frames at positions 1 to `p`. The first checkpoint is due at the first round
boundary `p` with `T(p) ≥ checkpoint_bytes`; after a checkpoint at `q`, the next is due at the first boundary `p` with
`T(p) ≥ (T(q) div checkpoint_bytes + 1) × checkpoint_bytes`, so a round crossing several multiples makes one
checkpoint. Checkpoint positions are a function of the log alone, so every replica checkpoints at the same positions. A clean shutdown syncs the log and writes no
checkpoint; the next open replays from the last one. A checkpoint, after the boundary's rewrites and cleaner moves:

1. closes every open extent;
2. cuts the manifest (section 13.3) and places, in stream 0, the leaves to be written, then the root, then the free set;
   then encodes the free set, which marks every extent not free after these placements, the extents released since the
   last checkpoint included;
3. syncs the grid after writing those tiles and every data tile placed since the last checkpoint;
4. writes the superblock's copies 0, 1, 2 and 3 in turn, syncing after each;
5. writes the WAL's superblock and trims the WAL through `position`;
6. frees the extents released between the previous checkpoint and this one (section 12.4), the previous checkpoint's
   stream 0 tiles' extents among them when no live tile remains, and punches them.

The crash order is the steps' order (T32): a crash before step 4 leaves the previous superblock in force; during step 4
leaves copies of two sequences, of which recovery takes the newer whose references verify; after step 4 the new
checkpoint is in force whatever the WAL's state. A replica that recovers from the previous checkpoint replays to the new
one's position and writes it again, the same bytes at the same addresses.

### 17.2 Recovery

At open, the store:

1. reads blocks 0 to 3 and keeps each copy whose `magic`, `format`, `copy` (its block's number) and `checksum` hold;
2. takes the highest `sequence` among them; two kept copies of one `sequence` that differ in any byte but `copy`
   refuse the open (`SuperblockDiverged`); with none kept, an all-zero extent 0 is a new grid (section 17.3) and
   anything else refuses the open (`SuperblockLost`); when the highest sequence's references fail their checks and a
   kept copy of the sequence below holds, recovery takes that one, since step 4 of section 17.1 had not completed;
3. checks `derivation`, `extent_bytes`, `grid_extents`, `cluster`, `partition` and `replica` against its settings and
   identity (`DerivationMismatch`, `Identity`), and `identity` against section 11.3;
4. reads the free set and the manifest, checking each tile as section 6.5 states, and rebuilds the directory, the live
   block counts (section 12.4) and the extents' states: an extent the free set marks and holding live blocks is closed;
   one it marks and holding none is free, as it is on a replica that did not crash (section 17.1, step 6); one it does
   not mark is free; no extent is open;
5. replays the log from `position + 1`, which rebuilds the delta and, compaction being deterministic, writes the tiles
   the replica wrote before the crash at the addresses it wrote them.

A tile failing its checks during step 4 is repaired (section 17.4) before the open continues; one that cannot be
repaired refuses the open naming its tile key and reference.

### 17.3 A new grid

A new grid is created whole and zeroed, extent 0 all zero; its first checkpoint, sequence 1, is written at the first
checkpoint boundary, with `position` and `watermark` 0 until a round is applied.

### 17.4 Repair

A replica that finds a tile failing its checks, at a read, at recovery or while scrubbing, asks a peer replica for the
block range its reference names, by address and checksum (stage 8), and accepts bytes whose checksum is the reference's;
replicas hold the same tiles at the same addresses (section 2). Before stage 8, or with no peer holding them, a tile
written after the last checkpoint is rebuilt by replaying the log; any other is unrepairable, and the replica stops
serving, naming the tile.

### 17.5 Scrubbing

A scrub reads every live tile once per cycle, in address order, `beat_factor × tile_entries_max` entries' worth of tiles
per boundary, checking each as a reader does and its digest against its entries.

## 18. Reads and lanes

### 18.1 Lanes

Every read request names a lane, which the frontend and the planner choose from the classification (`SCHEMA.md`
sections 9.2 and 9.6); the engine refuses a lane the catalog does not allow for the request's families (`Lane`).

| Lane | Answers at | Pins | Records a read set |
|---|---|---|---|
| L | the replica's latest applied state, within one turn, one frame per turn, each naming the version it read | no | no |
| S | a version `v` at or above the latest logged `H`: a closed epoch's bound or an as-of version; each turn of the request first checks that `v` is still at or above it, and answers `TooOld` for the rest of the request otherwise | `v`, until the request ends; on the primary the pin holds `H` (section 14.2), on a follower it holds nothing | no |
| X | the transaction's read version, with its own writes merged | the read version, until the transaction ends | yes, but stable reads (section 18.4) |

### 18.2 What a read sees

A read at version `v` of a key merges, in key order: the tile entries visible at `v`, the delta entries visible at `v`,
and, in lane X, the transaction's own writes. Lane L reads the latest applied state with no version filter but the
deleted versions'. A request whose tiles are not in the cache returns `Miss` with their references: its attempt is
undone, parked, and rerun from its request once they are read (T33).

### 18.3 Resume

A request that answers part of its result ends with a resume key: its family key (section 13.1), then the last
answered row key and neighbour joined as section 5.4 states, then the statement slot as a big-endian u32 (`ABSENT` for
none). The next request answers strictly after it, comparing as bytes. An entry
inserted behind the key since is not answered, which lanes L and S allow; lane X records a partial page's whole family
and bucket group as one conflict range.

### 18.4 Stable reads

In lane X the existence of a class I instance, and its attributes and key, are not recorded: only a purge changes them,
and a purge at version `v` aborts every open read-write transaction of its partition whose read version is below `v`. A
relation whose liveness derives from its participants' is not stable: lane L joins its participants' status and lane X
records it.

### 18.5 Frontier requests

A frontier request names one family, a set of row keys, an optional neighbour type range, optional inline-column
filters, and an optional top-k, and answers each row's surviving entries in the family's order: a type range is a range
of `prefix` values, a filter on an inline column skips a block whose column's values cannot pass it, and top-k reads a row
in its order and stops at k. Its wire form is `PROTOCOL.md`'s, which section 21 asks for.

### 18.6 Refusals

A read below the latest logged `H` answers `TooOld`; a lane L request that has gone back to loading twice is answered in
lane S at the replica's latest applied version at that moment; a request past its deadline ends with an error naming
the partition.

## 19. Faults, invariants and refusals

### 19.1 The fault model

Crashes at any instruction and power loss at any moment; writes torn at any 512-byte boundary; writes the drive
acknowledged and lost; writes and reads misdirected to another block; latent sector errors; bits flipped at rest; a
failed sync, after which nothing the sync covered is trusted; a punch the file system refuses.

### 19.2 Detection and response

| Fault | Detected by | Response |
|---|---|---|
| Torn, lost or flipped tile bytes | the tile's checksum | repair (section 17.4) |
| Misdirected or stale tile | the reference's checksum, the header's family and fence | repair |
| Torn superblock copy | the copy's checksum | the other copies (section 17.2) |
| A failed sync | its completion | the node stops; nothing is retried |
| A punch refused | its completion | the extent stays allocated on disk and free in the free set; nothing else changes |
| An I/O error reading or writing a tile | its completion | repair for a read; the node stops for a write |

### 19.3 Invariants

Each is checked where it applies, and a violation is an invariant error (Ultra Style rule 5): it stops the replica in a
release build and the caller in a debug build.

| Invariant | Holds |
|---|---|
| `DigestConserved` | Section 11.2, before any output is written |
| `RelationDigests` | Per (tenant, graph, predicate), the wrapping u128 sum of XXH3-128 over the bytes: the tenant as a big-endian u64, the graph's and the predicate's system IDs, the subject's system ID, the object's canonical key, the statement's system ID or 16 zero bytes; summed over its live forward entries, equals the same sum over its live reverse entries, after every round |
| `RoundTrip` | Every tile compaction writes decodes to its entries and re-encodes to its bytes |
| `HorizonMonotone` | The logged `H` never falls |
| `CheckpointIdentity` | A peer's identity for a checkpoint position this replica holds equals its own (stage 8) |
| `PlacementFound` | Placing a tile finds a free extent, which admission (sections 12.4, 15.6) guarantees |
| `PoolBound` | The delta never exceeds `delta_entries_max` |

### 19.4 Refusals of bytes

A decoder names the first rule a structure breaks, with the structure and the offset:

| Refusal | Rule |
|---|---|
| `Magic`, `Format`, `Shape`, `Flags` | Section 6.5 |
| `Length` | A length past its bound or other than its reference's |
| `Checksum` | A tile's or a superblock's checksum |
| `Misdirected` | A tile's checksum other than its reference's |
| `Padding` | A nonzero zero byte or padding byte |
| `Section` | A section missing, unknown or out of order |
| `Encoding` | An encoding a section does not allow |
| `Count` | A count other than the section's rule |
| `NotMinimal` | A packed width other than `bitlen` gives |
| `NotChosen` | FOR where DELTA is shorter, or DELTA where it is not |
| `Range` | A value past its section's bound |
| `Order` | Rows, entries, records or overlay records out of order, or two equal; `ends` that fall, or a `rows` or `ends` section a shape requires ascending that is not |
| `Copy` | A superblock copy whose `copy` is not the number of the block it was read from; the copy is not kept |
| `Parameter` | A FILTER's `k` or `bits` other than section 4.5's formulas |

Refusals of a whole structure are checked in the order section 6.5 gives for a tile and section 17.2 for a superblock;
within a section, in the order of its encoding's checks (section 4). A write a store refuses before it is logged is not a
refusal of bytes: `ValueTooLarge` (section 3.3), `GridFull` (section 12.4) and `Busy` (section 15.6) are refusals of a
commit.
| `Digest` | A header's digest other than its entries' |
| `SuperblockDiverged`, `SuperblockLost`, `DerivationMismatch`, `Identity` | Section 17.2 |

### 19.5 The simulator

The simulator runs the engine with its I/O performed by a model grid under every fault of section 19.1, from a seed, and
checks after every step:

- a shadow model, a map from every key to its entries with their versions, answers every read the engine answers, at
  every version at or above the logged `H`;
- every invariant of section 19.3;
- two engines fed the same rounds with different read loads, different cache sizes and different I/O completion orders
  hold byte-identical grids at every checkpoint, and so does a third that crashed and replayed;
- after every injected fault, the engine either repairs it, refuses with the refusal section 19.2 names, or stops;
  it never answers from a tile that failed a check.

A run is a function of its seed: the simulator's clock, random source and completion order come from it.

## 20. Decisions

| Question | Decision |
|---|---|
| General-purpose compression in format 1 | None: every encoding is specified to the bit, so two implementations write the same bytes and conformance compares bytes. LZ4 joins a later format, with its compressor's output specified, if E4 measures that it pays (the design note's T13 named LZ4 for strings and cold tiles) |
| One checksum per tile or per section | One per tile, in its header and its reference: a tile is read whole, so section checksums would add bytes and no detection |
| The header's size | 128 bytes, beside the design note's 64: the graph, the subject predicate, the fold horizon and the rewrite history need the room |
| Death time | From two positions, `built` and `previous`, beside the SSD paper's four: a tile's predecessor's lifetime decides its stream |
| A record run's versions | Records repeat their key per version, like an adjacency block's entries, so one merge serves every shape |
| The count index | An index relation (adjacency kind 4) with the token direction and count, not a record run |
| The default graph | 16 zero bytes in headers and tile keys |
| Values longer than a quarter of a tile | Refused in format 1 (`ValueTooLarge`, 16 KiB at the defaults) rather than kept in blob extents, so every row and slot fits a tile; blob extents join a later format if a workload asks for them |
| A checkpoint at a clean shutdown | None: checkpoint positions are a function of the log, so every replica's grid stays identical; a restart replays at most `checkpoint_bytes` of log |
| Joining a fence's components | Escaped (`esc`) where variable in width, so a fence compares as its components' tuple; a length prefix would not sort |

## 21. What this specification asks of the others

- **`PROTOCOL.md` section 12.1:** the Round frame gains `horizon`, a u64, the fold horizon of section 14.2, and its
  `derivation` becomes the derivation version of section 3.2, widened from a u32 to a u64; a Round section of Ingest
  bodies at 4 KiB frame offsets with its resolution section (T39); section 12.2's sections 6 to 8 name logical conflict
  keys (T38), its derived predicate-list entries and per-batch applied positions go, and its last paragraph's
  checkpoint becomes section 17's.
- **`PROTOCOL.md` section 1:** "a result term keeps the store's sortable value encoding" reads "the store's term
  encoding" (section 5.1 here), which orders for equality only.
- **`PROTOCOL.md` section 13:** the frontier request and its reply (section 18.5); lane L's frames naming their version;
  resume keys as section 18.3 gives them.
- **`CATALOG.md`:** the layout entries of section 13.4; tag 36, `Id128`; unit and quantity-kind numbers assigned at
  first declaration and never reused (section 5.2).
- **`SCHEMA.md` section 12:** restated as the logical obligations this file implements.
- **The config service:** the `[tiles]` table, its equality check across storage instances and its `fixed` rule.
- **`write-ahead-log`:** the WAL's superblock records only its segments; trimming follows section 17.1.

## 22. Conformance

`../conformance/storage` holds vectors written from this file alone by an independent generator,
`generate_vectors.py`, never from an implementation's output. A conforming implementation encodes each vector's input
to exactly its bytes, decodes the bytes back to its input, and refuses each refused vector with its refusal. The
directory's `README.md` states the vectors' format and what each family of vectors covers.

## 23. Sources

- `../../openspec/notes/UltraGraphDB Scalable Tile Store.md` and `../../openspec/notes/UltraGraphDB Tile Store Feasibility.md`.
- `../binary_protocol/PROTOCOL.md` sections 1, 10, 12 and 13; `../schema/SCHEMA.md` sections 5, 7, 9 and 12;
  `../schema/CATALOG.md` section 3.1.
- The xxHash specification, `doc/xxhash_spec.md` of github.com/Cyan4973/xxHash.
- The statistics codec of `ultragraphdb_store_quads` (`statistics_codec.rs`), whose format section 9.4 restates.
