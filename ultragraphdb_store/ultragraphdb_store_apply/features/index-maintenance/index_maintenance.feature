#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Index maintenance
  Each edge applied keeps its predicate's index tokens, reverse edges and count keys with it, in the request that
  applies it, and its value is converted to the schema's type or refused.

  Scenario: Setting an indexed value adds its tokens
    When `name: string @index(term) .` holds no value for `0x1` and `name` is set to `big apple`
    Then the index keys for `big` and `apple` each contain `0x1`

  Scenario: Changing a value moves its tokens
    When `name` of `0x1` changes from `big apple` to `red apple`
    Then the index key for `big` no longer contains `0x1`, and `red` and `apple` do

  Scenario: Deleting a value removes its index entries
    When the indexed value of `0x1` is deleted
    Then no index key of that predicate contains `0x1`

  Scenario: A language value uses its language's tokenizer
    When a fulltext-indexed predicate is set with language `de` to `Häuser`
    Then the index holds the German stem's token

  Scenario: Deleting one value of a list keeps shared tokens
    When `tags: [string] @index(term) .` holds `big apple` and `red apple` for `0x1`, and `big apple` is deleted
    Then the index key for `big` no longer contains `0x1`, and the keys for `red` and `apple` still do

  Scenario: A reverse edge follows its forward edge
    When `friend: uid @reverse .` is set from `0x1` to `0x2` and then to `0x3`
    Then the reverse key of `0x3` contains `0x1` and the reverse key of `0x2` does not

  Scenario: Adding a third uid moves the count
    When `friend: [uid] @count .` holds two uids for `0x1` and a third is added
    Then `0x1` is listed under count 3 and not under count 2

  Scenario: Re-setting an existing uid leaves the count
    When a uid already in the list is set again
    Then the count key of the subject does not change

  Scenario: Reverse counts are kept
    When `friend: [uid] @count @reverse .` gains the edge `0x1 → 0x2`
    Then the reverse count keys of `0x2` reflect one more incoming edge

  Scenario: A star delete clears reverse and count entries
    When `<0x1> <friend> * .` is deleted on `friend: [uid] @reverse @count .`
    Then `0x1` has no `friend` postings, appears in no reverse key of its former targets, is listed under no count key, and each former target's reverse count drops by one

  Scenario: A star delete clears index entries
    When `name: string @index(fulltext) @lang .` holds an untagged value and a value tagged `de` for `0x1`, and `<0x1> <name> * .` is deleted
    Then no index key of `name` contains `0x1`

  Scenario: A star delete on a scalar with language values clears its count
    When `name: string @count @lang .` holds an untagged value and a value tagged `en` for `0x1`, and `<0x1> <name> * .` is deleted
    Then `0x1` is listed under no count key of `name`

  Scenario: A value is converted to the schema type
    When `age: int .` is set for `0x1` with the untyped value `"12"`
    Then `age` of `0x1` holds the int 12

  Scenario: A value that cannot be converted is refused
    When `age: int .` is set for `0x1` with the untyped value `"twelve"`
    Then the write fails with `strconv.ParseInt: parsing "twelve": invalid syntax`, and `age` of `0x1` holds nothing
