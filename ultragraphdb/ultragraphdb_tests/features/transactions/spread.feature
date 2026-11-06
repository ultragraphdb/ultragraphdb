# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Transactions over a spread tenant
  openspec/changes/stage-5-5-three-role-refactor/specs/transactions/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request.

  Scenario: A causally later commit is not stamped earlier
    When a session commits in partition 1 at epoch 20, and its next transaction, carrying that token, commits in partition 2 while partition 2's latest delivered close is 17
    Then partition 2 commits it only after close 19 is delivered, at epoch 20 or later

  Scenario: Inserts of the same new statement conflict
    When two transactions in `<a>`'s partition concurrently insert `<a> <knows> <b>`, which is not held, `<b>` new and homed in another partition
    Then both registrations of `<b>` yield one ID and neither aborts; one transaction commits and the other aborts with a conflict, and its retry finds the statement held

  Scenario: Creating a dictionary entry conflicts with nothing
    When a transaction in `<new>`'s home reads `<new>`'s statements, finds none, and writes `<x> <seen> true`, and a registration from another partition creates `<new>`'s dictionary entry there after the read version, with no statement of `<new>` committed in the home
    Then the reading transaction commits

  Scenario: A transaction reaching a second partition is not retryable
    When a transaction bound to partition 1 writes a statement whose subject's bucket partition 2 owns
    Then it aborts with a cross-partition error naming partitions 1 and 2, not marked retryable, and nothing it wrote is held

  Scenario: A statement named by its reifier is checked at its subject
    When a transaction bound to partition 1 sets a property of a statement named by its reifier, the statement's subject's bucket owned by partition 2
    Then it aborts with a cross-partition error naming partitions 1 and 2, not marked retryable, and nothing it wrote is held

  Scenario: A write reaching two partitions is refused
    When a DirectWrite writes subjects of partitions 1 and 2
    Then the frontend refuses it with code 23 before any relay, and neither is held

  Scenario: A ConditionalWrite claiming a value goes to its claim
    When a ConditionalWrite's changes name only new blank nodes as subjects, its operation is an insert-if-absent of `epi.id` `"x"`, and partition 2 owns the bucket of `"x"`
    Then the frontend relays it to partition 2, and its reply names the node

  Scenario: A claim owned by another partition is cross-partition
    When `epi.id` is declared unique, partition 2 owns the bucket of `"x"`, and a transaction bound to partition 1 upsert-and-links the entity with `epi.id` `"x"`
    Then the transaction aborts with a cross-partition error naming partitions 1 and 2, not marked retryable, and nothing it wrote is held
