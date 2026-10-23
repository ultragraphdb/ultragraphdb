#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Schema changes and drops
  A schema change builds the indexes it adds from the data in the background, writes maintaining the previous
  schema's indexes and the new schema's meanwhile, and swaps them in one batch; a type change is checked before
  anything changes; drops remove what they drop physically; a request that read before a drop or schema change it
  writes under fails at commit.

  Scenario: A new index covers existing data
    When a predicate holds 1,000 values and `@index(exact)` is added
    Then after the rebuild, the index holds a token for each of the 1,000 values

  Scenario: Queries use the interim schema during a rebuild
    When a predicate indexed by `term` changes to `exact` and the `exact` index is being built
    Then the schema state used for queries reports the predicate with neither `term` nor `exact`

  Scenario: Writes during a rebuild are indexed
    When a value is written while its predicate's index is rebuilt
    Then after the rebuild, the index holds the value's tokens

  Scenario: Writes during a rebuild leave no entry of a removed index
    When `name: string @index(term) .` changes to `name: string @index(exact) .`, and `name` of `0x2` is set to `pear` while the `exact` index is built
    Then after the rebuild, no term key of `name` remains, and the exact key for `pear` contains `0x2`

  Scenario: A value-type change rebuilds its tokens whole
    When `n: string @index(exact) .` holding `7` for `0x1` changes to `n: int @index(int) .`, and `n` of `0x1` is set to `8` while the index is built
    Then after the rebuild, no exact key of `n` remains, and the int keys of `n` list `0x1` under 8 and not under 7

  Scenario: Removing one tokenizer keeps another
    When a predicate indexed by `term` and `exact` drops `term`
    Then its term keys are gone and its exact keys remain

  Scenario: A scalar becomes a list
    When `tag: string .` with values becomes `tag: [string] .`
    Then every existing value is read back as an element of the list

  Scenario: A list cannot become a scalar
    When `tag: [string] .` holding values is changed to `tag: string .`
    Then the change fails with `Schema change not allowed from [string] => string without deleting pred: tag`

  Scenario: An empty list cannot become a scalar
    When `tag: [string] @index(exact) .` holding no values is changed to `tag: string .`
    Then the change fails with `Type can't be changed from list to scalar for attr: [tag] without dropping it first.`, and the schema still declares `tag: [string] @index(exact) .`

  Scenario: A uid predicate holding data cannot become a string
    When `friend: uid .` holds edges and is changed to `friend: string .`
    Then the change fails with `Schema change not allowed from scalar to uid or vice versa while there is data for pred: friend`

  Scenario: An empty uid predicate can become a password
    When `token: uid .` holds no edges and is changed to `token: password .`
    Then the change is applied

  Scenario: Deltas summing to nothing hold no data
    When `friend: uid .` gained the edge `0x1 → 0x2` at 3 and lost it at 4, and is changed to `friend: string .` at 5
    Then the change is applied

  Scenario: A string cannot become a password
    When `secret: string .` is changed to `secret: password .`
    Then the change fails with `Schema change not allowed from STRING to PASSWORD`

  Scenario: A failed rebuild keeps the old index usable
    When `term` is replaced by `exact` and the `exact` build fails
    Then the schema declares `term` and every term key is present

  Scenario: Dropping a predicate removes its split parts
    When a predicate whose lists are split is dropped
    Then no key of the predicate remains, split parts included

  Scenario: Dropping a vector predicate drops its index predicates
    When `emb: float32vector @index(hnsw) .` holds vectors and is dropped
    Then no key of `emb`, `emb__vector_entry`, `emb__vector_` or `emb__vector_dead` remains

  Scenario: Dropping data keeps the schema
    When a namespace's data is dropped
    Then its predicates hold no data and its schema and types are unchanged

  Scenario: Dropping a namespace's everything survives a restart
    When everything in namespace 3 is dropped and the store is reopened
    Then namespace 3 holds only its initial schema and types

  Scenario: A namespace's initial schema is visible after its drop
    When everything in namespace 3 is dropped at 50
    Then a read at 60 finds namespace 3's initial schema and types

  Scenario: Dropping everything leaves the initial schema
    When everything is dropped
    Then the store holds the root namespace's initial schema and types, written at timestamp 1, and nothing else

  Scenario: Dropping a namespace's data leaves no split part
    When a namespace whose lists are split has its data dropped
    Then no key under `0x00` or `0x04` of the namespace remains, and its schema and type keys are unchanged

  Scenario: A request that read before a drop fails
    When a request reads at 5, `name` is dropped at 8, and the request then commits a write to `name`
    Then the commit fails with `Transaction has been aborted. Please retry`, and no key of `name` exists

  Scenario: A rollup read before a drop writes nothing after it
    When a rollup of a `name` list reads at 7, `name` is dropped at 8, and the rollup then writes
    Then its batch is discarded, and no key of `name` exists
