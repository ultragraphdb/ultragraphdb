## ADDED Requirements

### Requirement: Commits are grouped into rounds
The commit step SHALL close a round when the previous round's log sync completes, or at once when no sync is in
progress, with no timer, holding the commit requests waiting then, bounded by the maximum frame and a transaction
count; requests past the bounds wait for the next round. A round SHALL be validated, logged,
synced, applied and acknowledged as a unit, in round order.

#### Scenario: A lone commit waits about one sync
- **WHEN** one transaction commits on an idle node
- **THEN** it forms a round of its own and is acknowledged after one log sync and its apply

#### Scenario: Commits arriving during a sync share the next round
- **WHEN** ten transactions commit while a round's sync is in progress
- **THEN** they are decided in the next round, written with one log append and one sync

### Requirement: Commit versions follow the clock
Each admitted transaction SHALL receive a commit version equal to the larger of the previous version plus 2 and twice
the commit step's clock in microseconds; versions SHALL be even and strictly increasing, and a round SHALL record its
first version and transaction count.

#### Scenario: A clock stepping back does not lower versions
- **WHEN** the commit step's clock steps back by one second between two rounds
- **THEN** the later round's versions are greater than the earlier round's

### Requirement: Validation is exact
Each transaction of a round SHALL be checked, in round order, against the writes committed above its read version and
against the writes of the transactions of its round admitted before it. An aborted transaction SHALL add no writes to
what later transactions are checked against. The decisions of a round SHALL be a function of its transactions, their
order and the state before it.

#### Scenario: A transaction is not aborted by another's aborted write
- **WHEN** in one round T1 aborts, and T2's reads conflict only with what T1 would have written
- **THEN** T2 commits

#### Scenario: Two runs of one round decide alike
- **WHEN** the same round is validated twice from the same state
- **THEN** both decide the same transactions committed and aborted

### Requirement: The conflict history is bounded by the oldest open read
The record of committed writes SHALL be pruned below the read version of the oldest open read-write transaction, which
the read-write limit bounds; read-only transactions are never validated and do not hold it back. A transaction whose
read version is below the pruned floor SHALL abort as too old.

#### Scenario: A transaction older than the history aborts
- **WHEN** the history has been pruned above a transaction's read version and the transaction commits
- **THEN** the commit fails with a retryable too-old error

### Requirement: Acknowledgement at the visibility watermark
A transaction SHALL be acknowledged only after its round is synced in the log and applied to the store and the posting
cache; the visibility watermark SHALL advance over the contiguous prefix of such rounds, and every transaction SHALL
begin at the watermark or above. A failed log sync SHALL acknowledge none of its round and SHALL stop every later write
until the node restarts.

#### Scenario: No acknowledged commit is invisible
- **WHEN** a transaction is acknowledged and another transaction begins at any time after
- **THEN** the second transaction reads the first one's writes

#### Scenario: A failed sync acknowledges nothing
- **WHEN** the log sync of a round fails
- **THEN** none of the round's transactions is acknowledged, and later commits are refused until restart

### Requirement: Catalog changes, drops and index builds are ordered in rounds
A catalog change SHALL be a write to its namespace's catalog range, so every read-write transaction of that namespace
whose read version is below it and that commits after it SHALL abort. A drop SHALL be a write of its whole key range,
aborting every read-write transaction whose read version is below it, that commits after it, and that read or wrote in
the range. The nodes a write changes during an index build SHALL be recorded by the commit step in commit order. A
predicate's implicit default entry, recorded by its first write, SHALL be a commit-time insert-if-absent on its catalog
key and not a catalog change: concurrent first writes of one predicate both commit, the first creating the entry and
the second finding it (decided 2026-10-27). A declaration remains a catalog change. A drop's range SHALL hold the
dropped predicate's catalog entry, so a drop aborts its namespace's transactions as a catalog change does, and a
round SHALL close after a drop and before a second declaration that starts an index build: a round holds at most one
build, and nothing aborts for it (decided 2026-10-28).

#### Scenario: Concurrent first writes of a predicate both commit
- **WHEN** two transactions of `ns` read before either commits, the first writes `<a> <score> 12` and the second
  `<b> <score> "high"`, and `score` was not declared
- **THEN** both commit, and the catalog of `ns` holds `score` with the default entry once

#### Scenario: A write after a build's start in the same round aborts
- **WHEN** a round admits a declaration of `ns` that starts an index build of `p`, and a later transaction of the same
  round, executed against the catalog before the declaration, writes `p` in `ns`
- **THEN** the later transaction aborts with a conflict, and its retry is indexed by the build

#### Scenario: A write executed against an old catalog aborts
- **WHEN** a transaction executes against the catalog of `ns`, and a declaration of `ns` commits before it
- **THEN** the transaction aborts with a conflict, and its retry is checked against the new declaration

#### Scenario: A write into a dropped predicate aborts
- **WHEN** a transaction inserts `<a> <p> <b>` and a drop of `p` commits after its read version
- **THEN** the transaction aborts, and nothing of `p` is held after the drop

#### Scenario: A write during an index build is in the built index
- **WHEN** a term index on `name` is being built and a transaction inserting `<c> <name> "pear"` commits during the
  build
- **THEN** once the build is swapped in, the term index of `name` lists `c` under `pear`

### Requirement: Rollups take no version from the commit step
A rollup SHALL read at the visibility watermark and write as `posting-lists` states, one above the newest commit version
it folds, which is odd and so never a commit version.

#### Scenario: A rollup writes between commit versions
- **WHEN** a list with deltas at 10 and 12 is rolled up
- **THEN** its complete version is written at 13, and the next commit's version is even
