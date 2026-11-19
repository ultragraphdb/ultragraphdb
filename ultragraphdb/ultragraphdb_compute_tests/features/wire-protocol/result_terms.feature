# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A result names its terms by their text
  openspec/changes/stage-5-5-three-role-refactor/specs/wire-protocol/spec.md, through the three binaries.

  Scenario: A result names IRIs by their text
    When a read returns rows whose subjects are `http://ex.org/a` and `http://ex.org/b`
    Then the batch's dictionary holds both IRIs' texts and the rows name them by dictionary index, with no system ID

  Scenario: A result names a blank node by its opaque skolem IRI
    When a read returns a row whose subject is a blank node
    Then the batch names it, tagged as a blank node, by its skolem IRI `…/.well-known/genid/{bucket}-{random}`, whose text holds no type prefix and no sequence of its system ID
