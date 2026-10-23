## ADDED Requirements

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
A decoder SHALL exist only once positioned by a seek. Seeking SHALL return the block holding the target, or nothing
when no UID lies at or after it: past the last UID, backwards into an earlier block, onto an empty block, and to
`u64::MAX`. `linear_seek(u64::MAX)` SHALL return the last block, as Go's documentation of `LinearSeek` states and as
Go returns for `u64::MAX - 1` (ledger 3, 4, 5, 27).

#### Scenario: Intersecting from past a pack's last UID returns nothing
- **WHEN** `1..100` packed in blocks of 10 is intersected with `1..11` after UID 200
- **THEN** the result is empty and nothing panics

#### Scenario: A backward seek returns the target's block
- **WHEN** a decoder over `1..40` in blocks of 10 seeks to 35 and then to 5
- **THEN** the second seek returns `5..10`

#### Scenario: An empty block contributes nothing to a seek
- **WHEN** a pack holds a block whose UID count is 0 and a seek targets it
- **THEN** the seek moves on to the next block and nothing panics

#### Scenario: A linear seek to the largest UID stays on the last block
- **WHEN** a decoder over `[1,2,3]` in blocks of 2 is linearly sought to `u64::MAX`
- **THEN** it returns `[3]`
