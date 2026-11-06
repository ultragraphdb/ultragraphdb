# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Access paths through the three binaries
  openspec/changes/stage-5-5-three-role-refactor/specs/quad-reads/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request.

  Scenario: A resume key holds terms, not keys
    When a read with limit 1 over `<a> <knows> ?o` returns `<b>` and its resume key is decoded by the server
    Then the key holds the pattern's hash, its `scope` and the text `http://ex.org/b`, and no system ID or store key

  Scenario: A bound object reads the reverse keyspace
    When a read binds its object to `http://ex.org/b` and its predicate to `http://ex.org/knows`
    Then it reads the reverse lists of `http://ex.org/b` under `http://ex.org/knows` in the partitions `R(b)` names, and returns each subject

  Scenario: An unbound graph reads every graph
    When `<a> <p> <b>` is held in the default graph and in `<g1>`, and a read leaves the graph unbound
    Then it returns both statements, each with its graph

  Scenario: A statement without a statement ID is named by its quad
    When `<a> <knows> <b>` is held without a statement ID and a read returns it
    Then the row names the statement by `<a>`, `<knows>`, `<b>` and its graph alone

  Scenario: Parallel edges are told apart by their reifiers
    When `<a> <knows> <b>` is held without a statement ID, a parallel edge of it is inserted, and a read returns both
    Then two rows return, each carrying its own statement's reifier IRI, and neither carries a statement ID

  Scenario: A reverse read names each parallel edge's reifier
    When `<a> <knows> <b>` is held as two parallel edges, and a read binds its object to `<b>` and its predicate to `knows`
    Then two rows return for `<a>`, each with its statement's reifier from the reverse list's object record, and compute sends no Scan of a forward list

  Scenario: An unbound graph is walked by Scans of limit 1
    When `<a> <p> <b>` is held in `<g1>` and `<g2>`, and a read of `<a> <p> ?o` leaves the graph unbound
    Then compute finds each graph by a Scan of limit 1 from the keyspace's tenant and then from just past the graph found, cancels each after its first frame, and returns both statements

  Scenario: A reifier's skolem IRI reads its statement's properties
    When `<a> <knows> <b>` carries `<since> 2020` under the reifier `r`, and a read binds its subject to `r`
    Then compute reads the statement's record in the statements keyspace of the partition `r`'s bucket names, and returns one row for `<since>` and `2020` with `ABSENT` in the reifier column
