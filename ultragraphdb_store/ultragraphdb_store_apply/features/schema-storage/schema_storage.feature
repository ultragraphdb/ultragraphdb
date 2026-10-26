#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Schema storage
  The write scenarios of schema storage: a write to a predicate the catalog does not declare records its default entry
  in the write's commit batch, which a refused write never writes. The scenarios of the catalog surviving a restart run
  in ultragraphdb_core_catalog's features.

  Scenario: A first write records the predicate
    When `<a> <nickname> "Al"` is written
    Then the catalog holds `nickname` with the default entry

  Scenario: A failed write records no predicate
    Given `knows` is declared an edge
    When `<a> <alias> "Al"` and `<a> <knows> "Bob"` are written in one changeset
    Then the write fails with an error naming `knows`
    And the catalog does not hold `alias`

  Scenario: Concurrent first writes both commit
    Given `<x> <seeAlso> <score>` is held
    When two changesets read before either commits, the first writing `<a> <score> 12` and the second `<b> <score> "high"`
    Then both commit
    And the catalog holds `score` with the default entry
