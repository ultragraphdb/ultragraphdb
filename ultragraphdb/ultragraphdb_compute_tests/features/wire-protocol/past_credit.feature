# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Compute as a receiver of Entries frames
  openspec/changes/stage-5-5-three-role-refactor/specs/wire-protocol/spec.md.

  Rule: A frame past its stream's credit is refused

    Scenario: An Entries frame past its credit closes compute's role connection
      When a partition sends a Scan's stream an Entries frame with the stream's credit spent
      Then compute refuses it as `PastCredit`, naming the stream, and closes the role connection
