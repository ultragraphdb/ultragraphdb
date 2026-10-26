#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Failed builds and rollups across drops
  A build that fails restores the previous declaration, whose indexes writes kept complete; a rollup that read a list
  before a drop covering it writes nothing.

  Scenario: A failed rebuild keeps the old index usable
    When `term` is replaced by `exact` and the `exact` build fails
    Then the schema declares `term` and every term key is present

  Scenario: A rollup read before a drop writes nothing after it
    When a rollup of a `name` list reads at 7, `name` is dropped at 8, and the rollup then writes
    Then its batch is discarded, and no key of `name` exists
