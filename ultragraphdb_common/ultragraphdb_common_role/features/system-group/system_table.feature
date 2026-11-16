# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: System group: one storage instance hosts the system group
  A storage instance whose settings hold a `[system]` table hosts the system group, and the table names
  `system.role_address` and `system.data_dir` (openspec/changes/stage-5-8-system-group/specs/system-group/spec.md).
  Bound here by the start every role makes on its settings; the storage binary's exit is bound over the built binary.

  Scenario: A system table without its directory stops the start
    When a storage instance's `[system]` table names `role_address` and no `data_dir`
    Then the process exits with code 78 and an error naming `system.data_dir`
