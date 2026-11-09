## MODIFIED Requirements

### Requirement: Acknowledgement at the visibility watermark
A transaction SHALL be acknowledged only after its round is synced in its partition's log and applied to the store and
the posting cache; each partition's visibility watermark SHALL advance over the contiguous prefix of such rounds, and
every read-write transaction SHALL begin at its partition's watermark or above. A read-only transaction SHALL read at
its level (transactions, "Transactions read a snapshot at the visibility watermark"), which may lag the watermark. A
failed log sync SHALL acknowledge none of its round and SHALL stop every later write of its partition until the node
restarts.

#### Scenario: No acknowledged commit is invisible
- **WHEN** a transaction is acknowledged in a partition and a read-write transaction bound to that partition begins at
  any time after
- **THEN** the second transaction reads the first one's writes

#### Scenario: A failed sync acknowledges nothing
- **WHEN** the log sync of a round fails
- **THEN** none of the round's transactions is acknowledged, and later commits of its partition are refused until
  restart

### Requirement: Catalog changes, drops and index builds are ordered in rounds
A partition SHALL hold no catalog keys: a catalog change SHALL be a command of the metadata group, taking effect at the
epoch the sequencer assigns its version (metadata-group, "Catalog versions take effect at an epoch boundary"). A round
SHALL close when a close is delivered to the partition, so every round's commits carry one epoch and are validated and
applied against the one catalog version in effect at it. A read-write transaction that executed against a catalog
version other than the one in effect at its commit epoch SHALL abort with a conflict naming the catalog, and its retry
SHALL execute against the version in effect. A predicate the version in effect does not declare SHALL be validated with
the default entry, and its first write SHALL ask the metadata group to record that entry as a default-entry record
(metadata-group, "Catalog versions take effect at an epoch boundary"), a command that finds the entry when it already
exists and that no write waits for, so concurrent first writes of one predicate both commit; recording a default entry
SHALL abort no transaction. A drop SHALL be an operation naming its target, a predicate, a graph, a tenant's data or
everything in a tenant, whose key removals the partition derives on apply and on replay, aborting every read-write
transaction whose read version is below it, that commits after it, and that read or wrote what it removes; removing a
dropped predicate's declaration SHALL be a catalog change, and until it takes effect the partition SHALL maintain no
index of the predicate. An index build a catalog version starts SHALL begin in each partition at the version's epoch:
its start and its completion, the swap, SHALL be logical records in the partition's log; its backfill SHALL be derived
from the data at the build's start version, and the nodes a write changes during the build SHALL be recorded by the
commit step in commit order; a crash that interrupts a build SHALL make recovery run the backfill again from the start
version, so the log stays the only synced stream. A round SHALL close after a drop and before the first transaction of
an epoch at which a version starting an index build takes effect, and a round SHALL hold at most one build: builds that
versions taking effect at one epoch start SHALL begin in the order of their versions, each in a round of its own, and
nothing aborts for them.

#### Scenario: Concurrent first writes of a predicate both commit
- **WHEN** two transactions of `t1` read before either commits, the first writes `<a> <score> 12` and the second
  `<b> <score> "high"`, and `score` was not declared
- **THEN** both commit, and the metadata group holds `score`'s default entry once

#### Scenario: A write after a build's start in the same round aborts
- **WHEN** a version of `t1` starting an index build of `p` takes effect at epoch 40, and a transaction executed
  against the version before it writes `p` in `t1` and commits at epoch 40
- **THEN** the transaction aborts with a conflict naming the catalog, and its retry is indexed by the build

#### Scenario: A write executed against an old catalog aborts
- **WHEN** a transaction executes against catalog version 2 of `t1`, and commits at an epoch from which version 3 is
  in effect
- **THEN** the transaction aborts with a conflict naming the catalog, and its retry is checked against version 3

#### Scenario: Two builds taking effect at one epoch take two rounds
- **WHEN** versions 3 and 4 of `t1` both take effect at epoch 40, each starting an index build
- **THEN** the build of version 3 begins in one round and that of version 4 in a later round, and no transaction aborts
  for either

#### Scenario: A round closes at a delivered close
- **WHEN** a round is gathering transactions of epoch 10 and the close of 10 is delivered
- **THEN** the round closes, and the next transaction is stamped with epoch 11 in a new round

#### Scenario: A crash during a build runs the backfill again
- **WHEN** a build of a term index on `name` has started, its backfill is half written, and the process is killed
  before the build's completion record is durable
- **THEN** recovery runs the backfill again from the build's start version, and once its completion record is
  durable the index lists every subject the data holds

#### Scenario: A write into a dropped predicate aborts
- **WHEN** a transaction inserts `<a> <p> <b>` and a drop of `p` commits after its read version
- **THEN** the transaction aborts, and nothing of `p` is held after the drop

#### Scenario: A write during an index build is in the built index
- **WHEN** a term index on `name` is being built and a transaction inserting `<c> <name> "pear"` commits during the
  build
- **THEN** once the build is swapped in, the term index of `name` lists `c` under `pear`

#### Scenario: No catalog key is written to a partition
- **WHEN** a predicate is declared and another is first written
- **THEN** no partition's store holds a catalog key for either, and the metadata group holds both entries

## REMOVED Requirements

### Requirement: Commit versions follow the clock
**Reason**: Versions are built from the sequencer's epochs, so a snapshot at an epoch is one bound on the version in
every partition (P13, P24).
**Migration**: "Commit versions are built from the epoch".

## ADDED Requirements

### Requirement: Commit versions are built from the epoch
Each admitted transaction SHALL receive a commit version whose upper 42 bits are its partition's open epoch (sequencer,
"Partitions learn each closed epoch and stamp the open one") and whose lower 22 bits are an even sequence within that
epoch, strictly increasing within the partition. The open epoch SHALL be exactly one above the latest close delivered to
the partition, and the partition SHALL never advance it on its own, so it never stamps an epoch whose close has been
delivered to it, and it holds the catalog versions announced with its last delivered close when it stamps the epoch they
take effect at. A registration a home accepts (term-dictionary, "A reference to another partition's entity registers at
its home") SHALL be admitted into its commit step as a transaction is and take its own commit version. A round SHALL
record its first version and transaction count. When the sequence of the open epoch would pass its 22 bits, the
partition SHALL ask the sequencer to close the epoch and SHALL hold its admitted transactions until that close is
delivered, then continue from sequence 0 of the next epoch. The sequencer SHALL count as unreachable once a close
request has gone unanswered for `close_request_ms`, a required setting from 1 to 60,000 that the partition's role SHALL
refuse at start, with exit code 78, when it is below twice `epoch_ms_max`; from then until the close is delivered the
partition SHALL refuse each held and newly admitted transaction as busy, retryable, writing nothing.

#### Scenario: A version carries the open epoch
- **WHEN** epoch 9 is the latest close delivered to a partition and it admits its first transaction since
- **THEN** the version's upper 42 bits are 10 and its lower 22 bits are 0

#### Scenario: Versions increase within an epoch
- **WHEN** a partition admits three transactions in epoch 10
- **THEN** their sequences are 0, 2 and 4

#### Scenario: A full epoch asks for a close and waits
- **WHEN** a partition's sequence in epoch 10 reaches 2^22 − 2 and it admits another transaction
- **THEN** it asks the sequencer to close epoch 10, holds the transaction until the close of 10 is delivered, and
  stamps it with epoch 11 and sequence 0

#### Scenario: A full epoch with the sequencer unreachable answers busy
- **WHEN** a partition's sequence in epoch 10 is full, `close_request_ms` is 500, and its close request is still
  unanswered 500 ms after it was sent
- **THEN** every transaction it holds or admits is refused as busy, marked retryable, writing nothing, until the close
  of 10 is delivered
