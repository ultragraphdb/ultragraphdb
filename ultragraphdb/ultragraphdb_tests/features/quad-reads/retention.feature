# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: What a partition retains, as a client reads it
  openspec/changes/stage-5-5-three-role-refactor/specs/quad-reads/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request.

  Scenario: A reader coming back within the retention keeps its version
    When a read-only transaction at snapshot level reads at epoch 20, epochs close for 30 s with `read_retain_ms` 60,000, and it reads again
    Then the second read answers at epoch 20's version bound

  Scenario: A reader coming back past the retention is refused
    When a read-only transaction at snapshot level reads at epoch 20, the partition raises its discard version past that version bound once a later epoch has been closed for more than `read_retain_ms`, and it reads again
    Then the read is refused with code 3

  Scenario: A resumed version no longer retained is refused
    When a reader resumes with a key at a named version the partition has discarded
    Then the read is refused with code 3

  Scenario: An open Scan holds its version past the retention
    When a Scan keeps taking frames for longer than `read_retain_ms` while later epochs close
    Then the discard version stays at or below the Scan's version, and the Scan reaches `LAST` at that version
