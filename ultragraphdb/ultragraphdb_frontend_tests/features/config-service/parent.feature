# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A role starts from five bootstrap variables
  openspec/changes/stage-5-5-three-role-refactor/specs/config-service/spec.md

  Scenario: A role ends with the process it names
    When a role starts with `ULTRAGRAPHDB_PARENT_PID` naming a process that is not its parent
    Then it stops as on SIGTERM and exits with code 0
