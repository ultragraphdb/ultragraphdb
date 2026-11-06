#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: The Rust client retries a busy Register
  A full registry that holds no session it may evict answers a Register code 14, which the client sends again itself.

  Scenario: A Register answered busy is sent again
    When a client of 4 commit attempts registers, and a full registry answers its first two Registers code 14
    Then the client sends the Register three times, each on a new stream, and returns the session

  Scenario: A Register busy past its attempts returns code 14
    When a client of 3 commit attempts registers, and a full registry answers every Register code 14
    Then the client sends the Register three times and returns code 14, and the connection stays open

  Scenario: A connection lost under a Register's wait ends it
    When a Register answered code 14 waits for its next send, and the connection is lost
    Then the client returns the lost connection, and sends the Register on no later connection
