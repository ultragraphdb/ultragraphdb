# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Role binaries: a storage instance's system, metadata and sequencer tables
  "Configuration is explicit and validated at start" for the tables a hosting storage instance holds
  (openspec/changes/stage-5-8-system-group/specs/role-binaries/spec.md; design D2). The requirement names no scenario of
  these tables; these two state its sentences for them (notes/group-2.md, Found wanting).

  Scenario: A hosting storage instance's tables are read
    When a storage instance's settings hold a `[system]` table with `role_address` and `data_dir`, a `[metadata]` and a `[sequencer]` table
    Then each of their settings reads as written

  Scenario: An unknown setting in the system table stops the start
    When a storage instance's `[system]` table holds the key `role_adress`
    Then the settings are refused with an error naming `role_adress`
