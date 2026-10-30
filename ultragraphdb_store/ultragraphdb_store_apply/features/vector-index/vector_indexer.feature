#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Vectors indexed in commit order, outside the transaction
  A transaction writing a vector commits it in its forward list alone. The vector indexer inserts committed vectors
  into the HNSW lists in commit order and records the version it has indexed; a search returns only nodes whose
  vector is visible at its snapshot. A node's level is drawn from a hash of the index's seed, the namespace, the
  predicate and the node's UID. Transactions here execute against one snapshot and are judged by the commit step's
  conflict rules.

  Scenario: Concurrent vector writes do not conflict
    When eight transactions concurrently write vectors to `emb` for eight different nodes
    Then all eight commit, and once the indexer has passed their versions a search finds each of them

  Scenario: A deleted vector is not returned
    When a vector is deleted and a search runs before the indexer has processed the delete
    Then the search does not return its node

  Scenario: Insertion order does not change levels
    When the same ten vectors are indexed once in one order and once in the reverse order
    Then each node is drawn the same level both times
