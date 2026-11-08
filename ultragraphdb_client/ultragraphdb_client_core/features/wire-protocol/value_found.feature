#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: An OperationFailed Error carries the value found within the floor of `max_frame`
  A value found past what the Error frame leaves it is cut, a flag in `detail` says so, and the client says it.

  Scenario: A value found past the frame arrives cut and flagged
    When a ConditionalWrite's compare-and-set fails on a literal of 524,288 bytes, and another on a literal as long as the first refusal carried
    Then the first refusal holds the literal's start, cut, and the client says it was cut; the second holds the literal whole, and the client says nothing was cut
