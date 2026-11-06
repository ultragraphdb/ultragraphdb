# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Quad reads of a transaction through compute
  openspec/changes/stage-5-5-three-role-refactor/specs/quad-reads/spec.md, of a read-write transaction bound by a test playing the frontend; the frontend's own order of a transaction's requests is its own crate's.

  Rule: A read through compute is a read of its transaction

    Scenario: A read sees its transaction's own write
      When a transaction writes `<a> <knows> <b>`, the write is answered, and the same transaction reads `<a> <knows> ?o` before committing
      Then the read returns `<b>`, and a read by any other transaction does not

    Scenario: A read sees its transaction's new term and its emptied list
      When a transaction writes `<new> <p> "x"`, creating `<new>`, removes every object of `<a> <p> ?o`, and reads both
      Then the read returns `"x"` for `<new>` with `<new>` named by its text, and nothing for `<a>`

    Scenario: A scan through compute holds no list in the transaction
      When a transaction scans 10,000 lists through compute
      Then its read set holds one range and no list key, and it holds none of those lists in memory afterwards

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

    Scenario: A read past the read set's room ends its transaction
      When a transaction's reads leave no room for a span
      Then the read is refused with `TooLarge`, no entry of that span reaches the client, and the transaction's next request is refused

    Scenario: A statement's record read conflicts with a property written
      When T1 reads the properties of the reifier `r` through compute, T2 sets a property through `r` and commits, and T1 commits a write
      Then T1 aborts with a conflict of kind Range

  Rule: Access paths follow the bound positions

    Scenario: A graph appearing in a gap conflicts
      When T1 reads with its graph unbound through compute, finding `<g1>` alone, T2 commits a statement in a graph the walk passed over, and T1 commits a write
      Then T1 aborts with a conflict of kind Range
