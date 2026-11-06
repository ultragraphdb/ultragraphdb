# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The term dictionary through compute
  openspec/changes/stage-5-5-three-role-refactor/specs/term-dictionary/spec.md.

  Rule: A missing dictionary entry is corruption

    Scenario: A dangling ID read through compute answers code 17
      When a list names a system ID its partition's dictionary answers no entry for, and a client's read through compute reaches it
      Then the read is refused with code 17, the message naming the list's key, and the client's connection stays open

    Scenario: A lookup's dangling ID names its key
      When an index list names a system ID its partition's dictionary answers no entry for, and compute's API looks up the value
      Then the lookup is refused with code 17, the message naming the dictionary key

    Scenario: A lookup's corrupt dictionary value is no absence
      When a partition's entry of the looked-up predicate does not decode as a system ID, and compute's API looks up the value
      Then the lookup is refused with code 17, the message naming the entry's key
