# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Posting lists are stored in the repository's own format
  The posting format over 128-bit system IDs
  (openspec/changes/stage-5-5-three-role-refactor/specs/posting-lists/spec.md): the scenarios of the requirement
  "Posting lists are stored in the repository's own format", verbatim.

  Scenario: A list round-trips
    When a list with a packed ID list, value postings with statement IDs and splits is encoded and decoded
    Then the decoded list equals the original

  Scenario: A flipped byte is corruption
    When one byte of a stored list is flipped and the list is read
    Then the read fails with a corruption error naming the key

  Scenario: A list of the previous format is refused
    When a value written with stage 5's version byte is read
    Then the read fails with a corruption error naming the key and the unknown version
