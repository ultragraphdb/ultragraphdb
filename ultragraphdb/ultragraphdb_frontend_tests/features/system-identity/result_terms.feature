# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: System IDs never leave the database in a result
  openspec/changes/stage-5-5-three-role-refactor/specs/system-identity/spec.md, through the three binaries.

  Scenario: A result names an IRI by its text
    When a client reads the subject of a statement whose subject is `http://ex.org/a`
    Then the result batch names `http://ex.org/a` by its text, and its bytes contain no system ID

  Scenario: A statement with a statement ID is named by its reifier
    When a client reads a statement that carries a property and so holds a statement ID
    Then the result names the statement by its quad and its reifier's skolem IRI, whose text holds the statement's bucket and no sequence, and its bytes contain no system ID
