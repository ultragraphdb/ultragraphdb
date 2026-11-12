## MODIFIED Requirements

### Requirement: A read's transaction names its level
A transaction SHALL name on Begin the level, the session token, if any, and the deadline its reads share. A read-write
transaction SHALL read its partition's latest state whatever the level, and a token SHALL make its commit carry an epoch
at or above the token's (transactions, "Commits answer a session token"). A read-only transaction SHALL read at its
level: the latest state of one partition, binding to the partition of its first read, a later read reaching another
partition refused; a snapshot at a closed epoch (the default), the latest epoch the sequencer had closed when the
transaction began or its session token's epoch if that is higher, one snapshot for all its reads, each waiting until
every partition it asks has closed that epoch (sequencer, "Partitions learn each closed epoch and stamp the open one");
or monotonic-fast, which quad-pattern reads may take since they are monotonic, each read answering at one epoch: the
lowest closed epoch among the partitions it touches, but never below its floor, the larger of the session token's epoch
and the highest epoch any read of the transaction has answered at when this read is sent, waiting only for a partition
that has not closed the floor; a reverse read reads `R(o)` at the home's latest closed epoch `e`, waiting until `e` is
at or above the floor, resolves its groups through the placement map at `e`, and has every member answer at `e`, waiting
until each has closed `e`. A transaction's monotonic-fast reads therefore never go back and always see its token's
commit, but need not share one snapshot. A read-only transaction carrying a token SHALL wait, within its deadline, until
the token's epoch has closed. A snapshot read waiting on the open epoch SHALL make the frontend ask the sequencer for an
early close, by CloseRequest naming the open epoch with `partition` 0 (sequencer, "An early close is asked by
CloseRequest from either role"). The frontend SHALL hold a latest-level transaction's later read until its first read's
first Result names the partition, or that read ends; a first read refused leaves the next read first.

#### Scenario: A read after a commit sees it through its token
- **WHEN** a client commits a write, receives a session token, and begins a transaction with that token whose read
  asks for the write
- **THEN** the read returns the write

#### Scenario: Two reads of one transaction share its snapshot
- **WHEN** a transaction begins at snapshot level, reads `<a>`'s statements, another transaction commits a new
  statement of `<a>`, and the first transaction reads `<a>`'s statements again
- **THEN** both reads return the same rows

#### Scenario: A monotonic-fast read does not wait for a lagging partition
- **WHEN** partition 2 has closed epoch 9 and partition 1 epoch 12, and a monotonic-fast read with its subject unbound
  runs over both
- **THEN** it answers at once, reading both partitions at epoch 9, the lower of their closed epochs

#### Scenario: Monotonic-fast reads never go back
- **WHEN** a monotonic-fast transaction's first read answers at epoch 9, and its second read touches only a partition
  whose latest closed epoch is 12
- **THEN** the second read answers at epoch 12 without waiting

#### Scenario: A monotonic-fast read never answers below the token
- **WHEN** a read-only monotonic-fast transaction carries a token of partition 1 at epoch 20, and its read touches
  partition 1, closed at 21, and partition 2, closed at 18
- **THEN** the read waits until partition 2 has closed epoch 20, then answers at epoch 20 and returns the token's commit

#### Scenario: A latest-state read across partitions is refused
- **WHEN** a transaction at latest-state level reads with its subject unbound over two partitions
- **THEN** the read is refused with an error naming the level and the two partitions

#### Scenario: A later latest-state read waits for the first read's partition
- **WHEN** a transaction at latest level sends two reads beside each other, the first binding it to partition 1
- **THEN** the frontend sends the second only after the first's first Result names partition 1, with `partition` 1, and
  compute refuses it with code 25 when it reaches another partition

#### Scenario: A first read refused leaves the next read first
- **WHEN** a transaction at latest level sends two reads, and the first is refused before any Result
- **THEN** the second goes out as the transaction's first read and binds it

#### Scenario: Concurrent monotonic-fast reads take the highest epoch answered
- **WHEN** a monotonic-fast transaction's read answered at epoch 12 and another at epoch 9, and a third is sent after
  both
- **THEN** the third read's floor is 12
