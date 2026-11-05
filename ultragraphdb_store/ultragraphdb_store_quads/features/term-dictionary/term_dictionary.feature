#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: The term dictionary
  Each IRI and skolemized blank node maps to one system ID per tenant, never reused: an IRI's bucket computed from
  the tenant and the IRI, a blank node's stated by its skolem IRI, and its sequence drawn from a leased block. An
  IRI's ID is got or created while the writing transaction executes and shared by every writer of it; the dictionary,
  a keyspace without versions, is naming state only
  (openspec/changes/stage-5-5-three-role-refactor/specs/term-dictionary/spec.md): the scenarios the quad store decides
  alone, verbatim.

  Scenario: An aborted write leaves nothing that exists
    When a write creating `<gone>` is refused at commit, and a read then asks for every statement naming `<gone>`
    Then the read returns nothing, whether or not the dictionary holds an entry for `<gone>`

  Scenario: A read at an older version is unaffected by a newer term
    When `<late>` is first written at epoch 10 and a read at epoch 5 lists every statement of the default graph
    Then the read returns no statement naming `<late>`

  Scenario: A dangling hash is reported
    When a list names a literal hash whose dictionary entry is missing and the list is read
    Then the read fails with an error naming the list's key, and nothing panics

  Scenario: A foreign reference resolves locally
    When partition 1 holds `<a> <knows> <b>`, `<b>` homed in partition 2, and reads `<a>`'s `knows` while partition 2 is unreachable
    Then the read names `<b>` by its IRI from partition 1's dictionary

  Scenario: An IRI keeps its ID
    When `<a>` is written in two separate commits
    Then both name the same system ID

  Scenario: An IRI's bucket is its own
    When a new IRI receives a system ID
    Then its type prefix is 1 and its bucket equals the bucket computed from its tenant and canonical encoding

  Scenario: A blank node's skolem IRI names its bucket and nothing else
    When partition 1, owning the first 21,846 buckets of tenant 0's run, mints a blank node with its random source seeded to a recorded value
    Then the skolem IRI is the tenant's skolem base, `.well-known/genid/`, 6 hexadecimal digits naming one of those buckets, `-` and 26 hexadecimal digits; the node's ID carries that bucket; and the same seed mints the same IRI

  Scenario: An aborted creator leaves the IRI usable
    When a transaction creates `<new>` and aborts, and a later transaction writes `<new>`
    Then the later transaction commits, and `<new>` has one system ID
