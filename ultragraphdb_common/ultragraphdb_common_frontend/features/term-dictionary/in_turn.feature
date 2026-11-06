# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Blank nodes bind in turn
  openspec/changes/stage-5-5-three-role-refactor/specs/term-dictionary/spec.md

  Scenario: Transactions naming only new blank nodes bind in turn
    When three transactions whose first writes name only new blank nodes arrive over three partitions
    Then the frontend binds them to partitions 1, 2 and 3 in turn, and each blank node's bucket is one its partition owns
