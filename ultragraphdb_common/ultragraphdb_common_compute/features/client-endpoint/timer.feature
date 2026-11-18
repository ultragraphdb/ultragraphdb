# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The frontend times a read-write transaction
  openspec/changes/stage-5-5-three-role-refactor/specs/client-endpoint/spec.md

  Scenario: An unbound transaction passes its limit
    When a read-write transaction begins and sends nothing for `transaction_ms`
    Then the frontend answers code 4 on its stream, with no partition having seen it

  Scenario: A bound transaction passes its limit
    When a read-write transaction bound to partition 1 sends nothing for `transaction_ms` from its Begin
    Then the frontend cancels the Bind stream, which aborts it at the partition, and answers code 4 on its stream

  Scenario: A read holding a write back is named at the limit
    When a read of a transaction has spent its credit, the client sends a Write and a second read, waits on the second read, and grants no credit for `transaction_ms`
    Then the frontend answers code 4, the message naming the first read's stream and that it had no credit

  Scenario: A relayed Commit is not timed
    When a transaction's Commit has been relayed, and `transaction_ms` passes before its reply
    Then the client receives the Commit's reply
