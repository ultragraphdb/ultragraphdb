# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Read levels through the three binaries
  openspec/changes/stage-5-5-three-role-refactor/specs/quad-reads/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request.

  Scenario: A read after a commit sees it through its token
    When a client commits a write, receives a session token, and begins a transaction with that token whose read asks for the write
    Then the read returns the write

  Scenario: Two reads of one transaction share its snapshot
    When a transaction begins at snapshot level, reads `<a>`'s statements, another transaction commits a new statement of `<a>`, and the first transaction reads `<a>`'s statements again
    Then both reads return the same rows

  Scenario: Monotonic-fast reads never go back
    When a monotonic-fast transaction's first read answers at epoch 9, and its second read touches only a partition whose latest closed epoch is 12
    Then the second read answers at epoch 12 without waiting

  Scenario: A monotonic-fast read never answers below the token
    When a read-only monotonic-fast transaction carries a token of partition 1 at epoch 20, and its read touches partition 1, closed at 21, and partition 2, closed at 18
    Then the read waits until partition 2 has closed epoch 20, then answers at epoch 20 and returns the token's commit

  Scenario: Concurrent monotonic-fast reads take the highest epoch answered
    When a monotonic-fast transaction's read answered at epoch 12 and another at epoch 9, and a third is sent after both
    Then the third read's floor is 12
