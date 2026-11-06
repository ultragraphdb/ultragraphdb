#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: A frame past its stream's credit is refused
  Every receiver of Result or Entries frames is the side that dialled, so the refusal is its own and goes on no wire.

  Scenario: A Result frame past its credit closes the client's connection
    When a server sends a read's stream a Result frame with the stream's credit spent
    Then the client refuses it as `PastCredit`, naming the stream, and closes the connection
