#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: The data dictionary
  One read API answers what a namespace holds at a snapshot: its labels, relationship types and property keys, each
  label's shape, the catalog's declarations and the statistics. Declared entries are exact; the shapes of undeclared
  data come from a background pass over a snapshot, which no write reads or updates.

  Scenario: Labels and their properties are listed
    Given `<a> rdf:type <Person>`, `<a> <name> "Ann"`, `<a> <knows> <b>` and `<b> rdf:type <Person>` are held
    And the shapes are current
    Then the dictionary lists the label `Person`
    And the dictionary lists the property `name` of `Person` with datatype `xsd:string`
    And the dictionary lists the relationship type `knows` from `Person` to `Person`

  Scenario: A declared cardinality is reported without data
    Given `email` is declared a single-valued property of `Person` with datatype `xsd:string`
    Then the dictionary reports `email` on `Person`, single-valued, of datatype `xsd:string`

  Scenario: A new property appears after the next pass
    Given `<a> rdf:type <Person>` is held and the shapes are current
    When `<a> <nick> "Al"` is written and the next pass completes
    Then the dictionary lists `nick` on `Person`, with the pass's snapshot timestamp

  Scenario: A write reads no shape
    Given `<a> rdf:type <Person>` is held
    When `<a> <nick> "Al"` is written while a pass over the snapshot before it runs
    Then the write reads no list
    And the pass's shapes equal those of a pass over the same snapshot after the write
