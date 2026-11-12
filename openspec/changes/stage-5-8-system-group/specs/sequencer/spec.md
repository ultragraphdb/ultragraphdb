## ADDED Requirements

### Requirement: An early close is asked by CloseRequest from either role
The sequencer SHALL close its open epoch early when a CloseRequest names it, from a partition whose epoch is full, as
before, or from the frontend with `partition` 0, for a snapshot read waiting on the open epoch (quad-reads, "A read's
transaction names its level"). A CloseRequest naming an epoch already closed SHALL be answered and SHALL close nothing.

#### Scenario: The frontend's request closes the open epoch
- **WHEN** epoch 12 is the latest close, and the frontend sends CloseRequest naming epoch 13 with `partition` 0
- **THEN** the sequencer closes epoch 13 before `epoch_ms_max` has passed, once `epoch_ms_min` has, and every
  subscriber receives the close

#### Scenario: A request for a closed epoch closes nothing
- **WHEN** epoch 13 is closed and a CloseRequest names epoch 13
- **THEN** it is answered, and epoch 14 closes at its own time

### Requirement: A subscriber holding no close begins at the latest
The sequencer SHALL begin a subscription whose Subscribe sets `latest`, which the role holding the client endpoint
alone sends, with `from` 0, at the latest close it holds durable, whatever its epoch, or at the close of epoch 1 when
none exists yet, and SHALL deliver every later close in order. It SHALL end a subscription whose frames have gone
unacknowledged by the transport for twice `epoch_ms_max`, closing its connection.

#### Scenario: A late subscriber is not refused for trimmed closes
- **WHEN** the sequencer has closed 40 epochs and holds the last 10, and a Subscribe sets `latest`
- **THEN** the stream's first close is that of epoch 40, and the close of epoch 41 follows it

#### Scenario: A storage process may not ask for the latest
- **WHEN** a connection whose role is `storage` sends a Subscribe with `latest` 1
- **THEN** it is refused with `FieldValue` naming `latest`

## MODIFIED Requirements

### Requirement: Closes announce the versions taking effect after them
At each close `c + 1` the sequencer SHALL assign the effective epoch `c + 2` to every catalog version the metadata group
holds pending (metadata-group, "Catalog versions take effect at an epoch boundary") and to the oldest placement version
it holds pending, at most one per close (metadata-group, "The placement map assigns bucket ranges to partitions"),
record the assignments in the close's durable record, and deliver them with the close, which announces them; the
metadata group SHALL learn each version's effective epoch from that close. A version still pending when the system
group's host crashes before a close SHALL be assigned at the next close after recovery. The close record SHALL likewise
carry the session evictions the registry made since the previous close (metadata-group, "The metadata group registers
sessions"). A partition SHALL take a catalog or placement version into effect from its assigned epoch, and therefore
holds it before it can stamp that epoch.

#### Scenario: A version reaches every partition before its epoch
- **WHEN** a catalog version is pending while epoch 38 is the latest close, and the sequencer closes epoch 39
- **THEN** the close of epoch 39 assigns it effective epoch 40, its durable record holds the assignment, and every
  partition holds the version before it stamps a commit at epoch 40

#### Scenario: A close assigns one placement version
- **WHEN** placement versions 1 and 2 are pending while epoch 38 is the latest close, and the sequencer closes epochs
  39 and 40
- **THEN** the close of 39 assigns version 1 effective epoch 40, and the close of 40 assigns version 2 effective epoch
  41

#### Scenario: A crash before the close leaves the version pending
- **WHEN** a catalog version is committed as pending while epoch 38 is the latest close, and the system group's host is killed before it closes epoch 39
- **THEN** after recovery the version is still pending, and the next close, of epoch 39, assigns it effective epoch 40

### Requirement: The sequencer is a log-backed state machine with one replica
The sequencer's state SHALL be a function of its log: each close and each accepted transaction SHALL be appended to its
log and acknowledged only once durable, and recovery SHALL rebuild the state by replaying the log, ending at the same
last closed epoch and transaction order as before the crash. It SHALL be a state machine of the system group, hosted by
a storage process beside the metadata state machine (system-group, "One storage instance hosts the system group"), with
one replica until stage 8; its interface SHALL be the one stage 8 replicates.

#### Scenario: Recovery rebuilds the sequencer
- **WHEN** the sequencer closes epochs 1 to 12, accepts three transactions, and is killed and restarted
- **THEN** its last closed epoch is 12, and reading its log returns the three transactions in their order

#### Scenario: An unacknowledged close is not delivered
- **WHEN** the sequencer is killed after appending a close and before the append is durable
- **THEN** no partition received that close before the crash, and after restart the close is either durable and
  delivered or absent

### Requirement: Partitions learn each closed epoch and stamp the open one
The sequencer SHALL deliver every close, in order, to every subscriber: every partition, and every endpoint core of the
frontend, which takes from the closes the epochs, the catalog versions and the placement version its view holds
(client-endpoint, "An endpoint core keeps a view of what the closes announce"). A partition SHALL stamp its commits with
its open epoch, exactly one above the latest close delivered to it, and SHALL never advance its epoch on its own
(commit-rounds), so no commit is ever stamped with an epoch whose close has been delivered to the partition, and the
partition holds the catalog versions announced with its last delivered close when it stamps the epoch they take effect
at. A partition SHALL have closed an epoch once it has received that epoch's close and applied every round stamped at or
below it; a snapshot read at an epoch waits for exactly that (quad-reads). A partition that reconnects SHALL receive the
closes it missed, from its last delivered epoch.

#### Scenario: A partition stamps the open epoch
- **WHEN** epochs 1 to 7 are delivered to a partition and it commits a transaction
- **THEN** the commit's version carries epoch 8

#### Scenario: A partition stamps no epoch ahead of its closes
- **WHEN** epochs 1 to 7 are delivered to a partition, the sequencer has closed epoch 8, and its close has not yet
  reached the partition
- **THEN** the partition's commits still carry epoch 8, and its first commit after the close of 8 arrives carries
  epoch 9

#### Scenario: A partition closes an epoch once its rounds are applied
- **WHEN** a partition holds an unapplied round of epoch 8 and the close of epoch 8 is delivered
- **THEN** the partition reports epoch 8 closed only after that round is applied, and a snapshot read at epoch 8 waits
  until then

#### Scenario: A reconnecting partition catches up
- **WHEN** a partition loses its connection after epoch 3 and reconnects after epoch 9 closed
- **THEN** it receives epochs 4 to 9 in order before any later close
