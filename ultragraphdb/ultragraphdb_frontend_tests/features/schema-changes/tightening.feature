# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A tightening's check is the system group's
  openspec/changes/stage-5-8-system-group/specs/schema-changes/spec.md, "A tightening is checked before it is acknowledged":
  the frontend and two storage processes, the first hosting the system group and the second holding the data the tightening
  is checked against.

  Scenario: A check outlives the frontend that asked for it
    When a client commits a declaration of `nick` single-valued, the frontend stops before any partition reports, and a partition then reports that the data refuses it
    Then the metadata group holds a version restoring the previous declaration, and no Describe reports `nick` single-valued
