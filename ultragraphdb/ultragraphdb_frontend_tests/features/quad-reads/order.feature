# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The frontend keeps a transaction's order
  openspec/changes/stage-5-5-three-role-refactor/specs/quad-reads/spec.md, through the three binaries: what compute and storage
  answer is theirs, where ultragraphdb_common_frontend's suite plays them to see each frame's order.

  Scenario: A read waits for a write sent before it
    When a client sends a write and then a read of its transaction on another stream, without waiting for the write's reply
    Then the frontend sends the read to compute only after the write is answered, and the read returns the write

  Scenario: A read already answered takes nothing further
    When a read of a transaction has reached its `LAST`, and the transaction then ends with an Error
    Then the frontend sends nothing further on that read's stream
