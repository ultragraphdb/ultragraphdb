# The scenarios of the write-ahead-log capability whose decision names the store, verbatim from
# openspec/changes/stage-5-transactions-and-binary-protocol/specs/write-ahead-log/spec.md and, for the requirement stage
# 5.5 modifies ("Recovery replays the log above the store"), from
# openspec/changes/stage-5-5-three-role-refactor/specs/write-ahead-log/spec.md, bound again against the real store
# (7.9): the pipeline commits through the write-ahead log on simulated storage into a changeset applier over the
# simulated store, and recovery replays the log's rounds above the store's applied log position before the node starts.
# The write-ahead log's own suite binds them against a model of the store.

Feature: Recovery replays the log above the store

  Scenario: Rounds lost from the store are replayed
    When rounds 1 to 10 are synced in the log, the store had applied up to round 7, and power is lost
    Then after recovery the store holds rounds 1 to 10

  Scenario: A store ahead of the log refuses to start
    When the store's applied position lies beyond the log's tail
    Then the node refuses to start, naming the position

  Scenario: A store behind its checkpoint refuses to start
    When a checkpoint at position 40 has trimmed the log, and the store reopened has applied position 30
    Then the node refuses to start, naming 30 and 40, and replays nothing

  Scenario: A round never reaches the store before the log
    When the node loses power at any step of a round
    Then the store after recovery holds no write of a round the log does not hold

  Scenario: A reported fault loses no acknowledged round
    When the simulated storage fails a round's sync with an error, or loses power after a round was acknowledged
    Then after recovery every acknowledged round is present

  Scenario: Replay rebuilds drops and the session table
    When a store holds `p`'s keys and statements a delete by pattern will remove, a round holding a drop of `p` and the next round holding that delete by pattern and a commit of session `s`'s request 7 are applied, the store is restored to its state before them, and their records are replayed onto it
    Then no key of `p` and none of the removed statements is held, and a retry of `s`'s request 7 returns its stored reply without committing again

  Scenario: An unknown derivation version is refused
    When a record names a derivation version above the reader's
    Then replay fails with an error naming the version, and applies nothing of the record
