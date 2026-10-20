## ADDED Requirements

### Requirement: A request sees its own writes
Within one request, a read SHALL see that request's writes on top of the state at its read timestamp, and no
other request SHALL see them before they commit.

#### Scenario: A request reads its own write
- **WHEN** a request sets `name` on `0x1` and then reads `name` of `0x1`
- **THEN** the read returns the new value

#### Scenario: Another request does not see uncommitted writes
- **WHEN** one request has set `name` on `0x1` and not committed, and a second request reads it
- **THEN** the second request sees the value before the first request's write

### Requirement: A commit is one batch at one timestamp
Committing a request SHALL write every posting list it touched, with their index, reverse and count entries and any
schema entries it creates, as one write batch at the request's commit timestamp, and SHALL record the commit timestamp
as applied in the same batch; the in-memory applied timestamp SHALL advance only after the batch and its cache update.

#### Scenario: A commit is visible whole
- **WHEN** a request writing a value, its index entry and a reverse edge commits at 12 while another reader reads
  at 12 repeatedly
- **THEN** every read sees all three entries or none of them

#### Scenario: A failed request writes nothing
- **WHEN** a request fails after building writes and before committing
- **THEN** the store holds none of its writes

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
