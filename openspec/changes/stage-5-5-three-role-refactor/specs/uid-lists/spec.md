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
