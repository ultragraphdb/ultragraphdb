## ADDED Requirements

### Requirement: UID lists pack into Dgraph's UidPack format
A sorted UID list SHALL encode into `UidPack` blocks byte-identical to Dgraph's `codec`: a block closes when it
holds the block size (256 in production) or when the next UID's upper 32 bits differ; a block stores its first
UID as `base` and the rest as u32 deltas in group-varint groups of four, zero-padded.

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
Decoding SHALL return the encoded UIDs in order, and seeking SHALL return the UIDs at or after a target.

#### Scenario: Lists of every length round-trip
- **WHEN** each sorted list of length 0 to 100, including lists spanning a change in the upper 32 bits, is
  encoded and decoded
- **THEN** the decoded list equals the input

#### Scenario: Seek returns the suffix at or after the target
- **WHEN** a packed list of 1000 UIDs is sought to a UID inside it and to one between two of its UIDs
- **THEN** both return exactly the UIDs greater than or equal to the target

### Requirement: Sorted-set operations over UID lists
Intersection, union (merge), difference and index-of SHALL operate on sorted UID lists and on packed lists,
and SHALL return sorted, duplicate-free results.

#### Scenario: Intersection of packed and plain lists agrees
- **WHEN** two lists are intersected as plain lists and as packed lists
- **THEN** both results are equal, sorted and free of duplicates

#### Scenario: Merging several lists removes duplicates
- **WHEN** `[1,3,5]`, `[3,4]` and `[5,6]` are merged
- **THEN** the result is `[1,3,4,5,6]`
