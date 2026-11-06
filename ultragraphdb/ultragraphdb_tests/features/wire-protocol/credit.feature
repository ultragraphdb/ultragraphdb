# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The client's credit through the three binaries
  openspec/changes/stage-5-5-three-role-refactor/specs/wire-protocol/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request.

  Scenario: A long read flows without a manual grant
    When a client reads 10,000 rows through a shell with an initial credit of 4 frames
    Then every row arrives, credit being granted as batches are consumed, and no grant of zero frames is sent
