#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: The client's credit is automatic
  The shell grants credit on a read's stream as its batches are consumed, and exposes no manual grant.

  Scenario: A long read flows without a manual grant
    When a client reads 10,000 rows through a shell with an initial credit of 4 frames
    Then every row arrives, credit being granted as batches are consumed, and no grant of zero frames is sent
