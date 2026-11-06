# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Requests name terms by their text, through the three binaries
  openspec/changes/stage-5-5-three-role-refactor/specs/wire-protocol/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request.

  Scenario: A skolem IRI nobody can hold binds nothing
    When a read-write transaction's first request is a Read binding its subject to an IRI under tenant 7's skolem base in tenant 0, and its next request is a Write of `<a>`
    Then the Read is answered one batch flagged `LAST`, `rows` 0, no role sees it, and the transaction binds to `<a>`'s partition

  Scenario: A skolem IRI nobody can hold is refused in a change
    When a Write names as a subject an IRI under the tenant's skolem base whose tail states no bucket, and another under tenant 7's
    Then each is refused with Error 7, `detail` the term's index, and no partition is asked

  Scenario: The same path under another base is an ordinary IRI
    When a Write names `https://example.org/tenant/0/.well-known/genid/00abcd-0123456789abcdef0123456789`
    Then it is routed by its text's hash and stored as any IRI

  Scenario: A result names IRIs by their text
    When a read returns rows whose subjects are `http://ex.org/a` and `http://ex.org/b`
    Then the batch's dictionary holds both IRIs' texts and the rows name them by dictionary index, with no system ID

  Scenario: A result names a blank node by its opaque skolem IRI
    When a read returns a row whose subject is a blank node
    Then the batch names it, tagged as a blank node, by its skolem IRI `…/.well-known/genid/{bucket}-{random}`, whose text holds no type prefix and no sequence of its system ID

  Scenario: A literal at a pattern's subject is refused
    When a Read's subject slot holds a literal, and another's graph slot a blank node's label
    Then each is refused with Error 7, `detail` the term's index, the message naming the rule

  Scenario: A label or a triple term at a pattern's object is refused
    When a Read's object slot holds a blank node's label, and another's a triple term
    Then each is refused with Error 7, `detail` the term's index, the message naming the rule

  Scenario: A triple term in a change is refused
    When a Write's object is a triple term
    Then the Write is refused with Error 7, `detail` the term's index, and the transaction ends

  Scenario: A forged skolem IRI in a pattern matches nothing
    When a Read names as its subject a skolem IRI no dictionary holds
    Then the read answers one batch flagged `LAST` with no row, and its transaction goes on

  Scenario: A decimal the payload cannot hold is kept by its lexical form
    When a literal `"0.1234567890123456789"^^xsd:decimal`, of 19 fraction digits, is written and read back
    Then its term carries tag `0x31`, a known datatype's code and the hash of its lexical form, and the batch's text holds the lexical form unchanged

  Scenario: A statement with properties is named by its reifier
    When a read returns `<a> <knows> <b>` in `<g1>`, which carries the property `<since> 2020`
    Then the row names the statement by `<a>`, `<knows>`, `<b>` and `<g1>` and by its reifier's skolem IRI, and the batch holds no statement ID
