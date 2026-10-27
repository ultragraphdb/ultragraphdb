#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: What a transaction's reads record and what conflicts with them
  A read-write transaction records the lists it read whole, the statements it tested, the key ranges it scanned, the
  dictionary keys it found absent and its namespace's catalog range, and conflicts with a transaction committed after
  its read version only where that one wrote what it read. These scenarios run the recording side: each transaction
  executes against its snapshot, and its recorded reads are judged against the other's writes by the rules the commit
  step's resolver applies.

  Scenario: Inserts of different objects into one list both commit
    When two transactions concurrently insert `<a> <knows> <b>` and `<a> <knows> <c>`
    Then both commit, and `a` knows `b` and `c`

  Scenario: Inserts of the same new statement conflict
    When two transactions concurrently insert `<a> <knows> <b>`, which is not held
    Then one commits and the other aborts with a conflict, and its retry finds the statement held

  Scenario: A scanned range conflicts with an insert into it
    When a transaction scans the `int` index of `age` for values from 30 to 40 and writes `<x> <seen> true`, and a transaction inserting `<c> <age> 35` commits after its read version
    Then the scanning transaction aborts with a conflict

  Scenario: An insert outside a scanned range does not conflict
    When a transaction scans the `int` index of `age` for values from 30 to 40 and writes `<x> <seen> true`, and a transaction inserting `<c> <age> 50` commits after its read version
    Then the scanning transaction commits
