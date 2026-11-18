# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Wire protocol: the commands stage 6.6 adds, refused by stage
  openspec/changes/stage-6-6-ugs-data-definition/specs/wire-protocol/spec.md, "The frames follow PROTOCOL.md as extended
  for UGS": Purge and Migrate decode, and the frontend refuses each with code 7 naming its stage (PROTOCOL.md §11.2).
  Moved here from the transport's features in task 1.3, the refusal being the frontend's (notes/group-1.md, decision 9).

  Scenario: Purge and Migrate decode and are refused by stage
    When a client sends a well-formed Purge and a well-formed Migrate
    Then each is refused with code 7 on its stream, the message naming the command and its stage, and the connection stays open
