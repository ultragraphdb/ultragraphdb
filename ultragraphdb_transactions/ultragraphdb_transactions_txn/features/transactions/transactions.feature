# The scenarios of the transactions capability that a transaction on a worker carries, verbatim from
# openspec/changes/stage-5-transactions-and-binary-protocol/specs/transactions/spec.md. An acknowledged commit is one the
# changeset applier has applied, so the watermark has passed it; the commit step that acknowledges commits in rounds is
# group 7's.

Feature: Transactions on a worker

  # Requirement: Transactions read a snapshot at the visibility watermark

  Scenario: A read above the watermark is refused
    When a request names a read version above the visibility watermark
    Then it is refused at once with an error naming the version and the watermark, and the worker serves the next request

  Scenario: A transaction sees every commit acknowledged before it began
    When a commit setting `<a> <name> "A"` is acknowledged and a transaction then begins and reads `name` of `a`
    Then it reads `"A"`

  Scenario: A read-only transaction does not abort
    When a read-only transaction reads `name` of `a`, another transaction then commits a new `name` of `a`, and the read-only transaction ends
    Then it ends without an error, having read the value from before the commit

  # Requirement: A transaction reads its own writes

  Scenario: A read after a write sees it
    When a transaction inserts `<a> <knows> <b>` and then reads the objects of `knows` of `a`
    Then the read holds `b`

  # Requirement: Transactions have bounded lifetimes

  Scenario: A read-write transaction past its limit aborts
    When a read-write transaction begins, waits 11 s, and commits
    Then the commit fails with a retryable error naming the 10 s limit, and nothing it wrote is held

  # Requirement: Interactive transactions

  Scenario: An aborted transaction writes nothing
    When a transaction inserts `<a> <knows> <b>` in one request and aborts in the next
    Then `<a> <knows> <b>` is not held
