# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Data requests, drops and one-frame writes through the three binaries
  openspec/changes/stage-5-5-three-role-refactor/specs/wire-protocol/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request.

  Scenario: Describe answers from the metadata group
    When `name` is declared a property with a `term` index, `score` holds a default entry, and a client sends Describe for the tenant
    Then the frontend answers asking no other role: `read_version` is the version bound of the latest closed epoch, the declarations list `name` with its `term` index, the property keys hold `name` and `score`, and the relationship types hold `score`

  Scenario: A commit-time operation's node is named by its IRI
    When a transaction's insert-if-absent finds the node `http://ex.org/x` holding the value, and the transaction commits
    Then the Commit reply names `http://ex.org/x` by its text, and holds no system ID

  Scenario: A graph drop removes the graph
    When a client drops graph `g` of a tenant holding statements in `g` and in `h`
    Then the reply carries the drop's commit version, no statement of `g` remains, and every statement of `h` remains

  Scenario: A transaction writing into a dropped graph aborts
    When a transaction reads before a drop of graph `g` commits and then writes into `g`
    Then its commit is refused with a conflict of kind Drop

  Scenario: A client naming a principal is refused
    When a client's DirectWrite holds a `principal` above 0
    Then it is refused at the client endpoint with `FieldValue` naming `principal`, and no partition sees it

  Scenario: A malformed write is refused
    When a DirectWrite holds no change, or a ConditionalWrite no operation
    Then it is refused with `CountMismatch`, naming `changes` or `operations`

  Scenario: A stale statement is refused
    When a statement is prepared, the catalog of its tenant changes, and the statement is executed
    Then the execution is refused as stale, and preparing it again succeeds

  Scenario: A literal at a node slot is refused
    When a statement's subject parameter is bound to a literal
    Then the execution is refused with code 11, `detail` the statement

  Scenario: A repeated term is sent once per batch
    When a batch of 1,000 rows names the same IRI in every row
    Then its term table holds that IRI once

  Scenario: A code's retry names what runs again
    When a commit is refused with code 2, and a read with code 24
    Then the client's error says of the first that the whole transaction runs again on a new Begin, and of the second that the read runs again

  Scenario: A Commit crossing its stream's Error is late
    When a client's Commit crosses the Error that ended its transaction's stream
    Then the server drops the Commit, and the connection stays open

  Scenario: A read crossing its transaction's Error takes that Error
    When a client's Read crosses the Error that ended its transaction's stream
    Then the read is answered with that Error, as the transaction's reads still open are

  Scenario: A late frame reaches the client
    When a Result frame arrives for a read the client has cancelled
    Then the client checks the frame and drops it, and the connection stays open
