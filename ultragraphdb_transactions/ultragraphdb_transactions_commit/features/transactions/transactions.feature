# The scenario of the transactions capability's "Transactions are strictly serializable" requirement, verbatim from
# openspec/changes/stage-5-transactions-and-binary-protocol/specs/transactions/spec.md: two transactions executed on a
# changeset applier over the simulated store, decided by the commit step.

Feature: Transactions decided by the commit step

  # Requirement: Transactions are strictly serializable

  Scenario: Write skew is refused
    When two transactions each read `on-call` of `alice` and `bob`, both `true`, and each sets a different one to `false`, concurrently
    Then one of them commits and the other aborts with a conflict
