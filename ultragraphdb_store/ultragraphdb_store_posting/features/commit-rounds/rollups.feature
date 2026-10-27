#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Rollups take no version from the commit step
  A rollup reads at the visibility watermark and writes one above the newest commit version it folds, which is odd and
  so never a commit version.

  Scenario: A rollup writes between commit versions
    When a list with deltas at 10 and 12 is rolled up
    Then its complete version is written at 13, and the next commit's version is even
