#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Telemetry
  The applier logs the failures no caller receives: a build running in the background has none to return its error to.

  Scenario: An index build that fails is logged
    When an index build running in the background fails
    Then the applier emits an error event `error in building indexes, aborting` with the build's record as a field
