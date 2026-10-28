# The scenarios of the transactions capability the commit step carries, verbatim from
# openspec/changes/stage-5-transactions-and-binary-protocol/specs/transactions/spec.md: transactions executed on a
# changeset applier over the simulated store, decided by the commit step; the sessions' scenarios run the pipeline over
# the write-ahead log on simulated storage.

Feature: Transactions decided by the commit step

  # Requirement: Transactions are strictly serializable

  Scenario: Write skew is refused
    When two transactions each read `on-call` of `alice` and `bob`, both `true`, and each sets a different one to `false`, concurrently
    Then one of them commits and the other aborts with a conflict

  # Requirement: Commit-time operations

  Scenario: Concurrent insert-if-absent yields one node
    When eight transactions concurrently insert-if-absent a node with `epi.id` `"x"`, declared unique
    Then all eight commit, all receive the same UID, and one node holds `epi.id` `"x"`

  Scenario: Concurrent upsert-and-link deduplicates and links atomically
    When eight transactions each create a memory `m1` to `m8` and concurrently upsert-and-link the entity with `epi.id` `"x"`, declared unique, each linking its memory to it by `HAS_ENTITY`
    Then all eight commit and receive the same UID, one node holds `epi.id` `"x"`, each memory holds `HAS_ENTITY` to that node, and the node's reverse list of `HAS_ENTITY` holds all eight memories

  Scenario: Properties for creation are written once
    When two transactions concurrently upsert-and-link `epi.id` `"y"`, one carrying the label `"first"` for creation and the other `"second"`
    Then the node holds the label of the transaction committed first and not the other, and only that transaction's reply says the node was created

  Scenario: An upsert-and-link in an aborted transaction writes nothing
    When a transaction upsert-and-links `epi.id` `"z"` and aborts with a conflict on another of its reads
    Then no node holds `epi.id` `"z"` and no link of the transaction is held

  Scenario: A failed compare-and-set writes nothing
    When a transaction inserts `<a> <seen> true` and compare-and-sets `status` of `a` from `"open"` to `"closed"` while `status` is `"done"`
    Then the transaction is refused reporting `"done"`, `status` of `a` stays `"done"`, and `<a> <seen> true` is not held

  Scenario: Counter adds commute
    When a hundred transactions concurrently add 1 to the counter `next-id`
    Then all commit and `next-id` has grown by 100

  # Requirement: Exactly-once commits through sessions

  Scenario: A retried commit commits once
    When a client commits request 7 of its session, loses the reply, and sends request 7 again
    Then it receives the same reply and commit version, and the transaction's writes are held once

  Scenario: An evicted session is told so
    When a session is evicted from the full table and its client sends its next request
    Then the request is refused with an error saying the session was evicted

  # Requirement: Aborts name their reason

  Scenario: A full commit queue answers busy
    When the commit queue is full and a transaction commits
    Then the commit fails with a retryable busy error, and nothing it wrote is held

  Scenario: A write set over the maximum frame is refused
    When a transaction's write set grows past 1 MiB and it commits
    Then the commit fails with a too-large error that is not marked retryable, and nothing it wrote is held
