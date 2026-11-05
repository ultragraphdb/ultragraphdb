# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Wire protocol: what a Bind stream takes
  openspec/changes/stage-5-5-three-role-refactor/specs/wire-protocol/spec.md, through a real storage process beside a real
  frontend.

  Rule: Roles exchange role commands

    Scenario: A Write on a Bind stream is answered by Held
      When the frontend sends a Write on a Bind stream, its references registered and its changes held by the partition
      Then the partition answers Held on the stream, which stays open; the dialling side's check accepts it, and the frontend relays nothing to the client

    Scenario: A Write on a stream no Bind opened is refused
      When a storage process receives a Write on a stream no Bind opened, a Commit on one of kind 2, and a Drop on one of kind 1
      Then each is refused with Error 1, the message naming the command
