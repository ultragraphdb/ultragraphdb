# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: System IDs never leave the database
  openspec/changes/stage-5-5-three-role-refactor/specs/system-identity/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request.

  Scenario: A result names an IRI by its text
    When a client reads the subject of a statement whose subject is `http://ex.org/a`
    Then the result batch names `http://ex.org/a` by its text, and its bytes contain no system ID

  Scenario: A statement with a statement ID is named by its reifier
    When a client reads a statement that carries a property and so holds a statement ID
    Then the result names the statement by its quad and its reifier's skolem IRI, whose text holds the statement's bucket and no sequence, and its bytes contain no system ID

  Scenario: A resume key carries terms, not keys
    When a read stops at its limit after the row whose object is `http://ex.org/b`, and the server decodes the resume key it returned
    Then the key holds the pattern's hash and the text `http://ex.org/b`, and no store key or system ID

  Scenario: A tampered resume key is refused
    When a client changes one byte of a resume key and continues its read with it
    Then the read is refused with an error naming the resume key as malformed
