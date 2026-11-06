# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A tightening is checked before it is acknowledged
  openspec/changes/stage-5-5-three-role-refactor/specs/schema-changes/spec.md, with played partitions: what each partition reports, and when, the scenario decides.

  Scenario: A tightening the data contradicts is refused and restored
    When `a` holds two objects of `nick` and a client commits a declaration of `nick` single-valued
    Then the Commit is refused with code 7 naming `nick` and `a`, the metadata group holds a version restoring the previous declaration, and no Describe reports `nick` single-valued

  Scenario: A tightening the data bears out is acknowledged after the check
    When a client commits a declaration of `nick` single-valued over two partitions holding `a` with one object of it, and one has reported its check passed
    Then the Commit stays unanswered and Describe reports `nick` as before until the other reports, and then the Commit answers the first version of the effective epoch and Describe reports `nick` single-valued

  Scenario: A partition silent past the wait withdraws the tightening
    When a client commits a declaration of `nick` single-valued, and a partition holding the tenant reports nothing within `catalog_check_ms`
    Then the Commit is refused with code 24, `detail` that partition, naming it and the version, the metadata group holds a version restoring the previous declaration, and a report arriving later changes nothing

  Scenario: A version built on one being checked is refused
    When a client commits a declaration of `nick` single-valued, and before any partition reports a second transaction begun on that version commits a declaration of `age`
    Then the second Commit is refused with code 2, kind 5, naming the version being checked, and once the first is answered a third transaction's declaration of `age` commits

  Scenario: Declarations tightening and loosening at once are refused at the Commit
    When `friend` is declared an edge, and a client commits a declaration of `friend` a property
    Then the Commit is refused with code 7, `detail` its declaration's index, naming `friend`, and the catalog is unchanged

  Scenario: A restarted frontend finishes the check it held
    When a client commits a declaration of `nick` single-valued, the frontend stops before any partition reports, and after its restart a partition reports that the data refuses it
    Then the metadata group holds a version restoring the previous declaration, and no Describe reports `nick` single-valued

  Scenario: A Drop during a check stands against the restoring version
    When a client commits a declaration of `nick` and of `age` single-valued, a Drop of `nick` builds its version before any partition reports, and a partition reports that the data refuses the first
    Then the latest version holds neither `nick` nor `age` single-valued, and `nick` declared by no version after the Drop's
