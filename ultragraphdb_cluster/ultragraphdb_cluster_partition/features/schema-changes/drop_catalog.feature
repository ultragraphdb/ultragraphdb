# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A partition checks the catalog version a Drop names
  openspec/changes/stage-5-8-system-group/specs/schema-changes/spec.md

  Scenario: A Drop aimed at an earlier schema is refused by the partition
    When version 4 of a tenant takes effect, an endpoint core whose view still holds version 3 sends a Drop of kind 4 naming `catalog` 3, and the partition decides it in a round of the epoch at which version 4 is in effect
    Then the partition refuses the Drop with code 2, kind 5, and removes nothing
