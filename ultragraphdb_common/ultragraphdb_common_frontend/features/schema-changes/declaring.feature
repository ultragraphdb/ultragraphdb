# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A transaction's declarations, which hold nothing else
  openspec/changes/stage-5-5-three-role-refactor/specs/schema-changes/spec.md

  Scenario: A transaction holding declarations holds nothing else
    When transactions declaring `p` go on with an Operation, a change of data or a read, one changes data and then declares, and one Write holds a declaration and a change of data
    Then each is refused with code 7, `detail` the index of the first change of data in its Write, and its transaction ends

  Scenario: A Commit of declarations names no session
    When a transaction declaring `p` commits through a session
    Then the Commit is refused with code 7, and the transaction ends
