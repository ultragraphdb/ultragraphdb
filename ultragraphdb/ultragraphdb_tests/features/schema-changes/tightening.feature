# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A tightening through the three binaries
  openspec/changes/stage-5-5-three-role-refactor/specs/schema-changes/spec.md, through the real frontend and storage: a partition reads its own data, and the client hears what it found.

  Scenario: A tightening the data contradicts is refused and restored
    When `a` holds two objects of `nick` and a client commits a declaration of `nick` single-valued
    Then the Commit is refused with code 7 naming `nick` and `a`, the metadata group holds a version restoring the previous declaration, and no Describe reports `nick` single-valued

  Scenario: A tightening passes every partition's check through the binaries
    When `a` and `b`, one in each of two partitions, each hold one object of `nick`, and a client commits a declaration of `nick` single-valued
    Then both partitions have reported when the Commit answers the first version of the effective epoch, and Describe reports `nick` single-valued

  Scenario: A partition silent past the wait withdraws the tightening
    When a client commits a declaration of `nick` single-valued, and a partition holding the tenant reports nothing within `catalog_check_ms`
    Then the Commit is refused with code 24, `detail` that partition, naming it and the version, the metadata group holds a version restoring the previous declaration, and a report arriving later changes nothing

  Scenario: A write the tightening forbids is refused once the check passed
    When a client's declaration of `name` a property is acknowledged, `a` holding a literal of it, the storage process restarts, and a client writes `<a> <name> <b>`
    Then the write is refused, and `a` holds its literal alone

  Scenario: A partition holding no term of a predicate writes it under its declaration
    When a client's declaration of a new `name` a property is acknowledged over two partitions, and a client writes an IRI object of `name` to a subject of each
    Then each write is refused
