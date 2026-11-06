# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Drops through the three binaries
  openspec/changes/stage-5-5-three-role-refactor/specs/schema-changes/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request.

  Scenario: A default entry outlives a drop
    When `score` holds a default-entry record, and a Drop of `score` commits
    Then the metadata group still holds the record, and Describe names `score` as recorded

  Scenario: A Drop through no session runs again
    When a Drop of `p` through no session commits at its partition, its reply is lost, and the client sends the Drop again
    Then the second Drop removes nothing more, and the frontend builds the version removing `p`'s declaration before it relays the reply
