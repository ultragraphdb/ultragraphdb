# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Compute and a lost system group
  openspec/changes/stage-5-8-system-group/specs/system-group/spec.md, "A lost system group is waited for until the
  deadline": compute's part. The frontend's part is bound in ultragraphdb_frontend_tests.

  Scenario: A group gone past the deadline ends its dependants
    When the hosting storage process is killed and does not return within `join_deadline_ms`
    Then compute shuts down through its ordinary path and exits with code 69 naming the system group's address
