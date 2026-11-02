## ADDED Requirements

### Requirement: The sequencer closes epochs at its own wall-clock time
The sequencer SHALL close epochs in order, numbered from 1, each at least `epoch_ms_min` and at most `epoch_ms_max`
after the previous close, both settings with no default the binary chooses silently. Each close SHALL record the epoch
number and the sequencer's wall-clock time in microseconds, which SHALL never be below the previous close's time even
when the clock steps back. An epoch number SHALL fit in 42 bits; the sequencer SHALL refuse to close past the largest
42-bit number with an error naming the limit.

#### Scenario: Epochs close in order
- **WHEN** the sequencer runs for 100 ms with `epoch_ms_min` 5 and `epoch_ms_max` 10 on a stepped clock
- **THEN** it closes between 10 and 20 epochs, numbered consecutively from 1

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
commits with its open epoch: one above the latest close delivered to it, or its own epoch when it has advanced further
on its own (commit-rounds), so no commit is ever stamped with an epoch already closed. A partition SHALL have closed an
epoch once it has received that epoch's close and applied every round stamped at or below it; a snapshot read at an
epoch waits for exactly that (quad-reads). A partition that reconnects SHALL receive the closes it missed, from its last
delivered epoch.

#### Scenario: A partition stamps the open epoch
- **WHEN** epochs 1 to 7 are delivered to a partition and it commits a transaction
- **THEN** the commit's version carries epoch 8

#### Scenario: A partition closes an epoch once its rounds are applied
- **WHEN** a partition holds an unapplied round of epoch 8 and the close of epoch 8 is delivered
- **THEN** the partition reports epoch 8 closed only after that round is applied, and a snapshot read at epoch 8 waits
  until then

#### Scenario: A reconnecting partition catches up
- **WHEN** a partition loses its connection after epoch 3 and reconnects after epoch 9 closed
- **THEN** it receives epochs 4 to 9 in order before any later close

### Requirement: The sequencer orders multi-partition transactions in its log
The sequencer SHALL accept a multi-partition transaction, naming its partitions, and SHALL place it in the open epoch,
in the order of acceptance; reading the log from an epoch SHALL return the transactions of that epoch and every later
one, in their order, each with its epoch. In this stage no partition executes them; the log is read by its tests and by
stage 8.

#### Scenario: Transactions keep their order and epoch
- **WHEN** transactions A and B are accepted in epoch 6 and C in epoch 7, and the log is read from epoch 6
- **THEN** it returns A and B with epoch 6 and C with epoch 7, in that order

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
