#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Wire protocol: the streams stage 6.6 adds
  openspec/changes/stage-6-6-ugs-data-definition/specs/wire-protocol/spec.md, "The frames follow PROTOCOL.md as extended
  for UGS": a Describe's frames spend credit as Result frames do, the scenario whose decision the transport's scheduler
  makes. "Purge and Migrate decode and are refused by stage" is the frontend's, whose features hold it since task 1.3
  (notes/group-1.md, decision 9); a client's refusal of a frame past its credit is the client's to bind.

  Scenario: A Describe of five frames takes a Credit
    When a Describe answer takes five frames and the Welcome's `initial_credit` is 4
    Then the fifth frame is sent only after the client's Credit, and a fifth frame sent without one is refused as `PastCredit`
