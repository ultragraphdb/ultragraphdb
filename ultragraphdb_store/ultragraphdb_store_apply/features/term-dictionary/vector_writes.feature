#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Vector literals on write
  A vector literal is written only in its canonical form; any other form refuses the changeset, naming the literal's
  predicate.

  Scenario: A non-canonical vector is refused
    When `<a> <embedding> "[0.50,-1.0]"^^ugdb:float32vector` and `<a> <name> "A"` are written in one changeset
    Then the write fails with an error naming `embedding`
    And `<a> <name> "A"` is not held
