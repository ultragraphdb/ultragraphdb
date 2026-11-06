#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: A transaction names its reads' level, session token and deadline
  The client's side: the Begin carries them, and a read waits as its server says. The test server plays the epochs.

  Scenario: A read with a future token waits
    When a read-only transaction begins with a token of epoch 30 while epoch 29 is the latest closed, reads, and epoch 30 closes
    Then the read answers after epoch 30 closes, at that epoch or later

  Scenario: A read past its transaction's deadline is refused
    When a read-only transaction begins with a token of epoch 30 and a deadline of 50 ms, and epoch 30 does not close within it
    Then its read is refused as timed out
