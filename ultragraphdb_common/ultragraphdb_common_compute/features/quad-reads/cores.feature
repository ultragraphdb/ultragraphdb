# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Compute runs each read on one core
  openspec/changes/stage-5-9-compute-role/specs/quad-reads/spec.md: a transaction's Bind stream and its reads' Scans leave
  on the core's one connection to the partition (design D2), the partition played by a double.

  Scenario: A transaction's reads share its writes' core
    When a read-write transaction writes and then reads
    Then the Bind stream and the read's Scans leave from one core, and the read returns the write
