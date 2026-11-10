## ADDED Requirements

### Requirement: A round's header carries its flags, horizon and derivation version
Every Round a partition logs SHALL carry in its command header, beside its position, parent, incarnation, first version
and decision count, the fold horizon its primary stamped (tile-engine, "The fold horizon is logged by the primary and
folded at by every replica"), the derivation version it was applied under, a u64 computed over the `[tiles]` settings
(tile-grid, "The derivation version"), and its `flags`, whose bit 0, `checkpoint`, the primary sets while `GridFull`
holds its partition (tile-grid, "GridFull and its logged checkpoint round") and whose bits 1 to 7 are zero
(`specification/binary_protocol/PROTOCOL.md` section 12.1). A Round whose `flags` holds a bit other than bit 0 SHALL be
refused with `FieldValue` `"flags"`, and one whose `horizon` is neither 0, a version nor a version bound with
`FieldValue` `"horizon"`.

#### Scenario: A round names the settings it was applied under
- **WHEN** two storage instances with equal `[tiles]` tables each log a round
- **THEN** both rounds carry the same u64 `derivation`, the XXH3-64 that `STORAGE.md` section 3.2 computes from those
  settings

#### Scenario: A round flagged checkpoint is logged only under GridFull
- **WHEN** a partition's primary logs rounds while `free` stays at or above the reserve, and then a round while
  `GridFull` holds and its delta holds changes
- **THEN** only the last round's `flags` has bit 0 set

#### Scenario: An unknown flag is refused
- **WHEN** a Round's `flags` holds bit 3
- **THEN** it is refused with `FieldValue` naming `flags`, and nothing of it is applied

### Requirement: Execution derives what apply cannot compute
A partition's execution of a round's entries SHALL derive, and the round SHALL carry in its derivations section
(`PROTOCOL.md` section 12.1, section 6), every value apply needs that the entries' records do not hold: each count row's
degree before the round, one per count row the round's entries change, keyed by derivation kind 1 and the row's
conflict key; each `@limit` aggregate's change, one per aggregate the round changes, keyed by derivation kind 2; and each
statistics change, one per (graph, predicate) the round changes, keyed by derivation kind 3, each once and in ascending
key order. Execution SHALL take every prior value, absence proof and duplicate check these need from the state at its
read version, waiting for tiles where it must; apply SHALL add what the round carries and read no prior value
(tile-engine, "Apply reads the round and adds"). A derivation entry whose key or value breaks its kind's rule
(`PROTOCOL.md` section 12.1) SHALL refuse the Round with `FieldValue` `"derivation"`, and two entries out of ascending
key order with `FieldValue` `"derivation order"`.

#### Scenario: A count row's degree rides in the round
- **WHEN** `friend` is declared with a count index, `<a>` holds three objects of `friend`, and a round adds two more
- **THEN** the round's derivations section holds one entry of kind 1 for `<a>`'s subject count of `friend` with degree
  3, and after apply the count index holds `<a>` under 5 and not under 3

#### Scenario: A statistics change is carried once per graph and predicate
- **WHEN** a round's two transactions each insert a new statement of `name` in the default graph
- **THEN** the round's derivations section holds one entry of kind 3 for the default graph and `name`, counting two
  statements

#### Scenario: Replay adds the carried derivations
- **WHEN** a round holding derivations is applied, the store is discarded, and the log is replayed into an empty store
- **THEN** the count rows, statistics and `@limit` aggregates read the same after the replay as after the first apply

### Requirement: A round logs the sessions evicted at it
The first round a partition logs after an epoch close carrying session evictions is delivered to it SHALL name, in its
evictions section, each session the close evicted, as 16 bytes each in ascending order, and every other round's
evictions section SHALL be empty (`PROTOCOL.md` sections 11.5 and 12.1, section 7). Its evictions SHALL take effect at
its `first_version`, before its entries, whether or not it commits an entry (partitions, "The session table is a
record run keyed by the session"). An eviction of all zero bytes, or one not above the one before it, SHALL refuse the
Round with `FieldValue` `"eviction"`.

#### Scenario: The first round after the close names the evictions
- **WHEN** the close of epoch 30, evicting sessions `s1` and `s2`, is delivered to a partition, which then logs two
  rounds
- **THEN** the first round's evictions section names `s1` and `s2` in ascending order, and the second's is empty

#### Scenario: A round committing nothing still evicts
- **WHEN** a close evicting session `s` is delivered to a partition holding `s`'s entry, and the next round it logs
  holds only aborted entries
- **THEN** that round names `s` in its evictions, and the partition's table holds no entry for `s` after it

### Requirement: An Ingest share is an entry of its round
A committed Ingest share SHALL be an entry of its round as a transaction is: one decision in section 0, one record table
entry, and the commit version `first_version + 2k` of the k-th committed entry. Its record table entry SHALL name its
body in section 4 and its resolution in section 5 in place of a write-set record (ingest, "A share's round logs its body
as received with its resolution"), and the first body SHALL start at section 4's offset 0, each later one at the least
offset at or above the end of the one before whose frame offset is a multiple of 4,096, the bytes between zero
(`PROTOCOL.md` section 12.1).

#### Scenario: A share and a transaction share a round
- **WHEN** one round commits a transaction and then an Ingest share
- **THEN** the share's decision follows the transaction's, its record table entry names a body in section 4 at a frame
  offset that is a multiple of 4,096 and a resolution in section 5, and its commit version is `first_version + 2`

## MODIFIED Requirements

### Requirement: Acknowledgement at the visibility watermark
A transaction SHALL be acknowledged only after its round is synced in its partition's log and applied to its partition's
engine (tile-engine, "Apply reads the round and adds"); each partition's visibility watermark SHALL advance over the
contiguous prefix of such rounds, and every read-write transaction SHALL begin at its partition's watermark or above. A
read-only transaction SHALL read at its level (transactions, "Transactions read a snapshot at the visibility
watermark"), which may lag the watermark. A failed log sync SHALL acknowledge none of its round and SHALL stop every
later write of its partition until the node restarts.

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
everything in a tenant, logged as the last record of its round with the conflict ranges it covers (`PROTOCOL.md`
section 12.2), which the partition applies as a drop marker on apply and on replay (tile-engine, "Drops and purges are
logged markers that reads honour and compaction removes"), aborting every read-write transaction whose read version is
below it, that commits after it, and that read or wrote what it removes; removing a dropped predicate's declaration
SHALL be a catalog change, and until it takes effect the partition SHALL maintain no index of the predicate. An index
build a catalog version starts SHALL begin in each partition at the version's epoch: its start and its completion, a
swap or an undo, SHALL be logical records in the partition's log, and its build record (`STORAGE.md` section 9.3, kind
15) SHALL hold its start version and the last key its backfill reached, updated at each checkpoint; its backfill SHALL
read the data at the build's start version, which the fold horizon stays at or below while the build runs, and the
changes the commit step commits during the build SHALL reach the built index through apply; a crash that interrupts a
build SHALL make recovery resume the backfill strictly after the key its record holds at the last checkpoint
(tile-engine, "Index builds run beside compaction and resume from their record"), so the log stays the only synced
stream. A round SHALL close after a drop and before the first transaction of an epoch at which a version starting an
index build takes effect, and a round SHALL hold at most one build: builds that versions taking effect at one epoch
start SHALL begin in the order of their versions, each in a round of its own, and nothing aborts for them.

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

#### Scenario: A crash during a build resumes from its record
- **WHEN** a build of a term index on `name` has started, a checkpoint holds its build record naming the last key its
  backfill reached, and the process is killed before the build's completion record is durable
- **THEN** recovery resumes the backfill strictly after that key at the build's start version, and once its completion
  record is durable the index lists every subject the data holds

#### Scenario: A drop is the last record of its round
- **WHEN** a round gathering two transactions admits a Drop of `p` and then a third transaction
- **THEN** the Drop's record is the last committed record of the round, and the third transaction is decided in the
  next round

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

### Requirement: Commit versions are built from the epoch
Each admitted transaction SHALL receive a commit version whose upper 42 bits are its partition's open epoch (sequencer,
"Partitions learn each closed epoch and stamp the open one") and whose lower 22 bits are an even sequence within that
epoch, strictly increasing within the partition. The open epoch SHALL be exactly one above the latest close delivered to
the partition, and the partition SHALL never advance it on its own, so it never stamps an epoch whose close has been
delivered to it, and it holds the catalog versions announced with its last delivered close when it stamps the epoch they
take effect at. A registration a home accepts (term-dictionary, "A reference to another partition's entity registers at
its home") SHALL be admitted into its commit step as a transaction is and take its own commit version. A round SHALL
record its first version and transaction count. A round holding no committed entry, one holding only evictions or only
aborts, or one flagged `checkpoint` with no entry, SHALL record as its first version the epoch's next unused version,
the one its first committed entry would have taken, and SHALL consume that version for nothing else (`PROTOCOL.md`
section 12.1). When the sequence of the open epoch would pass its 22 bits, the partition SHALL ask the sequencer to
close the epoch and SHALL hold its admitted transactions until that close is delivered, then continue from sequence 0
of the next epoch. The sequencer SHALL count as unreachable once a close request has gone unanswered for
`close_request_ms`, a required setting from 1 to 60,000 that the partition's role SHALL refuse at start, with exit code
78, when it is below twice `epoch_ms_max`; from then until the close is delivered the partition SHALL refuse each held
and newly admitted transaction as busy, retryable, writing nothing.

#### Scenario: A version carries the open epoch
- **WHEN** epoch 9 is the latest close delivered to a partition and it admits its first transaction since
- **THEN** the version's upper 42 bits are 10 and its lower 22 bits are 0

#### Scenario: Versions increase within an epoch
- **WHEN** a partition admits three transactions in epoch 10
- **THEN** their sequences are 0, 2 and 4

#### Scenario: A round with no committed entry consumes a version
- **WHEN** a partition has committed sequences 0 and 2 in epoch 10, logs a round holding no committed entry, and then
  commits a transaction in epoch 10
- **THEN** the empty round's first version is epoch 10, sequence 4, and the transaction's is sequence 6

#### Scenario: A full epoch asks for a close and waits
- **WHEN** a partition's sequence in epoch 10 reaches 2^22 − 2 and it admits another transaction
- **THEN** it asks the sequencer to close epoch 10, holds the transaction until the close of 10 is delivered, and
  stamps it with epoch 11 and sequence 0

#### Scenario: A full epoch with the sequencer unreachable answers busy
- **WHEN** a partition's sequence in epoch 10 is full, `close_request_ms` is 500, and its close request is still
  unanswered 500 ms after it was sent
- **THEN** every transaction it holds or admits is refused as busy, marked retryable, writing nothing, until the close
  of 10 is delivered

## REMOVED Requirements

### Requirement: Rollups take no version from the commit step
**Reason**: The tile store keeps no posting lists and runs no rollup. Compaction folds entries at the logged fold
horizon, giving an entry created at or below it created version 0, and writes no version of its own
(`specification/storage/STORAGE.md` sections 14.5 and 16.3), so no write outside the commit step takes a version.
**Migration**: Folding is tile-engine's "Folding removes what the horizon passed"; commit versions remain the commit
step's alone, as "Commit versions are built from the epoch" states.
