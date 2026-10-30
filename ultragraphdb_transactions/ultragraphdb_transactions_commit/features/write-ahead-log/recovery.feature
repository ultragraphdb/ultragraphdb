# The scenarios of the write-ahead-log capability whose decision names the store, verbatim from
# openspec/changes/stage-5-transactions-and-binary-protocol/specs/write-ahead-log/spec.md, bound again against the real
# store (7.9): the pipeline commits through the write-ahead log on simulated storage into a changeset applier over the
# simulated store, and recovery replays the log's rounds above the store's applied version before the node starts.
# The write-ahead log's own suite binds them against a model of the store.

Feature: Recovery replays the log above the store

  Scenario: Rounds lost from the store are replayed
    When rounds 1 to 10 are synced in the log, the store had applied up to round 7, and power is lost
    Then after recovery the store holds rounds 1 to 10

  Scenario: A store ahead of the log refuses to start
    When the store's applied version belongs to no round in the log
    Then the node refuses to start, naming the version

  Scenario: A round never reaches the store before the log
    When the node loses power at any step of a round
    Then the store after recovery holds no write of a round the log does not hold

  Scenario: A reported fault loses no acknowledged round
    When the simulated storage fails a round's sync with an error, or loses power after a round was acknowledged
    Then after recovery every acknowledged round is present
