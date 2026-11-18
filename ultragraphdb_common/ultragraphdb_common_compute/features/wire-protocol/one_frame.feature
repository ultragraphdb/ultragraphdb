# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A write commits in one frame, as the frontend relays it
  openspec/changes/stage-5-5-three-role-refactor/specs/wire-protocol/spec.md

  Scenario: A DirectWrite is relayed with its principal
    When a client authenticated as principal 9 sends a DirectWrite through the frontend
    Then the frame the partition receives holds the client's body byte for byte and its command header but for `principal` 9, its write-set record names principal 9, and the client receives a Commit

  Scenario: A client naming a principal is refused
    When a client's DirectWrite holds a `principal` above 0
    Then it is refused at the client endpoint with `FieldValue` naming `principal`, and no partition sees it
