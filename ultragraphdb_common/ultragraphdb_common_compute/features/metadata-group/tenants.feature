# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The metadata group holds the tenants
  openspec/changes/stage-5-5-three-role-refactor/specs/metadata-group/spec.md

  Scenario: The first start creates tenant 0
    When the system group's host starts on a metadata group holding no tenant
    Then the group holds tenant 0 durably before it listens, Placement's runs list it, and a restart creates none

  Scenario: Two tenants share a run
    When the metadata group holds tenants 7 and 8, whose hashes share their top 8 bits, and a role reads Placement
    Then Placement's runs list each tenant once, both with the one `run`, and the bucket ranges are unchanged by either

  Scenario: An unknown name keeps one salt across a restart
    When a client authenticates as the unknown name `nobody`, the system group's host and compute restart, and a client authenticates as `nobody` on another core
    Then both challenges carry one salt, and the metadata group's log holds the secret it derives from
