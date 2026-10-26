#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: The catalog's write checks
  A write is checked against its namespace's catalog: object kinds, datatype constraints and unique values refuse the
  changeset whole; a single-valued predicate's write replaces the subject's object; the indexes declared are kept. The
  catalog's own scenarios run in ultragraphdb_core_catalog's features.

  Scenario: An undeclared predicate accepts any object
    When `<a> <p> <b>` and `<a> <p> "text"` are written
    Then `a`'s `p` objects are `<b>` and `"text"`

  Scenario: A declared index is read by index maintenance
    Given `name` is declared a property with a `term` index
    When `<a> <name> "big apple"` is written
    Then the `term` index of `name` lists `a` under `big` and `apple`

  Scenario: A literal on an edge predicate is refused
    Given `knows` is declared an edge
    When `<a> <knows> "Bob"` is written
    Then the write fails with an error naming `knows` and `"Bob"`
    And `<a> <knows> "Bob"` is not held

  Scenario: A datatype constraint refuses another datatype
    Given `age` is declared with datatype `xsd:integer`
    When `<a> <age> "old"` is written
    Then the write fails with an error naming `age`, `XMLSchema#integer` and `XMLSchema#string`

  Scenario: A single-valued property is overwritten
    Given `age` is declared single-valued
    And `<a> <age> 30` is held
    When `<a> <age> 31` is written
    Then `a`'s `age` objects are `31`

  Scenario: A second subject with a unique value is refused
    Given `id` is declared unique
    And `<a> <id> "x"` is held
    When `<b> <id> "x"` is written
    Then the write fails with an error naming `id`, `"x"` and `a`
    And `<b> <id> "x"` is not held

  Scenario: The same subject may write its unique value again
    Given `id` is declared unique
    And `<a> <id> "x"` is held
    When `<a> <id> "x"` is written
    Then the write succeeds
    And `a`'s `id` objects are `"x"`

  Scenario: A unique value is unique per graph
    Given `id` is declared unique
    And `<a> <id> "x" <g1>` is held
    When `<b> <id> "x" <g2>` is written
    Then `<a> <id> "x" <g1>` and `<b> <id> "x" <g2>` are held
