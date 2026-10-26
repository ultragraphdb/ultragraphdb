#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Index maintenance
  Each statement a changeset writes keeps its predicate's value-index tokens, reverse entries, predicate lists and
  count keys with it, in the request that applies it. Terms are kept as written; a literal is converted only to compute
  its tokens, by the tokenizers that take its datatype.

  Scenario: Writing an indexed literal adds its tokens
    Given `name` is declared with a `term` index
    When `<a> <name> "big apple"` is written
    Then the `term` index of `name` lists `a` under `big` and `apple`

  Scenario: Overwriting a single-valued literal moves its tokens
    Given `name` is declared single-valued with a `term` index
    And `<a> <name> "big apple"` is held
    When `<a> <name> "red apple"` is written
    Then the `term` index of `name` does not list `a` under `big`
    And the `term` index of `name` lists `a` under `red` and `apple`

  Scenario: Deleting a literal removes its index entries
    Given `name` is declared with a `term` index
    And `<a> <name> "big apple"` is held
    When `<a> <name> "big apple"` is deleted
    Then no value-index key of `name` holds `a`

  Scenario: A language-tagged literal uses its language's tokenizer
    Given `body` is declared with a `fulltext` index
    When `<a> <body> "Häuser"@de` is written
    Then the `fulltext` index of `body` lists `a` under the tokens of `"Häuser"@de`

  Scenario: Deleting one literal keeps shared tokens
    Given `tags` is declared with a `term` index
    And `<a> <tags> "big apple"` and `<a> <tags> "red apple"` are held
    When `<a> <tags> "big apple"` is deleted
    Then the `term` index of `tags` does not list `a` under `big`
    And the `term` index of `tags` lists `a` under `red` and `apple`

  Scenario: A reverse entry follows its statement
    Given `friend` is declared single-valued
    When `<a> <friend> <b>` and `<a> <friend> <c>` are written
    Then the reverse list of `friend` for `c` holds `a`
    And the reverse list of `friend` for `b` does not hold `a`

  Scenario: Undeclared predicates keep reverse entries
    When `<a> <likes> <b>` is written
    Then the reverse list of `likes` for `b` holds `a`

  Scenario: Adding a third object moves the count
    Given `friend` is declared with an exact count index
    And `<a> <friend> <b>` and `<a> <friend> <c>` are held
    When `<a> <friend> <d>` is written
    Then `a` is listed under forward count 3 of `friend` and not under 2

  Scenario: Re-writing an existing object leaves the count
    Given `friend` is declared with an exact count index
    And `<a> <friend> <b>` is held
    When `<a> <friend> <b>` is written again
    Then the count keys of `a` and `b` for `friend` do not change

  Scenario: Reverse counts are kept
    Given `friend` is declared with an exact count index
    And `<c> <friend> <b>` is held
    When `<a> <friend> <b>` is written
    Then `b` is listed under reverse count 2 of `friend` and not under 1

  Scenario: A delete by pattern clears reverse and count entries
    Given `friend` is declared with an exact count index
    And `<a> <friend> <b>`, `<a> <friend> <c>` and `<x> <friend> <b>` are held
    When the pattern `a`, `friend` is deleted
    Then `a` holds no `friend`
    And the reverse list of `friend` for `b` does not hold `a`
    And the reverse list of `friend` for `c` does not hold `a`
    And `a` is listed under no forward count key of `friend`
    And `b` is listed under reverse count 1 of `friend` and not under 2
    And `c` is listed under no reverse count key of `friend`

  Scenario: A delete by pattern clears index entries
    Given `name` is declared with a `fulltext` index
    And `<a> <name> "big houses"` and `<a> <name> "große Häuser"@de` are held
    When the pattern `a`, `name` is deleted
    Then no value-index key of `name` holds `a`

  Scenario: Non-canonical integers index alike
    Given `periods` is declared with an `int` index
    When `<a> <periods> "042"^^xsd:integer` and `<b> <periods> 42` are written
    Then the `int` index of `periods` lists `a` and `b` under `42`

  Scenario: Every value of a multi-valued predicate is indexed
    Given `periods` is declared with an `int` index
    When `<a> <periods> 41`, `<a> <periods> 42` and `<a> <periods> 43` are written
    Then a range read of the `int` index of `periods` over 42 to 43 returns `a`

  Scenario: A literal of another datatype is held unindexed
    Given `periods` is declared with an `int` index
    When `<a> <periods> "soon"` is written
    Then `a`'s `periods` objects are `"soon"`
    And no value-index key of `periods` holds `a`

  Scenario: A GeoJSON geometry is indexed by its cells
    Given `where` is declared with a `geo` index
    When `<a> <where> "{\"type\": \"Point\", \"coordinates\": [8.63, 50.12]}"^^geo:geoJSONLiteral` is written
    Then the `geo` index of `where` lists `a` under the tokens of `"{\"type\": \"Point\", \"coordinates\": [8.63, 50.12]}"^^geo:geoJSONLiteral`

  Scenario: An insert reads nothing
    Given `tag` is declared with a `term` index
    When `<a> <tag> "x"` is written
    Then the write reads no list
    And the write writes the forward, reverse, predicate-list, index and statistics regions of `tag`

  Scenario: A delete reads the subject's list
    Given `<a> <tag> "x"` is held
    When `<a> <tag> "x"` is deleted
    Then the write reads the forward list of `tag`

  Scenario: An integer keeps its lexical form under an index
    Given `n` is declared with an `int` index
    When `<a> <n> "007"^^xsd:integer` is written
    Then `a`'s `n` objects are `"007"^^xsd:integer`
    And the `int` index of `n` lists `a` under `7`
