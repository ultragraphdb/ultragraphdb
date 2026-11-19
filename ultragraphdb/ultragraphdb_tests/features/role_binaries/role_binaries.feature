# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Whole clusters as processes
  openspec/changes/stage-5-5-three-role-refactor/specs/role-binaries/spec.md, with the two roles of
  openspec/changes/stage-5-9-compute-role/specs/role-binaries/spec.md: the roles started in the supervisor's order by the
  harness, and over Unix sockets, where compute listens at no role address and sends no Join until the node list is built
  (corner case C6.28)

  Scenario: The start order reaches readiness
    When the supervisor starts the config service and the two roles
    Then the config service answers first, storage's `/ready` answers before compute is started, compute's answers last, and no role waited on one that was waiting on it

  Scenario: The roles join over Unix sockets
    When the storage process starts, then compute, each at a Unix socket
    Then every role's `/ready` answers, compute's last, and the system group's members name storage alone
    And each role's socket has mode 0600
