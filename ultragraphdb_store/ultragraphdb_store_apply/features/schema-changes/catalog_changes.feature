#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Catalog changes, rebuilds and drops
  A declaration that adds an index to a predicate holding data builds it from the data in the background, writes
  maintaining the previous declaration's indexes and the new one's meanwhile, and swaps it in with one batch; a
  declaration the data contradicts is refused; drops remove what they drop physically, in every keyspace and graph.

  Scenario: A new index covers existing data
    When a predicate holds 1,000 literals and an `exact` index is declared on it
    Then after the rebuild, the index holds a token for each of the 1,000 literals

  Scenario: Queries use the interim declaration during a rebuild
    When a predicate indexed by `term` is declared with `exact` instead, and the `exact` index is being built
    Then the declaration used for queries reports the predicate with neither `term` nor `exact`

  Scenario: Writes during a rebuild are indexed
    When a literal is written while its predicate's index is rebuilt
    Then after the rebuild, the index holds the literal's tokens

  Scenario: Writes during a rebuild leave no entry of a removed index
    When `name` indexed by `term` is declared with `exact` instead, and `<b> <name> "pear"` is written while the `exact` index is built
    Then after the rebuild, no term key of `name` remains, and the exact key for `pear` holds `b`

  Scenario: Removing one tokenizer keeps another
    When a predicate indexed by `term` and `exact` drops `term`
    Then its term keys are gone and its exact keys remain

  Scenario: A multi-valued predicate holding two objects cannot become single-valued
    When `a` holds two objects of `nick` and `nick` is declared single-valued
    Then the change fails with an error naming `nick` and `a`, and `nick` stays multi-valued

  Scenario: A predicate holding literals cannot become an edge
    When `name` holds `"Ann"` for `a` and `name` is declared an edge
    Then the change fails with an error naming `name` and `a`

  Scenario: A datatype constraint matching the data is applied
    When `age` holds only `xsd:integer` literals and is declared with datatype `xsd:integer`
    Then the change is applied

  Scenario: Deltas summing to nothing hold no data
    When `friend` gained `<a> <friend> <b>` at 3 and lost it at 4, and is declared a property at 5
    Then the change is applied

  Scenario: Dropping a predicate removes it from the predicate lists
    When `knows` is held for `a` and `b` and is dropped
    Then no key of `knows` remains, and neither the subject predicates of `a` nor of `b` holds `knows`

  Scenario: Dropping a vector predicate drops its vector index
    When `emb` is declared with a vector index, holds vectors, and is dropped
    Then no forward or vector-index key of `emb` remains

  Scenario: Dropping a graph keeps the other graphs
    When graphs `<g1>` and `<g2>` hold statements and `<g1>` is dropped
    Then no key of `<g1>` remains, and every statement of `<g2>` is still held

  Scenario: Dropping data keeps the catalog
    When a namespace's data is dropped
    Then its predicates hold no data and its catalog is unchanged

  Scenario: Dropping a namespace's everything survives a restart
    When everything in namespace 3 is dropped and the store is reopened
    Then namespace 3 holds only its initial catalog

  Scenario: Dropping everything leaves the initial catalog
    When everything is dropped
    Then the store holds no quad key but the root namespace's initial catalog, written at timestamp 1

  Scenario: Dropping a namespace's data leaves no split part
    When a namespace whose lists are split has its data dropped
    Then no key of any quad keyspace of the namespace remains, split or not, and its catalog keys are unchanged
