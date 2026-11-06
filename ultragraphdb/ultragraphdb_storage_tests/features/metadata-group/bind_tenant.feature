# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A partition refuses a tenant its map lists no run for
  openspec/changes/stage-5-5-three-role-refactor/specs/metadata-group/spec.md, through a real storage process beside a real frontend.

  Scenario: A Bind naming an unknown tenant is refused
    When a Bind names tenant 9, for which the partition's placement map lists no run
    Then the partition refuses it with Error 1, the message naming tenant 9, and holds no transaction
