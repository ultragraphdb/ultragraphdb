#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: The term dictionary
  Each IRI and skolemized blank node maps to one UID per namespace, never reused, its entity-type bits zero; the
  dictionary lives in the keyspace without versions and is written only in commit batches.

  Scenario: An IRI keeps its UID
    When <a> is written in two separate commits
    Then both name the same UID

  Scenario: The type bits are zero
    When a UID is assigned to a new IRI
    Then its top 16 bits are zero

  Scenario: The second creator retries
    When two writes read before either commits, both create <new>, and both commit
    Then the first commits, the second fails with "Transaction has been aborted. Please retry", and <new> has one UID

  Scenario: An aborted write leaves no entry
    When a write creating <gone> is refused at commit
    Then the dictionary holds no entry for <gone>

  Scenario: A read at an older timestamp is unaffected by a newer term
    When <late> is created at 10 and a read at 5 lists every statement of the default graph
    Then the read returns no statement naming <late>

  Scenario: A dangling hash is reported
    When a list names a literal hash whose dictionary entry is missing and the list is read
    Then the read fails with an error naming the list's key, and nothing panics
