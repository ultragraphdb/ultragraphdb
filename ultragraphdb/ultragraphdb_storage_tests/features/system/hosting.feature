# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

@system @hosting
Feature: A storage instance hosts the system group
  openspec/changes/stage-5-8-system-group/specs/system-group/spec.md: "One storage instance hosts the system group", "The
  system group runs on a core of its own over logs of its own", "The hosting process starts the group first and stops it
  last". The storage binary as processes, settings served by the config double; the development container's `/data`
  stands for a data root of the test's own.

  Scenario: The hosting instance serves the books
    When `storage_1` holds a `[system]` table and `storage_2` none, and both start
    Then `storage_1` listens at `system.role_address` and answers a Placement request there, and `storage_2` listens at its partitions' addresses alone

  Scenario: A system table without its directory stops the start
    When a storage instance's `[system]` table names `role_address` and no `data_dir`
    Then the process exits with code 78 and an error naming `system.data_dir`

  Scenario: The group's logs lie beside the partitions'
    When the development container has started with `system.data_dir = "/data/system"`
    Then `/data` holds `system/meta/`, `system/seq/` and `storage/p1/r1/`

  Scenario: Storage is ready before any compute node exists
    When the config service and the hosting storage process start, and no other role does
    Then the storage process's `/ready` answers
