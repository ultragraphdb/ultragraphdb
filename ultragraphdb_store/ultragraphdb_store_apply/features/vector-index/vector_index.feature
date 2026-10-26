#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Vector index maintenance
  Index maintenance keeps a vector predicate's HNSW index and its dead list (Dgraph's posting/index.go), inserting
  through the index's traits, and a declaration refuses an HNSW option below 1. The index's own scenarios run in
  ultragraphdb_store_hnsw's features.

  Scenario: A delete by value marks the vector dead
    When a vector of `emb`, declared single-valued with a vector index, is deleted by its value
    Then its node's UID is added to the dead list

  Scenario: Overwriting a vector does not leave its uid dead
    When a vector of `emb`, declared single-valued with a vector index, is written over the vector it holds
    Then its node's UID is not in the dead list, and a search for the new vector finds it

  Scenario: A vector deleted and set again is live
    When a vector of `emb`, declared single-valued with a vector index, is deleted and then written again
    Then its node's UID is not in the dead list, and later inserts keep it in their neighbour lists

  Scenario: An HNSW option below 1 is refused at the declaration
    When `emb` is declared with a vector index whose `maxLevels` is `0`
    Then the declaration fails with `IndexFactory: hnsw issues this error: 'the vector index option maxLevels must be at least 1, got 0'`, and the catalog does not hold `emb`
