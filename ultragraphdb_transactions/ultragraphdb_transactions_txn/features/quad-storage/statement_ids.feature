# The scenario of quad-storage's "Statement IDs" a transaction carries, verbatim from
# openspec/changes/stage-5-5-three-role-refactor/specs/quad-storage/spec.md. The code and detail are what the refusal answers with;
# the stream a partition's host answers it on is its own.

Feature: Statement IDs in a transaction

  Scenario: An object past its statement bound is refused
    When `<a> <knows> <b>` is held under `statements_per_object_max` statement IDs, and a transaction inserts one more parallel edge of it
    Then the Write is refused with code 6, `detail` the bound, on the transaction's stream, the transaction ends, and the object's postings are unchanged
