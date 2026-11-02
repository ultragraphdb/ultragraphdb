## REMOVED Requirements

### Requirement: UID lists pack into Dgraph's UidPack format
**Reason**: System IDs are 128 bits (system-identity), so Dgraph's 64-bit `UidPack` of u32 group-varint deltas cannot
hold them, and byte-for-byte compatibility with Dgraph ends. The Go golden vectors (`codec_uidpack`, `codec_seek`) and
their generator code are removed with it (decided 2026-11-01).
**Migration**: Packed lists use the format of "ID lists pack into the repository's own format"; stores written before
this stage are not readable, and there are no outside users.

## ADDED Requirements

### Requirement: ID lists pack into the repository's own format
A sorted list of distinct system IDs SHALL encode into blocks of at most the block size (256 in production): a block
SHALL store its first ID whole as 16 bytes and each later ID as the unsigned difference of its first 104 bits (type,
bucket and sequence; the reserved bits, always zero, left out) from those of the ID before it, in LEB128, and the
encoded list SHALL begin with a format version byte. A decoded difference that carries an ID past 104 bits, or a
first ID whose reserved bits are not zero, SHALL be refused as corruption. A list that is not strictly increasing SHALL
be refused when it is encoded, naming the first position out of order. Golden values for the format SHALL be generated
from this specification by an implementation independent of the codec and committed with their source.

#### Scenario: A block closes at the block size
- **WHEN** 300 increasing IDs are encoded with block size 256
- **THEN** the encoding holds two blocks, of 256 and 44 IDs, each beginning with its first ID whole

#### Scenario: A delta is written in LEB128
- **WHEN** the IDs (type 1, bucket 0, sequence 1) and (type 1, bucket 0, sequence 129) are encoded
- **THEN** the second is written as the difference 128 of their first 104 bits, in LEB128 the bytes `80 01`

#### Scenario: A list out of order is refused
- **WHEN** a list whose third ID is not greater than its second is encoded
- **THEN** encoding fails with an error naming position 2

## MODIFIED Requirements

### Requirement: Packed lists decode, seek and round-trip
Decoding SHALL return the encoded IDs in order, and decoding from a target SHALL return every ID at or after it.
Seeking a decoder SHALL return the IDs of the target's block at or after the target (`SeekStart`) or after it
(`SeekCurrent`), and `Next` SHALL return the following blocks.

#### Scenario: Lists of every length round-trip
- **WHEN** each sorted list of length 0 to 100, including lists spanning two types and two buckets, is encoded and
  decoded
- **THEN** the decoded list equals the input

#### Scenario: Decoding and seeking from a target
- **WHEN** a packed list of the 1,000 IDs of type 1, bucket 0 and sequences 10, 20, …, 10,000 is decoded from sequence
  5,005 and a decoder is sought to sequence 5,005
- **THEN** decoding returns the 500 IDs from sequence 5,010 on, and the seek returns the rest of the target's block from
  sequence 5,010

### Requirement: Decoders seek without panicking or hanging
A decoder SHALL exist only once positioned by a seek, and SHALL take any 128-bit value as a target, valid as a system
ID or not. Seeking to a block SHALL return the whole block holding the target, untruncated, or nothing when no ID lies
at or after it: past the last ID, backwards into an earlier block, onto an empty block, and to `u128::MAX`.
`linear_seek(u128::MAX)` SHALL return the last block.

#### Scenario: Intersecting from past a pack's last ID returns nothing
- **WHEN** the IDs of type 1, bucket 0 and sequences 1 to 100, packed in blocks of 10, are intersected with sequences 1
  to 11 after sequence 200
- **THEN** the result is empty and nothing panics

#### Scenario: A backward seek returns the target's block
- **WHEN** a decoder over sequences 1 to 40 of type 1, bucket 0, in blocks of 10, seeks to sequence 35 and then to 5
- **THEN** the second seek returns the block of sequences 1 to 10

#### Scenario: An empty block contributes nothing to a seek
- **WHEN** a pack holds a block whose ID count is 0 and a seek targets it
- **THEN** the seek moves on to the next block and nothing panics

#### Scenario: A seek to the largest target finds nothing past the end
- **WHEN** a decoder over sequences 1 to 3 of type 1, bucket 0, in blocks of 2, seeks to `u128::MAX`
- **THEN** the seek returns nothing and nothing panics

#### Scenario: A linear seek to the largest target stays on the last block
- **WHEN** a decoder over sequences 1 to 3 of type 1, bucket 0, in blocks of 2, is linearly sought to `u128::MAX`
- **THEN** it returns the block holding sequence 3
