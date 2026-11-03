#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Vector index
  Dgraph's HNSW index from tok/hnsw, stored in posting lists: the entry point, every node's neighbour lists and the
  dead list, searched by the predicate's metric, every node named by its 128-bit system ID. The scenarios of a
  declaration's parameters run in ultragraphdb_store_apply's features, where a declaration states its dimension.

  Scenario: The first insert creates the entry point
    When the first vector is inserted into an empty index
    Then the entry point names its node's system ID and its neighbour lists exist, empty, at every level

  Scenario: Deleting a vector marks it dead
    When a vector is deleted
    Then its node's ID is added to the dead list as 32 hexadecimal digits, and later inserts leave it out of the neighbour lists they write

  Scenario: Vector lists stay within their graph and predicate
    When two predicates in two graphs each hold vectors
    Then each predicate's HNSW lists are under its own graph and predicate, and a search of one finds none of the other's nodes

  Scenario: Small sets return the exact top k for each metric
    When 50 vectors are indexed under each metric and a search asks for the 5 nearest to a query vector
    Then the result equals the 5 nearest by brute force under that metric

  Scenario: A distance threshold filters results
    When a search carries a distance threshold
    Then no returned vector lies farther than the threshold

  Scenario: An empty index returns nothing
    When an index holding no vectors is searched
    Then the index reports an empty-index result, which the caller treats as no neighbours, and the search returns an empty result without error

  Scenario: A dead entry point hands over without wiping a node
    When the entry point's vector is deleted and another vector is inserted
    Then the first node holding a vector becomes the entry point with its neighbour lists kept, and the new vector is inserted
