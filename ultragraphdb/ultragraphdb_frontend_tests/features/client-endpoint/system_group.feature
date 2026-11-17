# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The frontend asks the system group a storage process hosts
  openspec/changes/stage-5-8-system-group/specs/client-endpoint/spec.md, "The endpoint asks the system group" and "The
  frontend finds the roles in Members": the frontend, storage and compute binaries as processes, one storage process hosting
  the system group, settings served by the config double.

  Scenario: The frontend listens at no role address
    When the frontend has started
    Then it listens at its client address and its observability address alone

  Scenario: A restarted frontend serves the sessions it served before
    When a client registers a session and commits request 1, the frontend is killed and restarted, and the client reconnects and commits request 2 through the same session
    Then request 2 commits, and the frontend replayed no log and read no file of its own

  Scenario: A storage process restarted at its address is dialled again
    When a storage process restarts at the same role address while the frontend runs
    Then the frontend's connection to it closes, the frontend reads Members again and dials the address, and its next write to that partition is answered
