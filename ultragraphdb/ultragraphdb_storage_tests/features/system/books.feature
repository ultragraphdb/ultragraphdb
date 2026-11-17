# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

@system @books
Feature: The system group's books in the storage process hosting it
  openspec/changes/stage-5-8-system-group/specs/system-group/spec.md: "The system group runs on a core of its own over logs
  of its own", "The hosting process starts the group first and stops it last". The storage binary as processes, settings
  served by the config double; the group read over role connections at its role address.

  Scenario: The books recover in the storage process
    When the group holds a catalog version, two block leases, a principal and twelve closes, and its host is killed and restarted
    Then the group's state after recovery equals its state before the kill, read at the same log position, and the sequencer's last closed epoch is 12

  Scenario: A partition's sync holds up no close
    When a partition's log sync is delayed past `epoch_ms_max` while the group's logs sync at once
    Then the sequencer closes its epochs on time, and the delayed partition receives them once it reads again

  Scenario: A partition recovers from the group of its own process
    When the hosting process restarts with a partition whose log names catalog version 3
    Then the group recovers first, the partition reads version 3 with Catalog and then replays

  Scenario: A failed log of the group stops the process
    When a write to the metadata log fails
    Then the hosting process shuts down through its ordinary path and exits with code 74
