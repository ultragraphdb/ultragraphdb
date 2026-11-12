## ADDED Requirements

### Requirement: A close names a change of the node list
The sequencer's close record SHALL hold whether the node list changed since the previous close, and the close SHALL
deliver it as `nodes`, the same in every frame of one close. The list's version SHALL be the epoch of the close that
names its change. A storage process SHALL read the flag and do nothing for it.

#### Scenario: A close without a change names none
- **WHEN** no compute node joined, changed or left between two closes
- **THEN** the second close holds `nodes` 0

#### Scenario: The flag is durable with the close
- **WHEN** a close naming a change is appended, and the system group's host is killed and restarted
- **THEN** a subscriber reading from before that close receives it with `nodes` 1

## MODIFIED Requirements

### Requirement: The sequencer closes epochs at its own wall-clock time
The sequencer SHALL close epochs in order, numbered from 1, each at least `epoch_ms_min` and at most `epoch_ms_max`
after the previous close, both settings with no default the binary chooses silently. `epoch_ms_min` SHALL lie from 1 to
1,000: from 1 ms, epoch numbers of 42 bits last over a century, and up to 1 s, a waiting read or a full partition waits
at most that long for its requested close. `epoch_ms_max` SHALL lie from `epoch_ms_min` to 10,000: up to 10 s, a
snapshot never lags the latest commit by more than the read-write transaction limit, and twice it stays within
`close_request_ms`'s bound of 60,000 (commit-rounds). A value out of its bounds SHALL stop compute at start with exit
code 78 and an error naming the setting and the bound. It SHALL close before `epoch_ms_max`, as soon as `epoch_ms_min`
has passed, when compute hosting it asks because a snapshot read waits on the open epoch (quad-reads, "A read's
transaction names its level"), or when a partition asks because its sequence would run out (commit-rounds, "Commit
versions are built from the epoch"). Each close SHALL record the epoch number and the sequencer's wall-clock time in
microseconds, which SHALL never be below the previous close's time even when the clock steps back. An epoch number SHALL
fit in 42 bits; the sequencer SHALL refuse to close past the largest 42-bit number with an error naming the limit.

#### Scenario: Epochs close in order
- **WHEN** the sequencer runs for 100 ms with `epoch_ms_min` 5 and `epoch_ms_max` 10 on a stepped clock
- **THEN** it closes between 10 and 20 epochs, numbered consecutively from 1

#### Scenario: Epoch lengths out of their bounds stop the start
- **WHEN** compute starts once with `epoch_ms_min` 0, once with `epoch_ms_min` 1,001, once with `epoch_ms_min` 5
  and `epoch_ms_max` 4, and once with `epoch_ms_max` 10,001
- **THEN** each start exits with code 78 and an error naming the setting and its bound: 1, 1,000, `epoch_ms_min` and
  10,000

#### Scenario: A partition's request closes the epoch early
- **WHEN** `epoch_ms_min` is 2 and `epoch_ms_max` 10, epoch 6 closed 1 ms ago, and a partition asks for a close
- **THEN** the sequencer closes epoch 7 once 2 ms have passed since epoch 6 closed, not at 10 ms

#### Scenario: A waiting read closes the epoch early
- **WHEN** `epoch_ms_min` is 2 and `epoch_ms_max` 10, epoch 6 closed 1 ms ago, and a snapshot read waits on the open
  epoch 7
- **THEN** compute asks its sequencer for a close, and epoch 7 closes once 2 ms have passed since epoch 6 closed

#### Scenario: A clock stepping back does not lower close times
- **WHEN** the clock steps back by one second between two closes
- **THEN** the later close's recorded time equals the earlier close's time, not one second less

#### Scenario: The last epoch number is refused
- **WHEN** the sequencer's last closed epoch is 2^42 − 1 and it is asked to close the next
- **THEN** the close is refused with an error naming the 42-bit limit

### Requirement: Partitions learn each closed epoch and stamp the open one
The sequencer SHALL deliver every close, in order, to every subscriber: every partition, and every endpoint core of
compute, which takes from the closes the epochs, the catalog versions and the placement version its view holds
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

### Requirement: An early close is asked by CloseRequest from either role
The sequencer SHALL close its open epoch early when a CloseRequest names it, from a partition whose epoch is full, as
before, or from compute with `partition` 0, for a snapshot read waiting on the open epoch (quad-reads, "A read's
transaction names its level"). A CloseRequest naming an epoch already closed SHALL be answered and SHALL close nothing.

#### Scenario: Compute's request closes the open epoch
- **WHEN** epoch 12 is the latest close, and compute sends CloseRequest naming epoch 13 with `partition` 0
- **THEN** the sequencer closes epoch 13 before `epoch_ms_max` has passed, once `epoch_ms_min` has, and every
  subscriber receives the close

#### Scenario: A request for a closed epoch closes nothing
- **WHEN** epoch 13 is closed and a CloseRequest names epoch 13
- **THEN** it is answered, and epoch 14 closes at its own time
