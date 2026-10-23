#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: The local commit, with index maintenance
  The local-commit scenarios over edges applied with their index, reverse and count entries: a request reads its own
  earlier writes, those entries included, and its commit writes them in one batch or not at all. The scenarios over
  bare posting lists run in ultragraphdb_store_posting's features.

  Scenario: A later edge reads an earlier edge of its request
    When `name` of `0x1` holds no value and a request sets it to `A` and then deletes `A`
    Then after the commit, `name` of `0x1` holds no value

  Scenario: A list first written blind is read whole
    When `friend` of `0x1` holds `0x2` in the store, and a request adds `0x3` without reading the list and then reads it
    Then the read holds `0x2` and `0x3`

  Scenario: A commit is visible whole
    When a request writing three lists commits at 12 while another reader reads at 12 repeatedly
    Then every read sees the changes to all three lists or to none of them

  Scenario: A failed request writes nothing
    When a request fails after building writes and before committing
    Then the store holds none of its writes
