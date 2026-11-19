# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The bootstrap principal
  openspec/changes/stage-5-5-three-role-refactor/specs/metadata-group/spec.md

  Scenario: The bootstrap principal authenticates
    When a cluster starts for the first time with a bootstrap credential file and a client authenticates with it
    Then authentication succeeds, and the metadata group holds that principal without the password
