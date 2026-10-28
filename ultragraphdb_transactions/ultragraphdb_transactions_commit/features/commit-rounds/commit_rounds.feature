# The scenarios of the commit-rounds capability that the commit step's rounds, versions, validation and history carry,
# verbatim from openspec/changes/stage-5-transactions-and-binary-protocol/specs/commit-rounds/spec.md. The commit step
# runs as its step function: a driver stands in for the pipeline, counting the rounds it is asked to append and the
# syncs it completes, and applying each synced round to a changeset applier over the simulated store. The scenarios of
# acknowledgement at the watermark, of catalog changes, drops and index builds, and of rollups are bound with the
# pipeline.

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
