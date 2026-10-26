#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Changesets
  A changeset names its namespace and its changes in order; it applies whole, in one request, or not at all, reports
  the regions it reads and writes, and writes the same bytes from the same stored state.

  Scenario: A delete by pattern removes every object
    Given `<a> <nick> "Al"`, `<a> <nick> <n1>` and `<a> <nick> <n2>` are held
    When the pattern `a`, `nick` is deleted
    Then `a` holds no `nick`
    And the reverse list of `nick` for `n1` does not hold `a`
    And the reverse list of `nick` for `n2` does not hold `a`

  Scenario: One refused operation refuses the changeset
    Given `knows` is declared an edge
    When `<a> <knows> <b>` and `<a> <knows> "Bob"` are written in one changeset
    Then the write fails with an error naming `knows` and `"Bob"`
    And `<a> <knows> <b>` is not held

  Scenario: Written regions are reported
    When `<a> <knows> <b>` is written
    Then the write writes the forward and reverse regions of `knows`
    And the write writes the subject- and object-predicate regions of the default graph

  Scenario: Two applications write the same bytes
    When one changeset is applied to two copies of one store
    Then both stores hold the same keys, versions and values
