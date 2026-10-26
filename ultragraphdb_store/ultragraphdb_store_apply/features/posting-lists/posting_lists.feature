#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Posting lists
  The quad lists hold their objects as sets: an IRI or blank node by its UID, a literal by its encoding, a fingerprint
  collision told apart by the encodings. A single-valued predicate's write replaces the subject's object.

  Scenario: A single-valued predicate replaces its object
    Given `name` is declared single-valued
    When `<a> <name> "A"` and `<a> <name> "B"` are written
    Then `a`'s `name` objects are `"B"`

  Scenario: Language-tagged literals coexist
    When `<a> <label> "A"`, `<a> <label> "Á"@fr` and `<a> <label> "B"@fr` are written
    Then `a`'s `label` objects are `"A"`, `"Á"@fr` and `"B"@fr`

  Scenario: A multi-valued predicate accumulates
    When `<a> <knows> <b>` and `<a> <knows> <c>` are written
    Then `a`'s `knows` objects are `<b>` and `<c>`

  Scenario: Colliding fingerprints keep both literals
    Given every literal's fingerprint is 7
    When `<a> <tag> "x"` and `<a> <tag> "y"` are written
    Then `a`'s `tag` objects are `"x"` and `"y"`
    And after `<a> <tag> "x"` is deleted, `a`'s `tag` objects are `"y"`

  Scenario: Deleting an object the list does not hold changes nothing
    Given `<a> <name> "A"` is held
    When `<a> <name> "B"` is deleted
    Then `a`'s `name` objects are `"A"`
