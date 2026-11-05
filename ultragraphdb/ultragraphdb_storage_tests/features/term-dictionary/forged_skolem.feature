# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Term dictionary: a forged skolem IRI at its home
  openspec/changes/stage-5-5-three-role-refactor/specs/term-dictionary/spec.md, between two storage processes.

  Rule: A reference to another partition's entity registers at its home

    Scenario: A forged skolem IRI in a write is refused at its home
      When a transaction in partition 1 writes an edge to a skolem IRI whose bucket partition 2 owns, and partition 2's dictionary does not hold it
      Then the registration's outcome is 7, and the Write is refused with code 7, ending the transaction
