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
