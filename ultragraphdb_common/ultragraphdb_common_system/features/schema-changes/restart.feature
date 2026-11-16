# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A tightening's check in the system group
  The scenario of openspec/changes/stage-5-8-system-group/specs/schema-changes/spec.md, "A restarted host of the system
  group takes a check up again", verbatim, bound through the system group.

  Rule: A restarted host of the system group takes a check up again

    Scenario: A check passes after the host restarts
      When a declaration of `nick` single-valued is being checked over two partitions, one has reported its check passed, the system group's host is killed and restarted, and the other partition then reports its check passed
      Then the version stands, Describe reports `nick` single-valued, and the client's Commit, whose reply was lost, is shown committed by Describe
