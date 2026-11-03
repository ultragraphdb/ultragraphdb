# The scenarios of the transactions capability the commit step carries, verbatim from
# openspec/changes/stage-5-5-three-role-refactor/specs/transactions/spec.md: transactions executed on a changeset
# applier over the simulated store and decided by the commit step, which closes reach as inputs; a second partition is
# a second commit step with a store of its own. The sessions requirement's "An evicted session is told so" and "A session
# belongs to its principal" are the frontend's owner check against the metadata group's registry, which no partition
# runs: they are bound with the frontend (group 9). "A snapshot read waits for its epoch" is bound on the condition the
# read waits for, the epoch the partition reports closed; the read itself is compute's.

Feature: Transactions decided by the commit step

  # Requirement: Transactions read a snapshot at the visibility watermark

  Scenario: A snapshot read waits for its epoch
    When a read-only transaction at snapshot level begins while epoch 20 is the latest close, and one partition it reads has closed only epoch 19
    Then its read answers once that partition has closed epoch 20, at epoch 20

  # Requirement: Transactions are strictly serializable

  Scenario: Write skew is refused
    When two transactions each read `on-call` of `alice` and `bob`, both `true`, and each sets a different one to `false`, concurrently, all four facts in one partition
    Then one of them commits and the other aborts with a conflict

  # Requirement: What a transaction's reads record and what conflicts with them

  Scenario: Inserts of different objects into one list both commit
    When two transactions concurrently insert `<a> <knows> <b>` and `<a> <knows> <c>`
    Then both commit, and `a` knows `b` and `c`

  Scenario: Inserts of the same new statement conflict
    When two transactions in `<a>`'s partition concurrently insert `<a> <knows> <b>`, which is not held, `<b>` new and homed in another partition
    Then both registrations of `<b>` yield one ID and neither aborts; one transaction commits and the other aborts with a conflict, and its retry finds the statement held

  Scenario: A scanned range conflicts with an insert into it
    When a transaction scans the `int` index of `age` for values from 30 to 40 and writes `<x> <seen> true`, and a transaction inserting `<c> <age> 35` commits after its read version
    Then the scanning transaction aborts with a conflict

  Scenario: An insert outside a scanned range does not conflict
    When a transaction scans the `int` index of `age` for values from 30 to 40 and writes `<x> <seen> true`, and a transaction inserting `<c> <age> 50` commits after its read version
    Then the scanning transaction commits

  Scenario: A term found absent conflicts with a statement of it
    When a transaction reads `<new>`'s statements, finds none, and writes `<x> <seen> true`, and a transaction in the same partition writing `<new> <name> "N"` commits after its read version
    Then the reading transaction aborts with a conflict naming an absent term

  Scenario: Creating a dictionary entry conflicts with nothing
    When a transaction in `<new>`'s home reads `<new>`'s statements, finds none, and writes `<x> <seen> true`, and a registration from another partition creates `<new>`'s dictionary entry there after the read version, with no statement of `<new>` committed in the home
    Then the reading transaction commits

  # Requirement: Exactly-once commits through sessions

  Scenario: A retried commit commits once
    When a client commits request 7 of its session, loses the reply, and sends request 7 again
    Then it receives the same reply and commit version, and the transaction's writes are held once

  Scenario: A session commits through two partitions
    When a session commits request 1 in partition 1, request 2 in partition 2 and request 3 in partition 1, then loses the reply to request 3 and sends it again on a new connection
    Then all three commit once, partition 1 accepts request 3 after its last, 1, and the retry receives request 3's stored reply from partition 1

  Scenario: A partition purges an entry only after the registry's eviction
    When a session has committed in partition 1, and the registry evicts it
    Then partition 1 keeps the session's entry until the close carrying the eviction is delivered to it, and then purges it

  Scenario: A retry of a request never committed is told so
    When a client sends request 8 of its session again, with no transaction open for it, and the session's last committed request is 7
    Then the request is refused as not committed, `detail` 8, and nothing is committed

  # Requirement: Aborts name their reason

  Scenario: A full commit queue answers busy
    When the commit queue is full and a transaction commits
    Then the commit fails with a retryable busy error, and nothing it wrote is held

  Scenario: A write set over the Welcome's bound is refused
    When a transaction's write set grows past the Welcome's `max_write_set` and it commits
    Then the commit fails with a too-large error that is not marked retryable, its `detail` the bound, and nothing it wrote is held

  Scenario: A transaction reaching a second partition is not retryable
    When a transaction bound to partition 1 writes a statement whose subject's bucket partition 2 owns
    Then it aborts with a cross-partition error naming partitions 1 and 2, not marked retryable, and nothing it wrote is held

  Scenario: A statement named by its reifier is checked at its subject
    When a transaction bound to partition 1 sets a property of a statement named by its reifier, the statement's subject's bucket owned by partition 2
    Then it aborts with a cross-partition error naming partitions 1 and 2, not marked retryable, and nothing it wrote is held

  # Requirement: Commit-time operations

  Scenario: Concurrent insert-if-absent yields one node
    When eight transactions concurrently insert-if-absent a node with `epi.id` `"x"`, declared unique, all in the partition owning the bucket of `"x"`
    Then all eight commit, all receive the same IRI, and one node holds `epi.id` `"x"`

  Scenario: Concurrent upsert-and-link deduplicates and links atomically
    When eight transactions each create a memory `m1` to `m8` and concurrently upsert-and-link the entity with `epi.id` `"x"`, declared unique, each linking its memory to it by `HAS_ENTITY`, all in the partition owning the bucket of `"x"`
    Then all eight commit and receive the same IRI, one node holds `epi.id` `"x"`, each memory holds `HAS_ENTITY` to that node, and a reverse read of `HAS_ENTITY` for the node returns all eight memories

  Scenario: Properties for creation are written once
    When two transactions concurrently upsert-and-link `epi.id` `"y"`, one carrying the label `"first"` for creation and the other `"second"`, both in the partition owning the bucket of `"y"`
    Then the node holds the label of the transaction committed first and not the other, and only that transaction's reply says the node was created

  Scenario: An upsert-and-link in an aborted transaction writes nothing
    When a transaction in the partition owning the bucket of `"z"` upsert-and-links `epi.id` `"z"` and aborts with a conflict on another of its reads
    Then no node holds `epi.id` `"z"` and no link of the transaction is held

  Scenario: A failed compare-and-set writes nothing
    When a transaction inserts `<a> <seen> true` and compare-and-sets `status` of `a` from `"open"` to `"closed"` while `status` is `"done"`
    Then the transaction is refused reporting `"done"`, `status` of `a` stays `"done"`, and `<a> <seen> true` is not held

  Scenario: Counter adds commute
    When a hundred transactions concurrently add 1 to the counter `next-id`
    Then all commit and `next-id` has grown by 100

  Scenario: A counter add returns the counter's new value
    When the counter `next-id` holds 5, and two transactions each add 1 to it
    Then both commit, the one committed first receives 6 and the other 7, and `next-id` holds 7

  Scenario: A claim owned by another partition is cross-partition
    When `epi.id` is declared unique, partition 2 owns the bucket of `"x"`, and a transaction bound to partition 1 upsert-and-links the entity with `epi.id` `"x"`
    Then the transaction aborts with a cross-partition error naming partitions 1 and 2, not marked retryable, and nothing it wrote is held

  Scenario: A created node is named by its skolem IRI
    When a transaction in the partition owning the bucket of `"w"` insert-if-absents a node with `epi.id` `"w"`, declared unique, and no node holds it
    Then the reply names the created node by its skolem IRI, says it was created, and holds no system ID

  # Requirement: Commits answer a session token

  Scenario: A token names its commit
    When a transaction commits in partition 1 at position 40 with a version of epoch 12
    Then its reply carries the token (partition 1, position 40, epoch 12)

  Scenario: A read with a token sees the commit
    When a client commits `<a> <knows> <b>` and reads `<a>`'s statements with the commit's token from another connection
    Then the read returns `<a> <knows> <b>`

  Scenario: A causally later commit is not stamped earlier
    When a session commits in partition 1 at epoch 20, and its next transaction, carrying that token, commits in partition 2 while partition 2's latest delivered close is 17
    Then partition 2 commits it only after close 19 is delivered, at epoch 20 or later

  # Requirement: An ended transaction releases what it holds

  Scenario: A released entry never reaches the store
    When transaction T1 creates `<gone>`, transaction T2 writes `<gone>` while T1 is open, T1 aborts and T2 commits, and the store is discarded and the log replayed into an empty store
    Then T2's record carries `<gone>`'s entry, replay resolves `<gone>` to the ID T2 committed, and nothing T1 created is held
