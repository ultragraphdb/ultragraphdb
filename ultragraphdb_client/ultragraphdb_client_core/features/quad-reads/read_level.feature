#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: A read's transaction names its level
  The client's side: a Begin through a session carries the session's token, so its read sees the session's commit. The
  levels' own rules are compute's and the frontend's.

  Scenario: A read after a commit sees it through its token
    When a client commits a write, receives a session token, and begins a transaction with that token whose read asks for the write
    Then the read returns the write
