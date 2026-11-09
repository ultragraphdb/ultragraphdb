# uid-lists Specification

## Purpose
Packed ID lists: sorted lists of 128-bit system IDs in the repository's own format of LEB128 blocks, how they decode,
seek and round-trip, and the sorted-set operations over packed and plain lists, which give the same results.

## Requirements
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

### Requirement: Sorted-set operations over UID lists
Intersection, union (merge), difference and index-of SHALL operate on sorted lists of 128-bit system IDs and on packed
lists as Dgraph's `algo` does on UIDs: results are sorted, merges remove duplicates, and an intersection keeps an ID
duplicated in both inputs.

#### Scenario: Intersection of packed and plain lists agrees
- **WHEN** two lists of system IDs are intersected as plain lists and as packed lists
- **THEN** both results are equal, sorted and free of duplicates

#### Scenario: Merging several lists removes duplicates
- **WHEN** the lists of type 1, bucket 0 and sequences `[1,3,5]`, `[3,4]` and `[5,6]` are merged
- **THEN** the result holds sequences `[1,3,4,5,6]` of type 1, bucket 0

### Requirement: Packed set operations equal the plain ones
The intersection of several packed lists SHALL be the intersection of every list, and the difference of two packed
lists SHALL be the difference of their system IDs over all blocks, with the plain lists' multiset semantics. Both SHALL
give the result the plain intersection and difference give for the same IDs, whatever the block boundaries.

#### Scenario: Three packed lists intersect to their common IDs
- **WHEN** the sequences `[1,2,3,9]`, `[2,3,9,10,11]` and `[9,20,21,22,23,24]` of type 1, bucket 0 are packed and
  intersected
- **THEN** the result is the ID of sequence 9

#### Scenario: A packed difference reads every block of the first list
- **WHEN** sequences `2..15` packed in blocks of 5 have sequence `[1]` subtracted, all of type 1, bucket 0
- **THEN** the result is sequences `2..15`

#### Scenario: An ID in a later block of the second list is subtracted
- **WHEN** sequence `[642]` has sequences `[1,2,3,4,5,642]` packed in blocks of 5 subtracted, all of type 1, bucket 0
- **THEN** the result is empty

#### Scenario: Subtracting an empty pack keeps the first list
- **WHEN** sequences `[1,2,3]` have an empty pack of block size 5 subtracted
- **THEN** the result is sequences `[1,2,3]`

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

### Requirement: ID lists pack into the repository's own format
A sorted list of distinct system IDs SHALL encode into blocks of at most the block size (256 in production): a block
SHALL store its first ID whole as 16 bytes and each later ID as the unsigned difference of its first 104 bits (type,
sequence and bucket; the reserved bits, always zero, left out) from those of the ID before it, in LEB128, and the
encoded list SHALL begin with a format version byte. A decoded difference that carries an ID past 104 bits, or a
first ID whose reserved bits are not zero, SHALL be refused as corruption. A list that is not strictly increasing SHALL
be refused when it is encoded, naming the first position out of order. Golden values for the format SHALL be generated
from this specification by an implementation independent of the codec and committed with their source.

#### Scenario: A block closes at the block size
- **WHEN** 300 increasing IDs are encoded with block size 256
- **THEN** the encoding holds two blocks, of 256 and 44 IDs, each beginning with its first ID whole

#### Scenario: A delta is written in LEB128
- **WHEN** the IDs (type 1, bucket 0, sequence 1) and (type 1, bucket 0, sequence 129) are encoded
- **THEN** the second is written as the difference of their first 104 bits, 128 × 2^24 since the sequence sits above
  the 24-bit bucket, in LEB128 the bytes `80 80 80 80 08`

#### Scenario: A list out of order is refused
- **WHEN** a list whose third ID is not greater than its second is encoded
- **THEN** encoding fails with an error naming position 2

