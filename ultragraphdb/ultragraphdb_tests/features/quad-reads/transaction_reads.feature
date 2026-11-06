# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

@both_carriers
@core
Feature: Quad reads of a transaction through the three binaries
  openspec/changes/stage-5-5-three-role-refactor/specs/quad-reads/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request; run over TCP and again over Unix sockets.

  Rule: A read through compute is a read of its transaction

    Scenario: A read sees its transaction's own write
      When a transaction writes `<a> <knows> <b>`, the write is answered, and the same transaction reads `<a> <knows> ?o` before committing
      Then the read returns `<b>`, and a read by any other transaction does not

    Scenario: A term nobody holds conflicts with its later statement through compute
      When transaction T1 reads `<nobody> <p> ?o` through compute, finding nothing, transaction T2 commits `<nobody> <p> "x"`, and T1 commits a write
      Then T1 aborts with a conflict of kind AbsentTerm

    Scenario: A scanned range conflicts with an insert into it through compute
      When transaction T1 reads `<a> <knows> ?o` through compute, finding nothing, transaction T2 commits `<a> <knows> <b>`, and T1 commits a write
      Then T1 aborts with a conflict of kind Range

    Scenario: A reverse range conflicts with an edge whose reverse entry lands in it
      When transaction T1 reads `?s <knows> <b>` through compute, transaction T2 commits `<c> <knows> <b>` in the same partition, and T1 commits a write
      Then T1 aborts with a conflict of kind Range

    Scenario: A range stopped at its limit conflicts only with what it returned
      When T1 reads with limit 1 a range holding `<a>` and `<z>`, T2 commits an insert past `<a>` that T1's read did not reach, and T1 commits without resuming
      Then T1 commits

    Scenario: Resuming records the rest
      When T1 reads with limit 1, resumes to the range's end, T2 commits an insert into the resumed part, and T1 commits a write
      Then T1 aborts with a conflict of kind Range

    Scenario: A statement's record read conflicts with a property written
      When T1 reads the properties of the reifier `r` through compute, T2 sets a property through `r` and commits, and T1 commits a write
      Then T1 aborts with a conflict of kind Range
