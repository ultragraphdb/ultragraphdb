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

