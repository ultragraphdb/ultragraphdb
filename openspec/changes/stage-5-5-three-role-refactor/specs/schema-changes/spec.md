## ADDED Requirements

### Requirement: A schema change takes effect at an epoch boundary
A schema change SHALL take effect in every partition from the epoch its catalog version names; a round of an earlier
epoch SHALL be validated against the previous version, and an index rebuild the change starts SHALL begin in each
partition at that epoch. A partition SHALL not commit a round of the named epoch before it holds the new version.

#### Scenario: Two partitions switch at the same epoch
- **WHEN** an index is added with effect from epoch 50 over two partitions
- **THEN** in both partitions rounds of epoch 49 maintain no entries of the index and rounds of epoch 50 do

#### Scenario: A partition waits for the version it needs
- **WHEN** a partition reaches epoch 50 before the version naming epoch 50 reaches it
- **THEN** it commits no round of epoch 50 until it holds that version
