# local-commit Specification

## Purpose
How requests commit locally: a request reads its own earlier edges, a commit is written in one batch at its
timestamp, and applied timestamps gate reads.
## Requirements
### Requirement: A request reads its own earlier edges
Within one request, applying an edge SHALL read the list with that request's earlier edges applied on top of the state
at its read timestamp, and scratch layers SHALL stack, so that a request applied after another in one batch reads that
request's writes; no request outside the batch SHALL see them before the batch is written. A list built without a store
read SHALL be completed from the store before any read of it in the same request, and a scalar list SHALL be read
through the merging iterator.

#### Scenario: A later edge reads an earlier edge of its request
- **WHEN** `name` of `0x1` holds no value and a request sets it to `A` and then deletes `A`
- **THEN** after the commit, `name` of `0x1` holds no value

#### Scenario: A later request in a batch reads an earlier one
- **WHEN** two requests are applied in one batch and the first sets `name` of `0x1` to `A`
- **THEN** the second reads `A` for `name` of `0x1`

#### Scenario: Another request does not see uncommitted writes
- **WHEN** one request has set `name` on `0x1` and not committed, and a second request reads it
- **THEN** the second request sees the value before the first request's write

#### Scenario: A list first written blind is read whole
- **WHEN** `friend` of `0x1` holds `0x2` in the store, and a request adds `0x3` without reading the list and then reads it
- **THEN** the read holds `0x2` and `0x3`

### Requirement: A commit is written in one batch at its timestamp
Committing a request SHALL write every posting list it touched, with their index, reverse and count entries and any
schema entries it creates, as part of one write batch, at the request's commit timestamp, and SHALL record the commit
timestamp as applied in the same batch; a batch MAY hold several requests' commits, each at its own timestamp; the
in-memory applied timestamp SHALL advance only after the batch and its cache update. A request SHALL take its commit
timestamp from the writer when its batch is written.

#### Scenario: A commit is visible whole
- **WHEN** a request writing three lists commits at 12 while another reader reads at 12 repeatedly
- **THEN** every read sees the changes to all three lists or to none of them

#### Scenario: Two commits in one batch are visible together
- **WHEN** one batch holds a commit at 12 and a commit at 13 while another reader reads at 13 repeatedly
- **THEN** every read sees both commits or neither

#### Scenario: A failed request writes nothing
- **WHEN** a request fails after building writes and before committing
- **THEN** the store holds none of its writes

#### Scenario: A failed request holds up no read
- **WHEN** a request fails before its batch is written and a read at the next timestamp starts
- **THEN** the read returns without waiting for the failed request

### Requirement: Applied timestamps gate reads
A read at timestamp `t` SHALL wait until every commit timestamp assigned at or below `t` is applied, and SHALL then
read one snapshot. A rollup SHALL take a fresh timestamp and wait until every commit below it is applied; the oldest
open read SHALL bound the discard timestamp.

#### Scenario: A read waits for a commit below its timestamp
- **WHEN** a commit at 10 is being written and a read at 11 starts
- **THEN** the read returns after the commit is applied and sees it

#### Scenario: A rollup leaves an open read intact
- **WHEN** a read at 5 is open and a list with deltas at 5 and 6 is rolled up
- **THEN** the rollup is written above 6, the read at 5 still returns its result, and the discard timestamp stays at or
  below 5

