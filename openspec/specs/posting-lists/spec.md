# posting-lists Specification

## Purpose
Dgraph's posting lists over the store: reads that merge deltas over the complete list, posting operations and
their identities, delete-all markers, rollups, and split lists.
## Requirements
### Requirement: A read merges deltas over the complete list
Reading a posting list at timestamp `t` SHALL merge, newest first, the deltas committed at or below `t` over the
newest complete version at or below `t`, SHALL ignore deltas committed above `t`, and SHALL read the older versions
when `t` is below the newest complete or empty version; Dgraph serves an empty rollup to reads below it
(`mvcc.go:708-709`).

#### Scenario: A read does not see a later delta
- **WHEN** a list holds a delta committed at 6 and is read at 5
- **THEN** the result excludes the delta's postings

#### Scenario: A read merges deltas over the complete list
- **WHEN** a list holds a complete version at 4 and deltas at 5 and 6 and is read at 6
- **THEN** the result is the complete list with both deltas applied in commit order

#### Scenario: A read below a rollup reads the older versions
- **WHEN** a list with deltas at 3 and 5 was rolled up into a complete version at 6 and is read at 4
- **THEN** the result is the list as of the delta at 3

#### Scenario: A read below an empty rollup reads the older versions
- **WHEN** a list holds a delta at 4, a delete-all at 5 and an empty rollup at 6, and is read at 4
- **THEN** the result holds the delta's postings

### Requirement: Posting operations and identities
Set, delete and overwrite SHALL apply as in Dgraph. A reference posting SHALL be identified by its target uid; an
untagged scalar value by uid `MaxUint64`; a language-tagged value by the 64-bit farm fingerprint of its language; a
value of a list type by the fingerprint of the value.

#### Scenario: A scalar value overwrites the previous one
- **WHEN** `<0x1> <name> "A"` is set and then `<0x1> <name> "B"` is set
- **THEN** the list holds one value posting, `B`

#### Scenario: Language values coexist
- **WHEN** `name` is set untagged to `A` and with language `fr` to `Á`
- **THEN** both are held, and a read for language `fr` returns `Á`

#### Scenario: A single uid predicate replaces its target
- **WHEN** a non-list uid predicate is set to `0x2` and then to `0x3`
- **THEN** the list holds only `0x3`

#### Scenario: A uid list accumulates
- **WHEN** a `[uid]` predicate is set to `0x2` and then to `0x3`
- **THEN** the list holds `0x2` and `0x3`

#### Scenario: Deleting a non-matching scalar value changes nothing
- **WHEN** `name` holds `A` and a delete of `name` with value `B` is applied
- **THEN** `name` still holds `A`

### Requirement: Delete-all markers
A delete-all (`_STAR_ALL`) committed at `T` SHALL hide the complete list and every earlier delta for reads at or
above `T`, and SHALL hide nothing for reads below `T`.

#### Scenario: Delete-all hides earlier postings from later reads
- **WHEN** a list holds postings, a delete-all is committed at 8, and the list is read at 8 and at 7
- **THEN** the read at 8 is empty and the read at 7 returns the postings

### Requirement: Rollups
A rollup at timestamp `R` SHALL write the list visible at `R` as one complete version with the discard flag, at the
largest of the list's complete-version timestamp, its delete-all timestamp and its visible delta commit timestamps,
plus one when `R` is finite; it SHALL keep only value postings and postings with facets beside the packed uids, and
reads at or above that version SHALL return what they returned before. It SHALL write its main key, its parts and the
deletions of the parts it removes in one batch at the main key's version; Dgraph writes the parts one version below and
each key in a transaction of its own. A rollup whose key gained a deletion above the rollup's version SHALL be
discarded and redone.

#### Scenario: A rollup preserves every read above it
- **WHEN** a list with a complete version and several deltas is rolled up
- **THEN** reads at every timestamp at or above the rollup's version return the same results as before it

#### Scenario: A rollup at the maximum timestamp takes the last commit's version
- **WHEN** a list holds deltas committed at 2, 4 and 6 and a delete-all committed at 10, and is rolled up at the
  maximum timestamp
- **THEN** the complete version is written at 10, the delete-all's commit timestamp

#### Scenario: A rollup at a finite timestamp writes one above the last commit
- **WHEN** the same list is rolled up at timestamp 12
- **THEN** the complete version is written at 11

#### Scenario: Facets survive a rollup
- **WHEN** a reference posting carrying a facet is rolled up
- **THEN** the posting and its facet are read back after the rollup

#### Scenario: A crash during a split rollup leaves one list whole
- **WHEN** the process crashes while the rollup of a split list is written, and the store is reopened
- **THEN** the list reads as it did before the rollup, with its old parts, or as after it, with its new parts

### Requirement: Split lists
A list whose encoded size reaches 512 KiB and that spans more than one block SHALL be split at the middle block's
base uid, recursively, into parts under split keys, the first part starting at uid 1; a rollup SHALL remove empty
parts and delete their keys, and a list left with one empty part SHALL become unsplit. Reading a split key as a list
SHALL fail with `cannot read posting list using multi-part list key`.

#### Scenario: A large list is split into parts
- **WHEN** a list grows past 512 KiB over several blocks and is rolled up
- **THEN** its complete version records its parts and each part is stored under a split key

#### Scenario: Iteration after a uid crosses part boundaries
- **WHEN** a split list is iterated from a uid in its first part
- **THEN** the result is every uid above it, across all parts, in order

#### Scenario: Deleting every uid of a split list empties it
- **WHEN** every uid of a split list is deleted and the list is rolled up
- **THEN** the list is stored as empty and no split key of the list remains

#### Scenario: A split part cannot be read as a list
- **WHEN** a split key is read directly as a posting list
- **THEN** the read fails with `cannot read posting list using multi-part list key`

