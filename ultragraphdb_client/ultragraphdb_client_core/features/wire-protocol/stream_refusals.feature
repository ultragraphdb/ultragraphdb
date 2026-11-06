#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: A request on a stream that cannot take it is refused
  The dialling side of `PROTOCOL.md` §2.5: a frame on a stream it has ended is late, and one on a stream it has not
  opened answers no request. The serving side is the frontend's.

  Scenario: A late frame reaches the client
    When a Result frame arrives for a read the client has cancelled
    Then the client checks the frame and drops it, and the connection stays open

  Scenario: A frame on a stream not yet opened is refused
    When a frame arrives on a stream at or above the next stream the client will open
    Then the client refuses it, and closes the connection
