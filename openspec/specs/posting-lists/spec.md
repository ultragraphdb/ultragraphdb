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
Insert, delete and single-valued overwrite SHALL apply to a list as a set of objects. An IRI or blank-node object SHALL
be identified by its UID. A literal object SHALL be identified by its encoded term: the posting's 64-bit fingerprint of
the encoding orders the list, and postings whose fingerprints are equal SHALL be told apart by their encodings, so a
fingerprint collision never merges two literals. Literals that differ in language tag SHALL be distinct objects.

#### Scenario: A single-valued predicate replaces its object
- **WHEN** `name` is declared single-valued and `<a> <name> "A"` is written and then `<a> <name> "B"`
- **THEN** the list holds one posting, `"B"`

#### Scenario: Language-tagged literals coexist
- **WHEN** `<a> <label> "A"`, `<a> <label> "Á"@fr` and `<a> <label> "B"@fr` are written to a multi-valued `label`
- **THEN** the list holds all three

#### Scenario: A multi-valued predicate accumulates
- **WHEN** `<a> <knows> <b>` and then `<a> <knows> <c>` are written
- **THEN** the list holds `b` and `c`

#### Scenario: Colliding fingerprints keep both literals
- **WHEN** two different literals whose encodings have the same 64-bit fingerprint are written to one list
- **THEN** the list holds both, and deleting one leaves the other

#### Scenario: Deleting an object the list does not hold changes nothing
- **WHEN** `a` holds `"A"` for `name` and `<a> <name> "B"` is deleted
- **THEN** `a` still holds `"A"`

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

### Requirement: Posting lists are stored in the repository's own format
A posting list SHALL be stored in a byte format of the repository's own, defined in `ultragraphdb_core_codec` with its
types: a version byte, flags, the commit version, the packed ID list's blocks, the postings and the splits,
little-endian but for each 128-bit value, which is written as its 16 big-endian bytes, followed by an XXH3-64 checksum
of the bytes before it. IDs SHALL be 128-bit system IDs, and the version byte SHALL be the one this stage's format
introduces. A stored value whose version is unknown, whose checksum fails or whose lengths overrun SHALL be refused as
corruption naming its key, never decoded in part.

#### Scenario: A list round-trips
- **WHEN** a list with a packed ID list, value postings with statement IDs and splits is encoded and decoded
- **THEN** the decoded list equals the original

#### Scenario: A flipped byte is corruption
- **WHEN** one byte of a stored list is flipped and the list is read
- **THEN** the read fails with a corruption error naming the key

#### Scenario: A list of the previous format is refused
- **WHEN** a value written with stage 5's version byte is read
- **THEN** the read fails with a corruption error naming the key and the unknown version

