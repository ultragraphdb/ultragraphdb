# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Whole clusters as processes
  openspec/changes/stage-5-5-three-role-refactor/specs/role-binaries/spec.md

  Scenario: The start order reaches readiness
    When the frontend starts, then storage, then compute
    Then every role's `/ready` answers, the frontend's last, and the frontend's members name storage and compute

  Scenario: The roles join over Unix sockets
    When the frontend starts, then storage, then compute, each at a Unix socket
    Then every role's `/ready` answers, the frontend's last, and the frontend's members name storage and compute
    And each role's socket has mode 0600
