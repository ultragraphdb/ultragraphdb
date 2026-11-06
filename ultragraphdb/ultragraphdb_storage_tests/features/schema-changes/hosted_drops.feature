# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A Drop hosted at its partition
  openspec/changes/stage-5-5-three-role-refactor/specs/schema-changes/spec.md, through a real storage process beside a real frontend.

  Scenario: Dropping data keeps the catalog
    When a tenant's data is dropped
    Then its predicates hold no data and its catalog is unchanged
