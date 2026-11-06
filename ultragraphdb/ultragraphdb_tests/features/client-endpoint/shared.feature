# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Routing and reads held back through the three binaries
  openspec/changes/stage-5-5-three-role-refactor/specs/client-endpoint/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request.

  Scenario: Credit still drains a read under held writes
    When a read holds Writes back, and the client grants the read credit
    Then the frontend reads the Credit and relays it, the read reaches its `LAST`, and the held Writes are then sent

  Scenario: A first read holds a later read only until its first frame
    When a transaction's first read takes six frames at `initial_credit` 4, and after its first batch the client sends a second read and waits on it
    Then the frontend sends the second read to compute on the first read's first frame, and the client receives its rows with the first read still open
