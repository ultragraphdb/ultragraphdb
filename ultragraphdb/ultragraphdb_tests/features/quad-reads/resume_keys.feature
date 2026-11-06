# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Resume keys as a client holds them
  openspec/changes/stage-5-5-three-role-refactor/specs/quad-reads/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request.

  Scenario: A read resumes where it stopped
    When a read with limit 2 matches 5 rows and is resumed twice
    Then the three answers return rows 1–2, 3–4 and 5, the last with no resume key

  Scenario: A resumed read may find nothing
    When a read with limit 1 and its graph unbound matches one row and is resumed
    Then the first answer holds the row and a resume key, and the second answers one batch flagged `LAST` with no row and no key

  Scenario: A resume key with another pattern is refused
    When a resume key from one read is used with a different pattern
    Then the read is refused with an error naming the resume key

  Scenario: The empty result
    When a read matches nothing
    Then it returns no rows and no resume key

  Scenario: A resume key of another transaction is refused
    When a resume key from one read-write transaction's read is used with the same pattern in another read-write transaction
    Then the read is refused with Error 1, the message naming the resume key

  Scenario: A reader at a named version resumes after a reconnect
    When a read-only transaction at snapshot level reads with limit 2, its connection is lost, and a new Begin names the version the first Begin answered and resumes with the key
    Then the read returns rows 3 onward at that version, neither repeating nor skipping a row

  Scenario: A limit stopping inside one object's statements resumes there
    When `<a> <knows> <b>` is held under three statement IDs, and a read with limit 2 is resumed
    Then the first answer returns the first two statements by ascending ID, the second the third, and the partition reads none of the list's prefix

  Scenario: A limit stopping after an object with no statement ID skips it whole
    When `<a> <knows> <b>` and `<a> <knows> <c>` are held with no statement ID, and a read with limit 1 is resumed
    Then the resumed Scan's `after` names `<b>`, compute drops `<b>` from what it answers, and the read returns `<c>`

  Scenario: A limit stopping inside a statement's record resumes there
    When a statement's record holds three properties, and a read of its reifier with limit 2 is resumed
    Then the second answer returns the third property alone

  Scenario: Only the last batch holds a key
    When a read with limit 10,000 stops at its limit after several batches
    Then section 6 of every batch but the last is empty, and the last holds the key
