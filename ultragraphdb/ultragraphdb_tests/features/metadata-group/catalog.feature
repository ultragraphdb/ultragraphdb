# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The metadata group through the three binaries
  openspec/changes/stage-5-5-three-role-refactor/specs/metadata-group/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request.

  Scenario: A catalog version past its cap is refused at its Commit
    When declaring Commits, each within one Declare frame, build a tenant's catalog version until one more would pass 16,777,216 bytes, and the client commits that one
    Then the Commit is refused with code 7, `detail` 16,777,216, the message naming the size, and the current version stays
