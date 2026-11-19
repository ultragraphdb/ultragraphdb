# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Drops, the catalog step
  openspec/changes/stage-5-5-three-role-refactor/specs/schema-changes/spec.md
  openspec/changes/stage-5-8-system-group/specs/schema-changes/spec.md, compute's early refusal of a Drop's catalog version

  Scenario: A resent Drop builds the version a stopped compute node left
    When a Drop of `p` through a session commits at its partition, compute stops before the catalog version removing `p`'s declaration is asked for, and the client resends the Drop after compute restarts
    Then the partition's table answers the stored reply, the system group commits that version as pending, asked by compute before it relays the reply, and `p` is dropped once

  Scenario: A Drop through no session runs again
    When a Drop of `p` through no session commits at its partition, its reply is lost, and the client sends the Drop again
    Then the second Drop removes nothing more, and the system group builds the version removing `p`'s declaration, asked by compute before it relays the reply

  Scenario: A default entry outlives a drop
    When `score` holds a default-entry record, and a Drop of `score` commits
    Then the metadata group still holds the record, and Describe names `score` as recorded

  Scenario: Compute refuses a Drop naming another version than the one in effect
    When version 1 of a tenant is in effect at the endpoint core's open epoch, and a client sends Drops of kind 3 naming `catalog` 0, 2 and 1, then a Drop of kind 2 and one of kind 1, naming none
    Then the first two are refused with code 2, kind 5, each message naming its version and version 1, before any partition sees them, and the other three reach the partition

  Scenario: A pending version leaves a Drop naming the version in effect to the partition
    When version 1 of a tenant is in effect, a Drop of kind 1 makes version 2 pending, and a client sends a Drop of kind 3 naming `catalog` 1
    Then compute passes it to the partition

  Scenario: During a check a Drop names the version being checked
    When version 2 of a tenant is being checked, its effective epoch at or below the endpoint core's open epoch, Describe names version 1, and a client sends a Drop of kind 3 naming `catalog` 2 and one naming `catalog` 1
    Then the first reaches the partition, and the second is refused with code 2, kind 5, the message naming versions 1 and 2
