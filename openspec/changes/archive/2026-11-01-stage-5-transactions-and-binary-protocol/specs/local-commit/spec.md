## MODIFIED Requirements

### Requirement: A request reads its own earlier edges
Within one transaction, applying an edge SHALL read the list with that transaction's earlier edges applied on top of
the state at its read version; no other transaction SHALL see them before the transaction is acknowledged, and a
transaction SHALL never read another transaction's writes that are not visible at its read version. A list built
without a store read SHALL be completed from the store before any read of it in the same transaction, and a scalar list
SHALL be read through the merging iterator.

#### Scenario: A later edge reads an earlier edge of its request
- **WHEN** `name` of `0x1` holds no value and a transaction sets it to `A` and then deletes `A`
- **THEN** after the commit, `name` of `0x1` holds no value

#### Scenario: Another request does not see uncommitted writes
- **WHEN** one transaction has set `name` on `0x1` and not committed, and a second transaction reads it
- **THEN** the second transaction sees the value before the first transaction's write

#### Scenario: A transaction in the same round does not read another's writes
- **WHEN** two transactions of one round execute from the same watermark and the first sets `name` of `0x1` to `A`
- **THEN** the second reads `name` of `0x1` as it was at the watermark

#### Scenario: A list first written blind is read whole
- **WHEN** `friend` of `0x1` holds `0x2` in the store, and a transaction adds `0x3` without reading the list and then
  reads it
- **THEN** the read holds `0x2` and `0x3`

### Requirement: A commit is written in one batch at its timestamp
The admitted transactions of a round SHALL be written, once the round is synced in the log, as one write batch holding
every posting list they touched with their index, reverse and count entries and any catalog entries they create, each
transaction at its commit version, recording the round's last commit version as applied in the same batch, without a
sync of the store. The in-memory applied version and the visibility watermark SHALL advance only after the batch and its
cache update. A transaction SHALL take its commit version from the commit step.

#### Scenario: A commit is visible whole
- **WHEN** a transaction writing three lists commits at 12 while another reader reads at 12 repeatedly
- **THEN** every read sees the changes to all three lists or to none of them

#### Scenario: Two commits in one batch are visible together
- **WHEN** one round holds a commit at 12 and a commit at 14 while another reader reads at 14 repeatedly
- **THEN** every read sees both commits or neither

#### Scenario: A failed request writes nothing
- **WHEN** a transaction aborts after building writes
- **THEN** the store holds none of its writes

#### Scenario: A failed request holds up no read
- **WHEN** a transaction aborts and another transaction begins
- **THEN** the second transaction begins at the watermark without waiting for the aborted one

### Requirement: Applied timestamps gate reads
A transaction SHALL read at the visibility watermark, which never passes a version not yet synced and applied, so its
reads never wait for a commit; a read at an explicit version above the watermark SHALL wait until the watermark reaches
it. A rollup SHALL read at the watermark and write one above the newest commit version it folds, taking no version from
the commit step; the oldest open read SHALL bound the discard timestamp.

#### Scenario: A read waits for a commit below its timestamp
- **WHEN** a commit at 10 is being applied and a read at 10 starts
- **THEN** the read returns after the commit is applied and sees it

#### Scenario: A rollup leaves an open read intact
- **WHEN** a read at 6 is open and a list with deltas at 6 and 8 is rolled up
- **THEN** the rollup is written at 9, the read at 6 still returns its result, and the discard timestamp stays at or
  below 6
