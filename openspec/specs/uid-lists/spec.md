# uid-lists Specification

## Purpose
Packed UID lists, Dgraph's `UidPack` of group-varint blocks, and the sorted-set operations over packed and plain
lists.
## Requirements
### Requirement: UID lists pack into Dgraph's UidPack format
A sorted UID list SHALL encode into `UidPack` blocks byte-identical to Dgraph's `codec`: a block closes when it holds
the block size (256 in production) or when the next UID's upper 32 bits differ; a block stores its first UID as `base`
and the rest as u32 deltas in group-varint groups of four, zero-padded.

#### Scenario: Encoding matches the golden vectors
- **WHEN** the golden UID lists are encoded with block size 256
- **THEN** every block's base, deltas and count equal Dgraph's

#### Scenario: A block closes where the upper 32 bits change
- **WHEN** the list `[1, 2, 2^32 + 1]` is encoded with block size 256
- **THEN** it yields two blocks, with bases 1 and 2^32 + 1

#### Scenario: A one-UID block emits one empty group
- **WHEN** the list `[7]` is encoded
- **THEN** its single block has base 7, count 1 and deltas `00 00 00 00 00`

### Requirement: Packed lists decode, seek and round-trip
Decoding SHALL return the encoded UIDs in order, and decoding from a target SHALL return every UID at or after it.
Seeking a decoder SHALL return the UIDs of the target's block at or after the target (`SeekStart`) or after it
(`SeekCurrent`), and `Next` SHALL return the following blocks.

#### Scenario: Lists of every length round-trip
- **WHEN** each sorted list of length 0 to 100, including lists spanning a change in the upper 32 bits, is encoded and
  decoded
- **THEN** the decoded list equals the input

#### Scenario: Decoding and seeking from a target
- **WHEN** a packed list of the 1,000 UIDs 10, 20, …, 10,000 is decoded from 5,005 and a decoder is sought to 5,005
- **THEN** decoding returns the 500 UIDs from 5,010 on, and the seek returns the rest of the target's block from 5,010

### Requirement: Sorted-set operations over UID lists
Intersection, union (merge), difference and index-of SHALL operate on sorted UID lists and on packed lists as Dgraph's
`algo` does: results are sorted, merges remove duplicates, and an intersection keeps a UID duplicated in both inputs.

#### Scenario: Intersection of packed and plain lists agrees
- **WHEN** two lists are intersected as plain lists and as packed lists
- **THEN** both results are equal, sorted and free of duplicates

#### Scenario: Merging several lists removes duplicates
- **WHEN** `[1,3,5]`, `[3,4]` and `[5,6]` are merged
- **THEN** the result is `[1,3,4,5,6]`

### Requirement: Packed set operations equal the plain ones
The intersection of several packed lists SHALL be the intersection of every list, and the difference of two packed
lists SHALL be the difference of their UIDs over all blocks, with the plain lists' multiset semantics. Both SHALL give
the result the plain `IntersectSorted` and `Difference` give for the same UIDs, whatever the block boundaries; Dgraph's
packed versions do not (ledger 1, 2, 25, 26).

#### Scenario: Three packed lists intersect to their common UIDs
- **WHEN** `[1,2,3,9]`, `[2,3,9,10,11]` and `[9,20,21,22,23,24]` are packed and intersected
- **THEN** the result is `[9]`

#### Scenario: A packed difference reads every block of the first list
- **WHEN** `2..15` packed in blocks of 5 has `[1]` subtracted
- **THEN** the result is `2..15`

#### Scenario: A UID in a later block of the second list is subtracted
- **WHEN** `[642]` has `[1,2,3,4,5,642]` packed in blocks of 5 subtracted
- **THEN** the result is empty

#### Scenario: Subtracting an empty pack keeps the first list
- **WHEN** `[1,2,3]` has an empty pack of block size 5 subtracted
- **THEN** the result is `[1,2,3]`

### Requirement: Decoders seek without panicking or hanging
A decoder SHALL exist only once positioned by a seek. Seeking to a block SHALL return the whole block holding the
target, untruncated as Go's `SeekToBlock` returns it, or nothing
when no UID lies at or after it: past the last UID, backwards into an earlier block, onto an empty block, and to
`u64::MAX`. `linear_seek(u64::MAX)` SHALL return the last block, as Go's documentation of `LinearSeek` states and as
Go returns for `u64::MAX - 1` (ledger 3, 4, 5, 27).

#### Scenario: Intersecting from past a pack's last UID returns nothing
- **WHEN** `1..100` packed in blocks of 10 is intersected with `1..11` after UID 200
- **THEN** the result is empty and nothing panics

#### Scenario: A backward seek returns the target's block
- **WHEN** a decoder over `1..40` in blocks of 10 seeks to 35 and then to 5
- **THEN** the second seek returns the block `1..10`

#### Scenario: An empty block contributes nothing to a seek
- **WHEN** a pack holds a block whose UID count is 0 and a seek targets it
- **THEN** the seek moves on to the next block and nothing panics

#### Scenario: A linear seek to the largest UID stays on the last block
- **WHEN** a decoder over `[1,2,3]` in blocks of 2 is linearly sought to `u64::MAX`
- **THEN** it returns `[3]`

