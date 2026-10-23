#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: The local commit
  A request reads its lists with its own earlier writes and those of the requests before it in its batch on top of
  the state at its read timestamp; its writes reach the store only in its batch's one store batch, at the commit
  timestamp the writer hands it, and a read at a timestamp waits until every commit at or below it is applied.

  Scenario: A later edge reads an earlier edge of its request
    When `name` of `0x1` holds no value and a request sets it to `A` and then deletes `A`
    Then after the commit, `name` of `0x1` holds no value

  Scenario: A later request in a batch reads an earlier one
    When two requests are applied in one batch and the first sets `name` of `0x1` to `A`
    Then the second reads `A` for `name` of `0x1`

  Scenario: Another request does not see uncommitted writes
    When one request has set `name` on `0x1` and not committed, and a second request reads it
    Then the second request sees the value before the first request's write

  Scenario: A list first written blind is read whole
    When `friend` of `0x1` holds `0x2` in the store, and a request adds `0x3` without reading the list and then reads it
    Then the read holds `0x2` and `0x3`

  Scenario: A commit is visible whole
    When a request writing three lists commits at 12 while another reader reads at 12 repeatedly
    Then every read sees the changes to all three lists or to none of them

  Scenario: Two commits in one batch are visible together
    When one batch holds a commit at 12 and a commit at 13 while another reader reads at 13 repeatedly
    Then every read sees both commits or neither

  Scenario: A failed request writes nothing
    When a request fails after building writes and before committing
    Then the store holds none of its writes

  Scenario: A failed request holds up no read
    When a request fails before its batch is written and a read at the next timestamp starts
    Then the read returns without waiting for the failed request

  Scenario: A read waits for a commit below its timestamp
    When a commit at 10 is being written and a read at 11 starts
    Then the read returns after the commit is applied and sees it

  Scenario: A rollup leaves an open read intact
    When a read at 5 is open and a list with deltas at 5 and 6 is rolled up
    Then the rollup is written above 6, the read at 5 still returns its result, and the discard timestamp stays at or below 5
