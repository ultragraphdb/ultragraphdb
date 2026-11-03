## ADDED Requirements

### Requirement: The sequencer closes epochs at its own wall-clock time
The sequencer SHALL close epochs in order, numbered from 1, each at least `epoch_ms_min` and at most `epoch_ms_max`
after the previous close, both settings with no default the binary chooses silently. `epoch_ms_min` SHALL lie from 1
to 1,000: from 1 ms, epoch numbers of 42 bits last over a century, and up to 1 s, a waiting read or a full partition
waits at most that long for its requested close. `epoch_ms_max` SHALL lie from `epoch_ms_min` to 10,000: up to 10 s, a
snapshot never lags the latest commit by more than the read-write transaction limit, and twice it stays within
`close_request_ms`'s bound of 60,000 (commit-rounds). A value out of its bounds SHALL stop the frontend at start with
exit code 78 and an error naming the setting and the bound. It SHALL close before
`epoch_ms_max`, as soon as `epoch_ms_min` has passed, when the frontend hosting it asks because a snapshot read waits on
the open epoch (quad-reads, "A read's transaction names its level"), or when a partition asks because its sequence would
run out (commit-rounds, "Commit versions are built from the epoch"). Each close SHALL record the epoch number and the
sequencer's wall-clock time in microseconds, which SHALL never be below the previous close's time even when the clock
steps back. An epoch number SHALL fit in 42 bits; the sequencer SHALL refuse to close past the largest 42-bit number
with an error naming the limit.

#### Scenario: Epochs close in order
- **WHEN** the sequencer runs for 100 ms with `epoch_ms_min` 5 and `epoch_ms_max` 10 on a stepped clock
- **THEN** it closes between 10 and 20 epochs, numbered consecutively from 1

#### Scenario: Epoch lengths out of their bounds stop the start
- **WHEN** the frontend starts once with `epoch_ms_min` 0, once with `epoch_ms_min` 1,001, once with `epoch_ms_min` 5
  and `epoch_ms_max` 4, and once with `epoch_ms_max` 10,001
- **THEN** each start exits with code 78 and an error naming the setting and its bound: 1, 1,000, `epoch_ms_min` and
  10,000

#### Scenario: A partition's request closes the epoch early
- **WHEN** `epoch_ms_min` is 2 and `epoch_ms_max` 10, epoch 6 closed 1 ms ago, and a partition asks for a close
- **THEN** the sequencer closes epoch 7 once 2 ms have passed since epoch 6 closed, not at 10 ms

#### Scenario: A waiting read closes the epoch early
- **WHEN** `epoch_ms_min` is 2 and `epoch_ms_max` 10, epoch 6 closed 1 ms ago, and a snapshot read waits on the open
  epoch 7
- **THEN** the frontend asks its sequencer for a close, and epoch 7 closes once 2 ms have passed since epoch 6 closed

#### Scenario: A clock stepping back does not lower close times
- **WHEN** the clock steps back by one second between two closes
- **THEN** the later close's recorded time equals the earlier close's time, not one second less

#### Scenario: The last epoch number is refused
- **WHEN** the sequencer's last closed epoch is 2^42 − 1 and it is asked to close the next
- **THEN** the close is refused with an error naming the 42-bit limit

### Requirement: A time maps to the epoch closed at or before it
`as of time t` SHALL map to the latest epoch whose recorded close time is at or below `t`; a time before the first
close SHALL be refused with an error naming the time and the first close's time.

#### Scenario: A time between two closes
- **WHEN** epoch 4 closed at 1,000 µs and epoch 5 at 2,000 µs, and the time 1,500 µs is mapped
- **THEN** the result is epoch 4

#### Scenario: A time before the first close is refused
- **WHEN** the first close was at 1,000 µs and the time 500 µs is mapped
- **THEN** the mapping fails with an error naming 500 µs and 1,000 µs

### Requirement: Partitions learn each closed epoch and stamp the open one
The sequencer SHALL deliver every close, in order, to every partition subscribed to it. A partition SHALL stamp its
commits with its open epoch, exactly one above the latest close delivered to it, and SHALL never advance its epoch on
its own (commit-rounds), so no commit is ever stamped with an epoch whose close has been delivered to the partition, and
the partition holds the catalog versions announced with its last delivered close when it stamps the epoch they take
effect at. A partition SHALL have closed an epoch once it has received that epoch's close and applied every round
stamped at or below it; a snapshot read at an epoch waits for exactly that (quad-reads). A partition that reconnects
SHALL receive the closes it missed, from its last delivered epoch.

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

### Requirement: Closes announce the versions taking effect after them
At each close `c + 1` the sequencer SHALL assign the effective epoch `c + 2` to every catalog version the metadata
group holds pending (metadata-group, "Catalog versions take effect at an epoch boundary") and to the oldest placement
version it holds pending, at most one per close (metadata-group, "The placement map assigns bucket ranges to
partitions"), record the assignments in the close's durable record, and deliver them with the close, which announces
them; the metadata group SHALL learn each version's effective epoch from that close. A version still pending when the frontend crashes before a close SHALL be
assigned at the next close after recovery. The close record SHALL likewise carry the session evictions the registry
made since the previous close (metadata-group, "The metadata group registers sessions"). A partition SHALL take a
catalog or placement version into effect from its assigned epoch, and therefore holds it before it can stamp that
epoch.

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
- **WHEN** a catalog version is committed as pending while epoch 38 is the latest close, and the frontend is killed
  before it closes epoch 39
- **THEN** after recovery the version is still pending, and the next close, of epoch 39, assigns it effective epoch 40

### Requirement: The sequencer orders multi-partition transactions in its log
The sequencer SHALL accept a multi-partition transaction, naming its partitions and the largest reply epoch among its
registrations (partitions, "A registration's epoch is at most the epoch of every write referencing it"), and SHALL place
it in the open epoch, in the order of acceptance: the open epoch is at or above every reply epoch, since a reply epoch
is one above a close the sequencer has already made; reading the log from an epoch SHALL return the transactions of that
epoch and every later one, in their order, each with its epoch. In this stage no partition executes them; the log is
read by its tests and by stage 8.

#### Scenario: Transactions keep their order and epoch
- **WHEN** transactions A and B are accepted in epoch 6 and C in epoch 7, and the log is read from epoch 6
- **THEN** it returns A and B with epoch 6 and C with epoch 7, in that order

#### Scenario: A transaction is placed at or above its reply epoch
- **WHEN** the latest close is 12 and a multi-partition transaction naming reply epoch 12 is accepted
- **THEN** the log places it in epoch 13

#### Scenario: A transaction naming no partition is refused
- **WHEN** a multi-partition transaction naming no partition is offered
- **THEN** it is refused with an error naming the empty partition list, and the log is unchanged

### Requirement: The sequencer is a log-backed state machine with one replica
The sequencer's state SHALL be a function of its log: each close and each accepted transaction SHALL be appended to its
log and acknowledged only once durable, and recovery SHALL rebuild the state by replaying the log, ending at the same
last closed epoch and transaction order as before the crash. In this stage it runs with one replica; its interface SHALL
be the one stage 8 replicates.

#### Scenario: Recovery rebuilds the sequencer
- **WHEN** the sequencer closes epochs 1 to 12, accepts three transactions, and is killed and restarted
- **THEN** its last closed epoch is 12, and reading its log returns the three transactions in their order

#### Scenario: An unacknowledged close is not delivered
- **WHEN** the sequencer is killed after appending a close and before the append is durable
- **THEN** no partition received that close before the crash, and after restart the close is either durable and
  delivered or absent
