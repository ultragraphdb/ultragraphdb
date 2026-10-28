# The scenarios of the commit-rounds capability that the commit step's rounds, versions, validation and history carry,
# verbatim from openspec/changes/stage-5-transactions-and-binary-protocol/specs/commit-rounds/spec.md. The commit step
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

  # Requirement: Commit versions follow the clock

  Scenario: A clock stepping back does not lower versions
    When the commit step's clock steps back by one second between two rounds
    Then the later round's versions are greater than the earlier round's

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
    When a transaction is acknowledged and another transaction begins at any time after
    Then the second transaction reads the first one's writes

  Scenario: A failed sync acknowledges nothing
    When the log sync of a round fails
    Then none of the round's transactions is acknowledged, and later commits are refused until restart

  # Requirement: Catalog changes, drops and index builds are ordered in rounds

  Scenario: Concurrent first writes of a predicate both commit
    When two transactions of `ns` read before either commits, the first writes `<a> <score> 12` and the second `<b> <score> "high"`, and `score` was not declared
    Then both commit, and the catalog of `ns` holds `score` with the default entry once

  Scenario: A write after a build's start in the same round aborts
    When a round admits a declaration of `ns` that starts an index build of `p`, and a later transaction of the same round, executed against the catalog before the declaration, writes `p` in `ns`
    Then the later transaction aborts with a conflict, and its retry is indexed by the build

  Scenario: A write executed against an old catalog aborts
    When a transaction executes against the catalog of `ns`, and a declaration of `ns` commits before it
    Then the transaction aborts with a conflict, and its retry is checked against the new declaration

  Scenario: A write into a dropped predicate aborts
    When a transaction inserts `<a> <p> <b>` and a drop of `p` commits after its read version
    Then the transaction aborts, and nothing of `p` is held after the drop

  Scenario: A write during an index build is in the built index
    When a term index on `name` is being built and a transaction inserting `<c> <name> "pear"` commits during the build
    Then once the build is swapped in, the term index of `name` lists `c` under `pear`

  # Requirement: Rollups take no version from the commit step

  Scenario: A rollup writes between commit versions
    When a list with deltas at 10 and 12 is rolled up
    Then its complete version is written at 13, and the next commit's version is even
