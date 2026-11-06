# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The metadata group holds the tenants
  openspec/changes/stage-5-5-three-role-refactor/specs/metadata-group/spec.md

  Scenario: The first start creates tenant 0
    When a frontend starts on a metadata group holding no tenant
    Then the group holds tenant 0 durably before the frontend serves, Placement's runs list it, and a restart creates none

  Scenario: Two tenants share a run
    When the metadata group holds tenants 7 and 8, whose hashes share their top 8 bits, and a role reads Placement
    Then Placement's runs list each tenant once, both with the one `run`, and the bucket ranges are unchanged by either

  Scenario: An unknown name keeps one salt across a restart
    When a client authenticates as the unknown name `nobody`, the frontend restarts, and a client authenticates as `nobody` on another core
    Then both challenges carry one salt, and the metadata group's log holds the secret it derives from

  Scenario: A catalog version past its cap is refused at its Commit
    When a transaction's declarations build a catalog version of 523,001 bytes, and the client commits it
    Then the Commit is refused with code 7, `detail` 523,000, the message naming the size, and the current version stays
