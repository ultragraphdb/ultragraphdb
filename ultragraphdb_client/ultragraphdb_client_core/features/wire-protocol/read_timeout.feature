#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: The client's timeout leaves a read its deadline
  A read's silence within its transaction's deadline is a wait for a close, and a stream without credit waits on its caller.

  Scenario: A long deadline survives the client's timeout
    When a read-only transaction's deadline is 40 s, and its read waits 35 s for a close, the client's timeout set at 30 s
    Then the connection stays open, and the read answers once the close arrives

  Scenario: A stream without credit runs no timeout
    When a read's stream has spent its credit, and its caller takes no batch for longer than the timeout
    Then the connection stays open
