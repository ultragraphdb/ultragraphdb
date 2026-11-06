#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: A schema change takes effect at an epoch boundary, partition by partition
  A catalog version takes effect in every partition from the epoch the sequencer assigns it, a partition stamping that
  epoch only once it holds the version; each partition builds a new index on its own and swaps it in with its own
  completion record. The partitions are two commit steps over stores of their own; the sequencer's closes are inputs.

  Scenario: Each partition swaps on its own
    When an `exact` index is declared over two partitions, and partition 1's build completes before partition 2's
    Then queries reading partition 1 use the new declaration, queries reading partition 2 the interim one until its completion record is durable, and neither swap writes a catalog key

  Scenario: Two partitions switch at the same epoch
    When an index is added while epoch 48 is the latest close, over two partitions, and the sequencer closes epoch 49
    Then the change takes effect at epoch 50, and in both partitions rounds of epoch 49 maintain no entries of the index and rounds of epoch 50 do

  Scenario: A partition holds the version before it stamps its epoch
    When an index is added while epoch 48 is the latest close, and a partition's latest delivered close is 48
    Then the partition's commits carry epoch 49 until the close of 49, carrying the new version, is delivered to it, and its first commit at epoch 50 is validated against the new version

  Scenario: A commit behind a version's round is validated against it
    When a transaction executed against catalog version 2 commits behind the round taking version 3 effect, before the close announcing version 3 is delivered
    Then it aborts with a conflict of kind Catalog, and a transaction executed against version 3 commits

  Scenario: A commit-time operation against an earlier version aborts
    When a transaction with a commit-time operation, executed against catalog version 2, commits once version 3 is in effect
    Then it aborts with a conflict of kind Catalog, and its operation runs not at all
