#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Vector index maintenance
  The vector indexer keeps a vector predicate's HNSW index and its dead list (Dgraph's posting/index.go), inserting
  through the index's traits; the steps run its pass after each write. A declaration states its dimension and refuses
  an HNSW option below 1. The index's own scenarios run in ultragraphdb_store_hnsw's features.

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
    When `emb` is declared with a vector index of dimension 4 whose `maxLevels` is `0`
    Then the declaration fails with `IndexFactory: hnsw issues this error: 'the vector index option maxLevels must be at least 1, got 0'`, and the catalog does not hold `emb`

  Scenario: The exponent sets the parameters
    When an hnsw index is declared with dimension 4 and `exponent: "4"`
    Then it uses 4 levels, an `efConstruction` of 200 and an `efSearch` of 120

  Scenario: A declaration without a dimension is refused
    When `emb` is declared with a vector index and no dimension
    Then the declaration fails with an error naming `emb` and the missing dimension, and the catalog does not hold `emb`
