#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Exact statistics
  A write counts a statement only when its subject's list does not hold it already, so the statistics stay exact
  without any write reading them.

  Scenario: A statement written twice is counted once
    When `<a> <knows> <b>` is written in one commit and again in a later one
    Then the statistics of `knows` report 1 statement
