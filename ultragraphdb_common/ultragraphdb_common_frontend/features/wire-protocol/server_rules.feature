# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The server's rules of a request
  openspec/changes/stage-5-5-three-role-refactor/specs/wire-protocol/spec.md

  Scenario: A skolem IRI nobody can hold binds nothing
    When a read-write transaction's first request is a Read binding its subject to an IRI under tenant 7's skolem base in tenant 0, and its next request is a Write of `<a>`
    Then the Read is answered one batch flagged `LAST`, `rows` 0, no role sees it, and the transaction binds to `<a>`'s partition

  Scenario: A skolem IRI nobody can hold is refused in a change
    When a Write names as a subject an IRI under the tenant's skolem base whose tail states no bucket, and another under tenant 7's
    Then each is refused with Error 7, `detail` the term's index, and no partition is asked

  Scenario: The same path under another base is an ordinary IRI
    When a Write names `https://example.org/tenant/0/.well-known/genid/00abcd-0123456789abcdef0123456789`
    Then it is routed by its text's hash and stored as any IRI

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

  Scenario: A stale statement is refused
    When a statement is prepared, the catalog of its tenant changes, and the statement is executed
    Then the execution is refused as stale, and preparing it again succeeds

  Scenario: Describe answers from the metadata group
    When `name` is declared a property with a `term` index, `score` holds a default entry, and a client sends Describe for the tenant
    Then the frontend answers asking no other role: `read_version` is the version bound of the latest closed epoch, the declarations list `name` with its `term` index, the property keys hold `name` and `score`, and the relationship types hold `score`

  Scenario: A literal at a node slot is refused
    When a statement's subject parameter is bound to a literal
    Then the execution is refused with code 11, `detail` the statement

  Scenario: A Describe past the client's frame is refused
    When a tenant's Describe answer does not fit one frame within the client's `max_frame`
    Then it is refused with code 6, `detail` that `max_frame`

  Scenario: A statement prepared in another tenant is refused
    When a connection prepares a statement in tenant 1 and executes it in a transaction of tenant 0
    Then the execution is refused with code 10, `detail` the statement, and the transaction goes on

  Scenario: One connection's transactions act in two tenants
    When a connection begins one transaction in tenant 0 and another in tenant 1, and each writes and reads `<a> <p> <b>`
    Then each reads its own statement, and neither reads the other's

  Scenario: A Write on a new stream is refused
    When a client sends a Write, an Operation, an Abort, and a Commit through no session, each on a new stream
    Then each is refused with Error 1 on that stream, the message naming the command

  Scenario: A Commit in a read-only transaction is refused
    When a client sends a Write, an Operation and a Commit, each on the stream of its own read-only transaction
    Then each is refused with Error 1, the message naming the command, and the transaction ends

  Scenario: A Commit crossing its stream's Error is late
    When a client's Commit crosses the Error that ended its transaction's stream
    Then the server drops the Commit, and the connection stays open

  Scenario: A request behind its transaction's Commit is late
    When a client sends a Write and a second Commit on its transaction's stream after the Commit the frontend relayed, and the partition then answers that Commit
    Then the server drops both, and the client receives the Commit's reply

  Scenario: A read crossing its transaction's Error takes that Error
    When a client's Read crosses the Error that ended its transaction's stream
    Then the read is answered with that Error, as the transaction's reads still open are
