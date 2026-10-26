#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: HNSW lists in the vector-index keyspace
  A predicate declared with a vector index keeps each vector as a `ugdb:float32vector` literal in its forward list,
  which the HNSW search reads, and its HNSW lists in the vector-index keyspace under its graph and predicate.

  Scenario: A vector of another dimension is refused
    When a vector index on `embedding` holds 768-element vectors and a 512-element vector is written to `embedding`
    Then the write fails with an error naming `embedding`, 768 and 512, and nothing of its changeset is held

  Scenario: The first insert creates the entry point
    When the first vector is inserted into an empty index
    Then the entry point names its node's UID and its neighbour lists exist, empty, at every level

  Scenario: Deleting a vector marks it dead
    When a vector is deleted
    Then its node's UID is added to the dead list, and later inserts leave it out of the neighbour lists they write

  Scenario: Vector lists stay within their graph and predicate
    When two predicates in two graphs each hold vectors
    Then each predicate's HNSW lists are under its own graph and predicate, and a search of one finds none of the other's nodes
