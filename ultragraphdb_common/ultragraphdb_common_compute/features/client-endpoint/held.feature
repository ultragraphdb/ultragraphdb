# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The frontend bounds what a connection's reads hold back
  openspec/changes/stage-5-5-three-role-refactor/specs/client-endpoint/spec.md

  Scenario: A write past `held_bytes` is refused
    When `held_bytes` is 1,024, a read of a transaction is open, and the client sends Writes that the read holds back past 1,024 bytes
    Then the Write past the bound is refused with code 15, `detail` 1,024, on the transaction's stream, and the transaction ends

  Scenario: Credit still drains a read under held writes
    When a read holds Writes back, and the client grants the read credit
    Then the frontend reads the Credit and relays it, the read reaches its `LAST`, and the held Writes are then sent

  Scenario: A client past its bound stops being read until its partition reads again
    When `held_bytes` is 4,096, partition 1 stops reading, a client pipelines Writes for partition 1 on four streams past the bound while its Commit on partition 2 waits for its reply, and sends a Begin behind them
    Then the frontend counts at most the bound and one frame for it, reads no further frame of it, and sends it the reply of its Commit, another client commits on partition 2, and when partition 1 reads again the Begin is answered, each stream's Writes reach partition 1 in the order sent, and each Commit is answered

  Scenario: A transaction's limit ends what a stopped partition holds back
    When `held_bytes` is 4,096, a partition stops reading, and a client pipelines Writes for it past the bound and passes `transaction_ms`, a Begin behind them
    Then each transaction is answered code 4, the frontend counts nothing for the client, and the Begin is answered while the partition still reads nothing

  Scenario: What never left is taken back
    When a partition stops reading with its socket full, a client's one-frame writes wait behind it past `transaction_ms`, and another client binds and aborts behind them
    Then each one-frame write that never left is answered code 4, the frontend counts nothing for the client, and when the partition reads again it sees none of those, no Bind and no Cancel, and answers the ones that left
