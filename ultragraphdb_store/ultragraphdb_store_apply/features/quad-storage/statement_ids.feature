#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Statement IDs
  A statement takes a system ID of the `Statement` type in its subject's bucket when it gains a property or repeats a
  held triple; its forward posting carries the ID and its record holds the statement and its properties
  (openspec/changes/stage-5-5-three-role-refactor/specs/quad-storage/spec.md, "Statement IDs", verbatim).

  Scenario: A property allocates a statement ID
    When `<a> <knows> <b>` is held without a statement ID and gains the property `<since> 2020`
    Then the statement has a statement ID of type prefix 2 and the bucket of `<a>`, its forward posting carries it, and its record holds `<since> 2020`

  Scenario: Removing a statement removes its record
    When a statement with a statement ID is deleted
    Then no statement record of that ID remains
