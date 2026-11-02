## MODIFIED Requirements

### Requirement: Unique values
A write SHALL be refused, writing nothing of its changeset, when it gives a predicate declared unique an object that
another subject holds for that predicate in the same graph. Writing an object the same subject already holds SHALL be
accepted. Every write of a unique predicate's value, a plain write as well as an insert-if-absent or an
upsert-and-link, SHALL claim the value at the partition owning the value's bucket (system-identity, "The bucket comes
from the namespace and the key"), which records the claim and decides it; a claim from another partition is
cross-partition and SHALL be refused with `CrossPartition` in this stage (partitions, "A read-write transaction stays
within one partition in this stage"), with one partition it is local.

#### Scenario: A second subject with a unique value is refused
- **WHEN** `id` is declared unique, `<a> <id> "x"` is held, and `<b> <id> "x"` is written, all in the partition owning
  the bucket of `"x"`
- **THEN** the write fails with an error naming `id`, `"x"` and `a`, and nothing of its changeset is held

#### Scenario: The same subject may write its unique value again
- **WHEN** `id` is declared unique, `<a> <id> "x"` is held, and `<a> <id> "x"` is written, in the partition owning the
  bucket of `"x"`
- **THEN** the write succeeds and `a` holds `id` `"x"`

#### Scenario: A unique value is unique per graph
- **WHEN** `id` is declared unique and `<a> <id> "x"` is held in graph `g1`, and `<b> <id> "x"` is written in graph
  `g2`, all in the partition owning the bucket of `"x"`
- **THEN** both statements are held

#### Scenario: A plain write and an insert-if-absent from two partitions cannot both claim
- **WHEN** `id` is declared unique, partition 1 owns the bucket of `"x"`, a transaction bound to partition 2 writes
  `<b> <id> "x"`, and a transaction bound to partition 1 insert-if-absents a node with `id` `"x"`
- **THEN** the write in partition 2 aborts with `CrossPartition` naming partitions 2 and 1, the insert-if-absent
  commits, and one node holds `id` `"x"`
