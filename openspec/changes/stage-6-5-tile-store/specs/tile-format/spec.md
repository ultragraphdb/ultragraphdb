## ADDED Requirements

### Requirement: Every structure has one encoding
Every structure of the tile format SHALL have exactly one encoding of its content, as
`specification/storage/STORAGE.md` section 1 states: an encoder writes it, a decoder refuses every other byte sequence
with the refusal section 19.4 names, and decoding then re-encoding gives the bytes read. Every integer field of a grid
structure SHALL be little-endian and read byte by byte with no alignment assumed; a system ID, a canonical key, a
sortable value and every integer inside a key SHALL be big-endian, so they sort as bytes. Every padding, reserved and
unused byte SHALL be zero, and a decoder SHALL refuse a nonzero one with `Padding`. The format SHALL be held to the
vectors of `specification/conformance/storage/`, which are written from `STORAGE.md` alone and never from an
implementation's output.

#### Scenario: Every tile vector round-trips
- **WHEN** each `.tile` file that `vectors/tiles.json` and `vectors/kinds.json` name is decoded and its content
  re-encoded
- **THEN** the bytes equal the file, and the decoded content equals the vector's input

#### Scenario: A nonzero zero byte is refused
- **WHEN** the case `header zero byte` of `vectors/refusals.json` is decoded
- **THEN** it is refused with `Padding`, naming the tile and the offset

### Requirement: Packed integers take the shorter of FOR and DELTA
A vector of u64 values SHALL be written as FOR (code 1) or DELTA (code 2), fields packed least significant bit first
at the width `bitlen` gives, as `specification/storage/STORAGE.md` section 4.1 lays out; an empty FOR vector has width
0 and base 0. An encoder SHALL write DELTA exactly when the vector is nondecreasing, holds at least two values and
DELTA's length is strictly less than FOR's, and FOR otherwise. A decoder SHALL check, in order: `count` against the
section's rule (`Count`); the zero bytes and the packed bits past the last field (`Padding`); `width` (`NotMinimal`);
the choice (`NotChosen`); each value against the section's bound (`Range`).

#### Scenario: The packed-integer vectors
- **WHEN** each case of `vectors/encodings.json` of kind `ints` is encoded
- **THEN** it encodes to `hex` under the code `encoding` names, `for_hex` and `delta_hex` are the two candidates the
  choice rule compared, and `hex` decodes to `values`

#### Scenario: DELTA is chosen only when strictly shorter
- **WHEN** the cases `dense sorted run, DELTA width 0` and `sorted with varying gaps, DELTA width 3` are encoded
- **THEN** the first is DELTA of width 0, and the second is FOR, since its DELTA candidate is not strictly shorter

#### Scenario: A packed section's refusals
- **WHEN** the cases `FOR not minimal`, `FOR where DELTA is shorter`, `DELTA where FOR is shorter` and
  `FOR packed bits past the last field` of `vectors/refusals.json` are decoded
- **THEN** they are refused with `NotMinimal`, `NotChosen`, `NotChosen` and `Padding` respectively

### Requirement: BITMAP, BYTES and FIXED sections
A bit vector SHALL be a BITMAP (code 3), a vector of byte strings BYTES (code 4), and a vector of equal-width values
FIXED (code 5), each starting at a multiple of 8 from its tile's first byte with a length that is a multiple of 8, as
`specification/storage/STORAGE.md` sections 4.2 to 4.4 lay out. A FIXED width SHALL be 1 to `value_bytes_max`
(section 3.3). A decoder SHALL check a BITMAP's `count` (`Count`), then its zero bytes and the bits at or past `count`
(`Padding`); a BYTES section's `count` (`Count`), each end at or above the one before (`Order`), the last end equal to
`total`, 0 for no strings (`Count`), then the padding (`Padding`); a FIXED section's `count` (`Count`), its `width`
(`Encoding`), then the padding (`Padding`).

#### Scenario: The BITMAP, BYTES and FIXED vectors
- **WHEN** each case of `vectors/encodings.json` of kind `bitmap`, `bytes` or `fixed` is encoded
- **THEN** it encodes to `hex`, and `hex` decodes to the case's input

#### Scenario: A FIXED column as wide as the value bound
- **WHEN** the case `fixed width value_bytes_max, 16384 at the defaults` is encoded and decoded
- **THEN** both succeed, and the case `FIXED width past value_bytes_max, 16385 at the defaults` of
  `vectors/refusals.json` is refused with `Encoding`

#### Scenario: BITMAP and BYTES refusals
- **WHEN** the cases `BITMAP bit past count`, `BYTES ends falling` and `BYTES total other than the last end` of
  `vectors/refusals.json` are decoded
- **THEN** they are refused with `Padding`, `Order` and `Count` respectively

### Requirement: Filters follow their formulas
A Bloom filter over a set of byte-string keys SHALL be a FILTER section (code 6) with
`m = max(64, 64 × ceil(keys × b / 64))` bits and `k = min(16, max(1, (b × 693 + 500) div 1000))` probes for
`b = filter_bits_per_key`, each key setting bits `(h + j × g) mod m` with `h` its XXH3-64 and `g = rotl64(h, 32) | 1`,
as `specification/storage/STORAGE.md` section 4.5 states. A key MAY be present when all its `k` bits are set and SHALL
be reported absent otherwise. With `filter_bits_per_key` 0 a tile SHALL write no filter section and report every key as
possibly present. A decoder SHALL check `keys` against the count its section's rule gives (`Count`), `k` and `bits`
against the formulas (`Parameter`), then the zero bytes (`Padding`).

#### Scenario: The filter vectors
- **WHEN** each case of `vectors/encodings.json` of kind `filter` is built from its keys and `bits_per_key`
- **THEN** its `m`, `k` and bytes equal the case's `m`, `k` and `hex`, and every probe answers the case's
  `may_be_present`

#### Scenario: A filter with the wrong probe count is refused
- **WHEN** the case `FILTER k other than the formula` of `vectors/refusals.json` is decoded
- **THEN** it is refused with `Parameter`

#### Scenario: No filter at zero bits per key
- **WHEN** an adjacency block and a record run are encoded with `filter_bits_per_key` 0
- **THEN** the block holds no section of `id` 11, the run none of `id` 5, and every key probed may be present

### Requirement: The term encoding names terms by system ID
A term in the generic layout SHALL be stored in the store's term encoding, `PROTOCOL.md` section 10.1's tags and
payloads with tags `0x01` and `0x02` carrying the term's 16-byte system ID and tags `0x32` and `0x33` the datatype's
system ID in place of its IRI's hash, every other tag exactly as `PROTOCOL.md` gives it, as
`specification/storage/STORAGE.md` section 5.1 states. Equal terms SHALL encode equally; the encoding orders for
equality, not by value.

#### Scenario: The term vectors
- **WHEN** each case of `terms` in `vectors/sortable.json` is encoded
- **THEN** it encodes to `hex`, and `hex` decodes to the term

#### Scenario: An IRI is its system ID
- **WHEN** the case `an IRI` is encoded
- **THEN** the encoding is tag `0x01` followed by the IRI's system ID, and holds no hash of its text

### Requirement: Sortable values compare as their values
A value of a declared attribute in a compiled layout SHALL be stored in its tag's sortable encoding of
`specification/storage/STORAGE.md` section 5.2, which compares byte by byte as the values compare: big-endian integers
with the sign bit inverted for signed types, IEEE 754 floats with −0.0 mapped to +0.0 and every NaN to the quiet NaN
before the sign transform, temporal values of 18 bytes keeping their offset, `Text` as `esc` of its text then its
lowercased language tag, `Geo` as its RFC 8785 canonical form, and the remaining tags as the table gives. A value's
comparison form SHALL be its first 16 bytes for tags 17 to 20 and its whole encoding otherwise; order, equality,
claims and tokens SHALL compare it. A fixed-width value SHALL be small when its width is at most 18 bytes.

#### Scenario: The sortable vectors
- **WHEN** each case of `sortable` in `vectors/sortable.json` is encoded
- **THEN** it encodes to `hex`, its comparison form is `comparison_hex`, it is small exactly when `small` is true, and
  `hex` decodes to the value

#### Scenario: Orders are kept
- **WHEN** each list in `ascending` of `vectors/sortable.json` is encoded
- **THEN** its comparison forms ascend strictly, as bytes, in the list's order

#### Scenario: One instant at two offsets is one value
- **WHEN** the cases `DateTime 2026-11-10T12:00:00Z` and `DateTime 2026-11-10T13:00:00+01:00` are encoded
- **THEN** their comparison forms are equal, their stored encodings differ, and each decodes back to its own offset

#### Scenario: Negative zero is zero
- **WHEN** the cases `Float64 -0.0` and `Float64 0.0` are encoded
- **THEN** both have one comparison form, as the list in `equal` of `vectors/sortable.json` states

### Requirement: Tokens are exact and long tokens are hashed
A token of a `range` or `interval` index SHALL be the tokenizer's identifier byte followed by the comparison
form of the value's sortable encoding, exact, a `Measure<K>` value converted to its quantity kind's canonical unit as a
`Float64`'s sortable encoding, the exact rational value rounded once to the nearest `Float64`, ties to even, and a
generic-layout literal taking the sortable type its datatype picks; a `count` index's token SHALL be the identifier
byte, the direction as a u8 and the count as a big-endian `UInt64` (T48); any other token SHALL be the identifier byte and the token bytes its tokenizer gives, as
`specification/storage/STORAGE.md` section 5.3 states. A token of more than 256 bytes, its identifier byte included,
SHALL be stored as `0xFF` followed by the XXH3-128 of all its bytes as 16 big-endian bytes, and a reader SHALL confirm
every match of such a token against the value it indexes. No tokenizer's identifier byte SHALL be `0xFF`.

#### Scenario: The token vectors
- **WHEN** each case of `tokens` in `vectors/sortable.json` is built from its `identifier` and its value or body
- **THEN** it equals `hex`

#### Scenario: Two offsets of one instant give one range token
- **WHEN** the cases `range token of DateTime 2026-11-10T12:00:00Z` and `range token of DateTime
  2026-11-10T13:00:00+01:00, equal to the one above` are built
- **THEN** both tokens are the same 17 bytes

#### Scenario: The hashing bound
- **WHEN** the cases `token of exactly 256 bytes` and `token of 301 bytes` are built
- **THEN** the first is stored as its 256 bytes, and the second as 17 bytes starting with `0xFF`

### Requirement: Escaped components compare as tuples
`esc(x)` SHALL write every byte of `x`, each `0x00` as `0x00 0xFF`, then `0x00 0x00`, so a concatenation of escaped
components compares as the tuple of its components, as `specification/storage/STORAGE.md` section 1 defines it. A
canonical key SHALL be as section 5.4 gives it: a slot pair as 6 bytes, the type prefix as a big-endian u16 then the
slot as a big-endian u32; a system ID as its 16 bytes; a term as its term encoding; a token as its bytes; a record
run's key as its kind gives it. Where a row key and a neighbour are joined into one byte string, each component not of
fixed width SHALL be written as `esc` of it, and a slot pair and a system ID as they are.

#### Scenario: The esc vectors
- **WHEN** each case of `esc.cases` and `esc.single` in `vectors/sortable.json` is encoded
- **THEN** it equals `hex`, and the joined cases ordered as bytes follow `esc.ascending_indices`, as they do ordered
  as tuples

#### Scenario: A slot pair's canonical bytes
- **WHEN** the slot pair of type prefix 3 and slot 5 is written as a key
- **THEN** its bytes are `00 03 00 00 00 05`, and it sorts after every pair of type prefix 2

### Requirement: The tile header
A tile SHALL be a 128-byte header, a directory of 16 bytes per section and its sections back to back, at most
`tile_bytes_max` long, with the header's fields as `specification/storage/STORAGE.md` sections 6.1 and 6.2 lay them
out: `magic` `UGTL`, `format` 1, `shape` 1 to 4 with its `kind`, `checksum` the XXH3-64 of the tile's `length` bytes
with that field zeroed, `flags` holding only `versioned` (bit 0) and `class_i` (bit 1), and every field a shape leaves
0 at 0. A tile moved by the cleaner or a bucket group's move SHALL keep every byte, its header included.

#### Scenario: The tile vectors encode with every header field
- **WHEN** each tile of `vectors/tiles.json` is encoded from its input and every `header` field
- **THEN** it equals its `.tile` file, its length, `checksum` and `digest` are the vector's, and its decoded header
  equals `header`

#### Scenario: Header refusals
- **WHEN** the cases `magic`, `format 2`, `shape 9` and `flags bit 2` of `vectors/refusals.json` are decoded
- **THEN** they are refused with `Magic`, `Format`, `Shape` and `Flags` respectively

### Requirement: The section directory
The directory SHALL hold one entry per section in ascending `id` order, each `id` at most once, each with its
encoding code, its offset from the tile's first byte and its length, a multiple of 8; the first section SHALL start at
`128 + 16 × sections` and each later one at the end of the one before, and the tile SHALL end where the last ends, as
`specification/storage/STORAGE.md` section 6.3 lays out. A decoder SHALL refuse a section its shape does not list or
whose *Present* column excludes it for the tile's kind and flags or for the settings, a FILTER section while
`filter_bits_per_key` is 0 among them, and a missing required section (`Section`; T48); an
encoding its row does not allow (`Encoding`); a count other than its row's (`Count`); and a value past its bound
(`Range`).

#### Scenario: Directory refusals
- **WHEN** the cases `unknown section id 15`, `prefix section missing`, `rows as BYTES in kind 1` and
  `prefix count 5 of 6 entries` of `vectors/refusals.json` are decoded
- **THEN** they are refused with `Section`, `Section`, `Encoding` and `Count` respectively

#### Scenario: Sections out of order are refused
- **WHEN** a tile whose directory lists section 2 before section 1 is decoded
- **THEN** it is refused with `Section`

### Requirement: References and the order of a tile's checks
A reference SHALL be 24 bytes, the tile's address (a multiple of 4,096 in extents 1 to `grid_extents − 1`), its
`length`, four zero bytes and its `checksum`, as `specification/storage/STORAGE.md` section 6.4 lays out. A decoder
SHALL check, in order: `magic` (`Magic`); `format` (`Format`); `length` at least `128 + 16 × sections`, at most
`tile_bytes_max` and equal to the reference's (`Length`); `checksum` (`Checksum`); the reference's checksum
(`Misdirected`); `shape`, `kind` and the `direction` its kind gives (`Shape`); `flags` (`Flags`); the header's zero
bytes and the fields its shape leaves 0 (`Padding`); the directory; each section's encoding in section 4's order; then
its shape's rules; then, last, the digest (`Digest`), as section 6.5 and section 19.4 state (T48); it SHALL name the first rule broken, with the structure and
the offset, and SHALL use nothing of a tile before every check has passed.

#### Scenario: A tile whose checksum holds but is not the reference's
- **WHEN** the case `checksum holds, reference names another` of `vectors/refusals.json` is read through a reference
  whose checksum is `0000000000000001`
- **THEN** it is refused with `Misdirected`

#### Scenario: Length and checksum refusals
- **WHEN** the cases `length past tile_bytes_max` and `a flipped bit` of `vectors/refusals.json` are decoded
- **THEN** they are refused with `Length` and `Checksum` respectively

#### Scenario: The control case is accepted
- **WHEN** the case `valid control for the cases above` of `vectors/refusals.json` is decoded through its reference
- **THEN** it is accepted

#### Scenario: The first broken rule is named
- **WHEN** the control case is decoded with its `magic` changed and one bit of a section flipped
- **THEN** it is refused with `Magic`, not `Checksum`

### Requirement: Adjacency blocks of kinds 1 to 5
An adjacency block SHALL hold a slice of one relation's matrix in one graph, one direction and one bucket group, with
the row key, neighbour and `direction` its kind gives and the sections the *Present* column gives for its kind and
flags, as `specification/storage/STORAGE.md` sections 7.1 to 7.3 lay out: kind 1's neighbour composed from its
`prefix`, `sequence` and `bucket` sections; slot rows and slot neighbours each with their type prefix in sections 12
and 13; statements in sections 8 and 14; `created` and `deleted` only when `versioned`, `deleted` absent in class I and
above its created version when nonzero. A decoder SHALL refuse a slot at or above 2^32 − 1, a type prefix at or above
2^16, a bucket at or above 2^24 and a nonzero `deleted` version not above its created version (`Range`; T48).

#### Scenario: The adjacency vectors
- **WHEN** the tiles `adjacency_forward` and `adjacency_reverse_versioned` of `vectors/tiles.json` and
  `adjacency_forward_values`, `adjacency_index` and `adjacency_version_run` of `vectors/kinds.json` are encoded
- **THEN** each equals its `.tile` file, and its `fence_hex` and `tile_key_hex` follow from it

#### Scenario: A kind's direction and sections
- **WHEN** the cases `forward block with direction 2`, `row_prefixes section missing` and `prefix past 2^16` of
  `vectors/refusals.json` are decoded
- **THEN** they are refused with `Shape`, `Section` and `Range` respectively

### Requirement: Entry order and inline columns
Rows SHALL ascend by canonical key, a slot row by (type prefix, slot), and entries within a row SHALL be ordered by the
`order` the header names, with `1 + i` the comparison form of inline column `i` ascending and `0x81 + i` descending,
then by neighbour, then by statement with `ABSENT` last, then by created version, as
`specification/storage/STORAGE.md` sections 7.2 and 7.4 state; no two entries SHALL agree in row key, neighbour,
statement and created version. Inline columns SHALL number at most 8, each small and 1 to 18 bytes wide, each a FIXED
section `16 + 2c` with its BITMAP `17 + 2c`. A decoder SHALL refuse any other order, and two equal entries, with
`Order`.

#### Scenario: A reverse block ordered by an inline column
- **WHEN** the tile `adjacency_reverse_ordered` of `vectors/kinds.json`, `order` `0x81`, is encoded and decoded
- **THEN** it equals its `.tile` file, and each row's entries descend by inline column 0's comparison form

#### Scenario: Rows out of order are refused
- **WHEN** the case `rows descending` of `vectors/refusals.json` is decoded
- **THEN** it is refused with `Order`

#### Scenario: Two equal entries are refused
- **WHEN** a block holding two entries equal in row key, neighbour, statement and created version is decoded
- **THEN** it is refused with `Order`

### Requirement: Column chunks of kinds 1 to 3
A column chunk SHALL hold one attribute of one type in one graph and one bucket group for `span` slots from its base,
`span` a power of two at most `chunk_slots` and the base a multiple of it: kind 1 fixed-width values, kind 2
variable-width values, kind 3 the identity column of 11 bytes per slot with `subject` zero; its folded values in slot
order under the `present` bitmap and, when `versioned`, its overlay records ascending by (slot, version), each version
above `horizon`, as `specification/storage/STORAGE.md` section 8.1 lays out. At version `v` at or above `horizon`, a
slot SHALL hold its latest overlay record at or below `v`, nothing when that record holds no value, and otherwise its
folded value. A change to a slot no chunk covers SHALL belong to a new chunk of `span` `chunk_slots` at base
`s − (s mod chunk_slots)` (section 8.2), and a chunk whose encoding exceeds `tile_bytes_max` SHALL be replaced by its
two halves, the lower first, each halved again while it exceeds (section 8.3).

#### Scenario: The column vectors
- **WHEN** the tile `column_chunk_overlay` of `vectors/tiles.json` and the tiles `column_variable` and
  `column_identity` of `vectors/kinds.json` are encoded
- **THEN** each equals its `.tile` file, and decodes to its input

#### Scenario: An overlay record without a value hides the folded value
- **WHEN** a chunk of `horizon` 10 holds a folded value for slot 7 and an overlay record of slot 7 without a value at
  version 12, and the chunk is read
- **THEN** slot 7 holds the folded value at version 11 and nothing at version 12 or above

#### Scenario: A slot past every chunk opens a new one
- **WHEN** a change names slot 5,000 of a family whose only chunk covers slots 0 to 4,095, with `chunk_slots` 4,096
- **THEN** it belongs to a new chunk of base 4,096 and `span` 4,096

#### Scenario: Overlay records out of order are refused
- **WHEN** a chunk whose overlay records hold one (slot, version) twice is decoded
- **THEN** it is refused with `Order`

### Requirement: Record runs, their classes and scope
A record run SHALL be a sorted run of records, each a key and a value, ascending by key and then by created version,
a run that is not `versioned` holding each key once, with the sections and header scope that
`specification/storage/STORAGE.md` sections 9.1 and 9.2 give: kinds 2, 3, 11, 16 and 17 class I, kind 13 its type's
class, every other kind class M; `tenant`, `graph` and `group` per kind as the scope table gives them, a copy of
another partition's dictionary entry and a purge marker naming another partition's instance in the family of `group`
`ABSENT`. A decoder SHALL refuse a nonzero `deleted` version not above its record's created version (`Range`; T48).

#### Scenario: Record runs of every kind
- **WHEN** each `record_*` tile of `vectors/kinds.json` is encoded from its input and `header`
- **THEN** it equals its `.tile` file, its header's `tenant`, `graph`, `group` and `class_i` are those section 9.2
  gives its kind, and `fence_hex` and `tile_key_hex` follow

#### Scenario: An idempotency claim takes its type's class
- **WHEN** the tiles `record_idempotency_claim` and `record_idempotency_claim_append_only` are decoded
- **THEN** the first is class M and the second `class_i`

#### Scenario: A key held twice in an unversioned run is refused
- **WHEN** a record run that is not `versioned` holding one key twice is decoded
- **THEN** it is refused with `Order`

### Requirement: Record kinds 1 to 20 and their folds
Each record SHALL hold the key and value its kind gives in `specification/storage/STORAGE.md` section 9.3, the
session record's value laid out as that section's table states, and a statistics value as section 9.4 states. The
records of one key of an additive kind (7, 9 and 19) visible at a version SHALL hold their fold: a referrer set's
union by group keeping each group's greatest generation and earliest epoch, statistics merged by adding counts,
dropping kinds whose count becomes zero and taking each register's maximum, a `@limit` aggregate's changes summed.

#### Scenario: The additive folds
- **WHEN** the runs `record_referrer_set`, `record_statistics_additive` and `record_limit_aggregate` of
  `vectors/kinds.json` are folded
- **THEN** each fold equals the run's `fold_hex`

#### Scenario: The session record
- **WHEN** the tile `record_session` of `vectors/kinds.json` is decoded
- **THEN** its record's value holds `request`, `version`, `position`, `share`, `command`, the 128-byte reply header,
  `length` and the body at the offsets section 9.3 gives

#### Scenario: Statistics in one record
- **WHEN** the tile `record_statistics` of `vectors/tiles.json` is encoded from its input
- **THEN** it equals its `.tile` file

### Requirement: The free set
The free set SHALL be a tile of shape 4, kind 1, with `tenant`, `graph` and `subject` zero and `group` `ABSENT`, and
one section, `id` 1, a BITMAP of `grid_extents` bits with bit `e` set when extent `e` is allocated, bit 0 always set,
and `entries` the count of bits set, as `specification/storage/STORAGE.md` section 10 lays out; a decoder SHALL refuse
a free set with bit 0 clear (`Range`; T48).

#### Scenario: The free set vector
- **WHEN** the tile `free_set` of `vectors/tiles.json` is encoded from 64 extents with extents 0, 1 and 2 allocated
- **THEN** it equals its `.tile` file, and its `entries` is 3

### Requirement: A tile's digest is the sum of its entries' hashes
A tile's digest SHALL be the wrapping u128 sum of the XXH3-128 of each entry's digest bytes, as
`specification/storage/STORAGE.md` section 11.1 gives them per shape, so it depends on the entries and neither their
order nor their encoding; versions SHALL be no part of an adjacency entry's or a record's digest bytes. A decoder
SHALL refuse a header whose digest differs from its entries' with `Digest`.

#### Scenario: Every digest recomputes
- **WHEN** each tile of `vectors/tiles.json` and `vectors/kinds.json` is decoded
- **THEN** the digest recomputed from its entries equals its header's `digest` and the vector's

#### Scenario: A digest of other entries is refused
- **WHEN** the case `digest of other entries` of `vectors/refusals.json` is decoded
- **THEN** it is refused with `Digest`

#### Scenario: Folding versions keeps an adjacency block's digest
- **WHEN** a `versioned` adjacency block whose entries are all live is re-encoded with every created version 0 and
  without its version sections
- **THEN** both encodings have one digest

### Requirement: Compaction conserves digests
A rewrite SHALL list the entries added, those an output holds that no input holds, and the entries removed, those an
input holds that no output holds, each identified by its digest bytes, and SHALL find, before writing any output,
`Σ digest(Ui) = Σ digest(Tj) + Σ hash(added) − Σ hash(removed)`, wrapping, with each output's digest computed from its
encoding, as `specification/storage/STORAGE.md` section 11.2 states; a difference SHALL be the invariant error
`DigestConserved`. Folding an adjacency entry's versions SHALL neither add nor remove it; folding a column overlay
record SHALL remove the record and the folded value it replaces and add the new folded value.

#### Scenario: A column chunk's overlay folded
- **WHEN** the chunk `column_fold.input_file` of `vectors/compaction.json` is folded at `horizon`
- **THEN** the output equals `column_fold.output_file`, the entries added and removed are `added_hex` and
  `removed_hex`, and `input_digest` plus the added hashes less the removed equals `output_digest`

#### Scenario: A broken conservation writes nothing
- **WHEN** a rewrite's encoded outputs do not sum to the inputs' digests with its added and removed entries
- **THEN** it raises `DigestConserved` and places no output

### Requirement: The checkpoint identity
A checkpoint's identity SHALL be the XXH3-128 of 50 bytes: the superblock's `format` as a u16, then as u64s its
`derivation`, `position`, `horizon`, `committed`, the manifest root's `checksum` and the free set's `checksum`, as
`specification/storage/STORAGE.md` section 11.3 states. It SHALL hold nothing a replica's own history sets.

#### Scenario: The identity vector
- **WHEN** the identity is computed from `superblock.input` of `vectors/grid.json`
- **THEN** it equals `superblock.identity`

#### Scenario: Replicas share an identity
- **WHEN** two superblocks differ only in `replica`, `sequence`, `parent` and `checksum`
- **THEN** their identities are equal
