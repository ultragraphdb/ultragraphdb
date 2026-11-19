# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Compute asks the system group a storage process hosts
  openspec/changes/stage-5-9-compute-role/specs/client-endpoint/spec.md, "The endpoint asks the system group" and "Compute
  finds the partitions in Members": the compute and storage binaries as processes, one storage process hosting the system
  group, settings served by the config double.

  Scenario: Compute listens at no role address
    When compute has started
    Then it listens at its client address and its observability address alone

  Scenario: A restarted compute node serves the sessions it served before
    When a client registers a session and commits request 1, compute is killed and restarted, and the client reconnects and commits request 2 through the same session
    Then request 2 commits, and compute replayed no log and read no file of its own

  Scenario: A storage process restarted at its address is dialled again
    When a storage process restarts at the same role address while compute runs
    Then compute's connection to it closes, compute reads Members again and dials the address, and its next write to that partition is answered
