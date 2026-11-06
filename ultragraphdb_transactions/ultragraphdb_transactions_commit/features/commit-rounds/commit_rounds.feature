# The scenarios of the commit-rounds capability that the commit step's rounds, versions, validation and history carry,
# verbatim from openspec/changes/stage-5-transactions-and-binary-protocol/specs/commit-rounds/spec.md, and, for the
# requirements stage 5.5 adds or modifies ("Commit versions are built from the epoch", "Acknowledgement at the
# visibility watermark", and the round closing at a delivered close), from
# openspec/changes/stage-5-5-three-role-refactor/specs/commit-rounds/spec.md. Closes reach the commit step as inputs. The commit step
# runs as its step function: a driver stands in for the pipeline, counting the rounds it is asked to append and the
# syncs it completes, and applying each synced round to a changeset applier over the simulated store; the scenarios of
# acknowledgement at the watermark run the pipeline itself, over the write-ahead log on simulated storage.

Feature: Commit rounds

  # Requirement: Commits are grouped into rounds

  Scenario: A lone commit waits about one sync
    When one transaction commits on an idle node
    Then it forms a round of its own and is acknowledged after one log sync and its apply

  Scenario: Commits arriving during a sync share the next round
    When ten transactions commit while a round's sync is in progress
    Then they are decided in the next round, written with one log append and one sync

  # Requirement: Commit versions are built from the epoch

  Scenario: A version carries the open epoch
    When epoch 9 is the latest close delivered to a partition and it admits its first transaction since
    Then the version's upper 42 bits are 10 and its lower 22 bits are 0

  Scenario: Versions increase within an epoch
    When a partition admits three transactions in epoch 10
    Then their sequences are 0, 2 and 4

  Scenario: A full epoch asks for a close and waits
    When a partition's sequence in epoch 10 reaches 2^22 − 2 and it admits another transaction
    Then it asks the sequencer to close epoch 10, holds the transaction until the close of 10 is delivered, and stamps it with epoch 11 and sequence 0

  Scenario: A full epoch with the sequencer unreachable answers busy
    When a partition's sequence in epoch 10 is full, `close_request_ms` is 500, and its close request is still unanswered 500 ms after it was sent
    Then every transaction it holds or admits is refused as busy, marked retryable, writing nothing, until the close of 10 is delivered

  # Requirement: Validation is exact

  Scenario: A transaction is not aborted by another's aborted write
    When in one round T1 aborts, and T2's reads conflict only with what T1 would have written
    Then T2 commits

  Scenario: Two runs of one round decide alike
    When the same round is validated twice from the same state
    Then both decide the same transactions committed and aborted

  # Requirement: The conflict history is bounded by the oldest open read

  Scenario: A transaction older than the history aborts
    When the history has been pruned above a transaction's read version and the transaction commits
    Then the commit fails with a retryable too-old error

  # Requirement: Acknowledgement at the visibility watermark

  Scenario: No acknowledged commit is invisible
    When a transaction is acknowledged in a partition and a read-write transaction bound to that partition begins at any time after
    Then the second transaction reads the first one's writes

  Scenario: A failed sync acknowledges nothing
    When the log sync of a round fails
    Then none of the round's transactions is acknowledged, and later commits of its partition are refused until restart

  # Requirement: Catalog changes, drops and index builds are ordered in rounds

  Scenario: A round closes at a delivered close
    When a round is gathering transactions of epoch 10 and the close of 10 is delivered
    Then the round closes, and the next transaction is stamped with epoch 11 in a new round

  Scenario: Concurrent first writes of a predicate both commit
    When two transactions of `t1` read before either commits, the first writes `<a> <score> 12` and the second `<b> <score> "high"`, and `score` was not declared
    Then both commit, and the metadata group holds `score`'s default entry once

  Scenario: A write after a build's start in the same round aborts
    When a version of `t1` starting an index build of `p` takes effect at epoch 40, and a transaction executed against the version before it writes `p` in `t1` and commits at epoch 40
    Then the transaction aborts with a conflict naming the catalog, and its retry is indexed by the build

  Scenario: A write executed against an old catalog aborts
    When a transaction executes against catalog version 2 of `t1`, and commits at an epoch from which version 3 is in effect
    Then the transaction aborts with a conflict naming the catalog, and its retry is checked against version 3

  Scenario: Two builds taking effect at one epoch take two rounds
    When versions 3 and 4 of `t1` both take effect at epoch 40, each starting an index build
    Then the build of version 3 begins in one round and that of version 4 in a later round, and no transaction aborts for either

  Scenario: A crash during a build runs the backfill again
    When a build of a term index on `name` has started, its backfill is half written, and the process is killed before the build's completion record is durable
    Then recovery runs the backfill again from the build's start version, and once its completion record is durable the index lists every subject the data holds

  Scenario: A write into a dropped predicate aborts
    When a transaction inserts `<a> <p> <b>` and a drop of `p` commits after its read version
    Then the transaction aborts, and nothing of `p` is held after the drop

  Scenario: A write during an index build is in the built index
    When a term index on `name` is being built and a transaction inserting `<c> <name> "pear"` commits during the build
    Then once the build is swapped in, the term index of `name` lists `c` under `pear`

  Scenario: No catalog key is written to a partition
    When a predicate is declared and another is first written
    Then no partition's store holds a catalog key for either, and the metadata group holds both entries

  # Requirement: Rollups take no version from the commit step

  Scenario: A rollup writes between commit versions
    When a list with deltas at 10 and 12 is rolled up
    Then its complete version is written at 13, and the next commit's version is even
