# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The role secret at the client endpoint
  openspec/changes/stage-5-5-three-role-refactor/specs/role-binaries/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request.

  Scenario: The role secret opens no client session
    When a client authenticates on the client endpoint with the role secret
    Then authentication is refused with the same error as a wrong password
