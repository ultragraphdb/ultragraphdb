# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A schema change at an epoch boundary
  The scenarios of "A schema change takes effect at an epoch boundary"
  (openspec/changes/stage-5-5-three-role-refactor/specs/schema-changes/spec.md), verbatim. Bound on the group's side
  only: the index is added as a new catalog version the group commits, the real sequencer closes the epochs and
  announces it, and the two partitions are models that learn each delivered close, stamp the open epoch and validate a
  round against the version the group's `CatalogSource` answers at the round's epoch, an index's entries maintained by
  a round exactly when that version declares it. The index maintenance itself is group 7's partition code.

  Rule: A schema change takes effect at an epoch boundary

    Scenario: Two partitions switch at the same epoch
      When an index is added while epoch 48 is the latest close, over two partitions, and the sequencer closes epoch 49
      Then the change takes effect at epoch 50, and in both partitions rounds of epoch 49 maintain no entries of the index and rounds of epoch 50 do

    Scenario: A partition holds the version before it stamps its epoch
      When an index is added while epoch 48 is the latest close, and a partition's latest delivered close is 48
      Then the partition's commits carry epoch 49 until the close of 49, carrying the new version, is delivered to it, and its first commit at epoch 50 is validated against the new version
