#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Vector index
  Dgraph's HNSW index from tok/hnsw, stored in posting lists: the entry point, every node's neighbour lists and the
  dead list, searched by the predicate's metric.

  Scenario: The first insert creates the entry point
    When the first vector is inserted into an empty index
    Then the entry point names its uid and its neighbour lists exist, empty, at every level

  Scenario: Deleting a vector marks it dead
    When a vector is deleted
    Then its uid is added to the dead list, and later inserts leave it out of the neighbour lists they write

  Scenario: Small sets return the exact top k for each metric
    When 50 vectors are indexed under each metric and a search asks for the 5 nearest to a query vector
    Then the result equals the 5 nearest by brute force under that metric

  Scenario: A distance threshold filters results
    When a search carries a distance threshold
    Then no returned vector lies farther than the threshold

  Scenario: An empty index returns nothing
    When an index holding no vectors is searched
    Then the index reports an empty-index result, which the caller treats as no neighbours, and the search returns an empty result without error

  Scenario: The exponent sets the parameters
    When an hnsw index is declared with `exponent: "4"`
    Then it uses 4 levels, an `efConstruction` of 200 and an `efSearch` of 120
