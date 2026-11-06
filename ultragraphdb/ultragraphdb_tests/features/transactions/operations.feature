# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Commit-time operations through the three binaries
  openspec/changes/stage-5-5-three-role-refactor/specs/transactions/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request.

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

  Scenario: Counter adds commute
    When a hundred transactions concurrently add 1 to the counter `next-id`
    Then all commit and `next-id` has grown by 100
