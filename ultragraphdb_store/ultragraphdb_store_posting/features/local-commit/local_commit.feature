#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: The local commit
  A transaction reads its lists at its read version with its own earlier writes on top, and never another
  transaction's; the admitted transactions of a round are written as one store batch, each at the commit version the
  commit step gives it, without a sync, and the visibility watermark passes the round only once it and its cache update
  are applied. A read at the watermark never waits; a read above it waits until the watermark reaches it.

  Scenario: A later edge reads an earlier edge of its request
    When `name` of `0x1` holds no value and a transaction sets it to `A` and then deletes `A`
    Then after the commit, `name` of `0x1` holds no value

  Scenario: Another request does not see uncommitted writes
    When one transaction has set `name` on `0x1` and not committed, and a second transaction reads it
    Then the second transaction sees the value before the first transaction's write

  Scenario: A transaction in the same round does not read another's writes
    When two transactions of one round execute from the same watermark and the first sets `name` of `0x1` to `A`
    Then the second reads `name` of `0x1` as it was at the watermark

  Scenario: A list first written blind is read whole
    When `friend` of `0x1` holds `0x2` in the store, and a transaction adds `0x3` without reading the list and then reads it
    Then the read holds `0x2` and `0x3`

  Scenario: A commit is visible whole
    When a transaction writing three lists commits at 12 while another reader reads at 12 repeatedly
    Then every read sees the changes to all three lists or to none of them

  Scenario: Two commits in one batch are visible together
    When one round holds a commit at 12 and a commit at 14 while another reader reads at 14 repeatedly
    Then every read sees both commits or neither

  Scenario: A failed request writes nothing
    When a transaction aborts after building writes
    Then the store holds none of its writes

  Scenario: A failed request holds up no read
    When a transaction aborts and another transaction begins
    Then the second transaction begins at the watermark without waiting for the aborted one

  Scenario: A read waits for a commit below its timestamp
    When a commit at 10 is being applied and a read at 10 starts
    Then the read returns after the commit is applied and sees it

  Scenario: A rollup leaves an open read intact
    When a read at 6 is open and a list with deltas at 6 and 8 is rolled up
    Then the rollup is written at 9, the read at 6 still returns its result, and the discard timestamp stays at or below 6
