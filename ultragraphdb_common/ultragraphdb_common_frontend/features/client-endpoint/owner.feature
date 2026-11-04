# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: One owner answers for the metadata group

  Scenario: An eviction reaches every core at once
    When a session is evicted while a connection on another core holds it, and that connection commits next
    Then the commit is refused with the eviction's error

  Scenario: A close carries each eviction to its subscriber
    When a session is evicted and a subscriber waits for the next close
    Then the close names the session, and its catalog and placement are the group's

  Scenario: A lease is answered from a rising mark
    When a partition leases two blocks of one type
    Then the blocks do not overlap, and a restart leases past them both

  Scenario: A close request closes the epoch ahead of its length
    When a partition asks for the close of its open epoch in a sequencer of long epochs
    Then the close arrives well before the epoch's length
