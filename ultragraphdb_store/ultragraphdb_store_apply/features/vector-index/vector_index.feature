#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Vector index maintenance
  Index maintenance keeps a vector predicate's HNSW index and its dead list (Dgraph's posting/index.go), inserting
  through the index's traits, and a schema change refuses an HNSW option below 1. The index's own scenarios run in
  ultragraphdb_store_hnsw's features.

  Scenario: A delete by value marks the vector dead
    When a vector of `emb: float32vector @index(hnsw) .` is deleted by its value
    Then its uid is added to the dead list

  Scenario: Overwriting a vector does not leave its uid dead
    When a vector of `emb: float32vector @index(hnsw) .` is set over the vector it holds
    Then its uid is not in the dead list, and the index still finds it

  Scenario: A vector deleted and set again is live
    When a vector of `emb: float32vector @index(hnsw) .` is deleted and then set again
    Then its uid is not in the dead list, and the index still finds it

  Scenario: An HNSW option below 1 is refused at the schema change
    When `emb: float32vector @index(hnsw(maxLevels:"0")) .` is applied
    Then the change fails with `IndexFactory: hnsw issues this error: 'the vector index option maxLevels must be at least 1, got 0'`, and the schema does not hold `emb`
