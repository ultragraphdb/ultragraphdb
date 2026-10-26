#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Statement IDs
  A statement takes an ID, from its namespace's own sequence, when it gains a property or repeats a held triple; its
  forward posting carries the ID and its record holds the statement and its properties.

  Scenario: A property allocates a statement ID
    Given `<a> <knows> <b>` is held
    When `<a> <knows> <b>` gains the property `<since> 2020`
    Then the forward posting of `<a> <knows> <b>` carries one statement ID
    And the record of that ID holds `<a> <knows> <b>` and the property `<since> 2020`

  Scenario: Removing a statement removes its record
    Given `<a> <knows> <b>` is held with the property `<since> 2020`
    When the statement of `<a> <knows> <b>` is deleted by its ID
    Then no statement record of that ID remains
    And `<a> <knows> <b>` is not held
