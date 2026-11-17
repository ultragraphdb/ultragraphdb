# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

@system @hosting
Feature: The system group's role commands reach storage
  openspec/changes/stage-5-8-system-group/specs/wire-protocol/spec.md, "Roles exchange role commands": the system group's
  sequencer, hosted by a storage process, delivers its closes to a storage process subscribed at its role address. Moved
  here from the frontend's suite, which hosts no sequencer since the switch (design D2).

  Scenario: Epoch closes reach storage in order
    When the system group's sequencer closes epochs 1 to 5 and storage is connected
    Then storage receives five epoch-close commands, in order
