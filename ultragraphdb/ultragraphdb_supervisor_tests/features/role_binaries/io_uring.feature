# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: io_uring where allowed, polling where not
  openspec/changes/stage-5-5-three-role-refactor/specs/role-binaries/spec.md

  Scenario: A refused io_uring falls back
    When a role starts under Docker's default seccomp profile
    Then it starts on the polling driver, logs that it does, and serves requests
