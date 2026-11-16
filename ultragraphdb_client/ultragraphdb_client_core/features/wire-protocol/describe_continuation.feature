#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: A Describe's answer continues across frames
  An answer larger than one frame within the client's `max_frame` continues in further Describe frames under the
  stream's credit, read as one body; a stream passing `catalog_bytes_max` is refused, and no further frame of it read.

  Scenario: A Describe continues across frames
    When a tenant's Describe answer takes two frames within the client's `max_frame`
    Then the client reads the declarations one frame would hold, within the Welcome's credit

  Scenario: A Describe past the catalog's bound is refused
    When a server sends a Describe answer whose sections hold more than `catalog_bytes_max` bytes together
    Then the client refuses it as `FieldValue` naming `describe`, and closes the connection
