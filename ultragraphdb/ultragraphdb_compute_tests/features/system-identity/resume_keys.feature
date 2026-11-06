# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: System identity through compute
  openspec/changes/stage-5-5-three-role-refactor/specs/system-identity/spec.md.

  Rule: System IDs never leave the database

    Scenario: A resume key carries terms, not keys
      When a read stops at its limit after the row whose object is `http://ex.org/b`, and the server decodes the resume key it returned
      Then the key holds the pattern's hash and the text `http://ex.org/b`, and no store key or system ID

    Scenario: A tampered resume key is refused
      When a client changes one byte of a resume key and continues its read with it
      Then the read is refused with an error naming the resume key as malformed
