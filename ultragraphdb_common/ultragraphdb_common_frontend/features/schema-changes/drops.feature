# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Drops, the catalog step
  openspec/changes/stage-5-5-three-role-refactor/specs/schema-changes/spec.md

  Scenario: A resent Drop builds the version a stopped frontend left
    When a Drop of `p` through a session commits at its partition, the frontend stops before it commits the catalog version removing `p`'s declaration, and the client resends the Drop after the frontend restarts
    Then the partition's table answers the stored reply, the frontend commits that version as pending before it relays the reply, and `p` is dropped once

  Scenario: A Drop through no session runs again
    When a Drop of `p` through no session commits at its partition, its reply is lost, and the client sends the Drop again
    Then the second Drop removes nothing more, and the frontend builds the version removing `p`'s declaration before it relays the reply

  Scenario: A default entry outlives a drop
    When `score` holds a default-entry record, and a Drop of `score` commits
    Then the metadata group still holds the record, and Describe names `score` as recorded
