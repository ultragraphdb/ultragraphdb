#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Partitions under changesets
  A reader consults a node's predicate lists before it reads a list of the node
  (openspec/changes/stage-5-5-three-role-refactor/specs/partitions/spec.md, "Absence is proven by a node's predicate
  lists"): the scenario an execution's reads decide, verbatim.

  Scenario: A predicate the node lacks is not read
    When a node holds only `name`, and a read asks for its `age`
    Then the read answers no `age` after reading the node's subject-predicates list, and reads no `age` list
