# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The sequencer in the system group
  The scenarios of openspec/changes/stage-5-8-system-group/specs/sequencer/spec.md this change added or rewrote,
  verbatim, bound through the system group; the scenarios it left as they were stay bound at the state machine in
  `ultragraphdb_cluster_sequencer`.

  Rule: An early close is asked by CloseRequest from either role

    Scenario: The frontend's request closes the open epoch
      When epoch 12 is the latest close, and the frontend sends CloseRequest naming epoch 13 with `partition` 0
      Then the sequencer closes epoch 13 before `epoch_ms_max` has passed, once `epoch_ms_min` has, and every subscriber receives the close

    Scenario: A request for a closed epoch closes nothing
      When epoch 13 is closed and a CloseRequest names epoch 13
      Then no frame answers it, and epoch 14 closes at its own time

  Rule: A subscriber holding no close begins at the latest

    Scenario: A late subscriber is not refused for trimmed closes
      When the sequencer has closed 40 epochs and holds the last 10, and a Subscribe sets `latest`
      Then the stream's first close is that of epoch 40, and the close of epoch 41 follows it

    Scenario: A storage process may not ask for the latest
      When a connection whose role is `storage` sends a Subscribe with `latest` 1
      Then it is refused with `FieldValue` naming `latest`

  Rule: Closes announce the versions taking effect after them

    Scenario: A crash before the close leaves the version pending
      When a catalog version is committed as pending while epoch 38 is the latest close, and the system group's host is killed before it closes epoch 39
      Then after recovery the version is still pending, and the next close, of epoch 39, assigns it effective epoch 40
