#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Changesets
  A changeset names its tenant and its changes in order; it executes against the snapshot at the watermark, each new
  term's ID got or created while it runs and shared with every execution naming the term; it applies whole, in one
  request, or not at all, reports what it read, the terms it found absent by ID and no dictionary key, and the regions
  it reads and writes, and writes the same bytes from the same stored state
  (openspec/changes/stage-5-5-three-role-refactor/specs/changesets/spec.md, its two requirements verbatim).

  Scenario: A delete by pattern removes every object
    When `a` holds three objects of `nick` and a changeset deletes the pattern `a`, `nick` in the default graph
    Then `a` holds no `nick`, and the reverse lists of the three objects no longer hold `a`

  Scenario: A new term's ID is fixed before the commit
    When a transaction creates `<new>`, and another transaction writing `<new>` executes before the first commits
    Then both receive the same system ID for `<new>` while executing, and whichever commits holds `<new>` under it

  Scenario: One refused operation refuses the changeset
    Given `knows` is declared an edge
    When `<a> <knows> <b>` and `<a> <knows> "Bob"` are written in one changeset
    Then the write fails with an error naming `knows` and `"Bob"`
    And `<a> <knows> <b>` is not held

  Scenario: Written regions are reported
    When a changeset inserting `<a> <knows> <b>` is executed
    Then the reported write regions include the forward and reverse regions of `knows` and the subject- and object-predicate regions of the default graph

  Scenario: An insert records the statement it tested
    When a changeset inserting `<a> <knows> <b>` is executed and `a` holds other objects of `knows`
    Then its read set holds the statement `a`, `knows`, `b`, not the whole list of `knows` of `a`

  Scenario: A new term leaves no dictionary key in the read set
    When a changeset inserting `<new> <knows> <b>` is executed and `<new>` had no ID
    Then its read set holds the absent term `<new>` and no dictionary key

  Scenario: Two applications write the same bytes
    When one changeset is applied to two copies of one store
    Then both stores hold the same keys, versions and values
