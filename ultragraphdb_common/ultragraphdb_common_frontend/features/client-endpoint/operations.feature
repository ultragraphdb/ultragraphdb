# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Changes, operations and claims on their way to a partition
  openspec/changes/stage-5-5-three-role-refactor/specs/client-endpoint/spec.md

  Scenario: Every kind of change goes to its subject's partition
    When a transaction's Write holds an edge, a delete, a statement's delete by its reifier, a pattern's delete, and a property set on a reifier and deleted from a triple, each naming a subject of partition 1
    Then the Write reaches partition 1 on its transaction's Bind stream, and the Commit is answered

  Scenario: An Operation in a transaction goes to its subject's partition, of a blank subject in turn
    When one transaction adds to a counter of a subject of partition 1, another compares and sets its value, and a third adds to a blank node's counter, and each commits, and a fourth's first read names no subject
    Then each Operation reaches the partition on its transaction's Bind stream, each Commit is answered, and the read binds the tenant's one partition

  Scenario: A value of every literal form is claimed at its home
    When ConditionalWrites insert-if-absent a typed literal, a language-tagged one, one of each base direction, an IRI and a literal of a datatype the store names by its ID, and a transaction's Operation upserts a typed literal
    Then each literal's claim is asked of its predicate's home before the write leaves, the IRI and the datatype named by ID go in turn, and each is answered

  Scenario: A claim's home refusing its predicate refuses the write
    When the home of a claim's predicate refuses to register it, for a ConditionalWrite and for a transaction's Operation
    Then each is refused with code 7, and the transaction ends

  Scenario: A claim whose home no connection reaches is unavailable
    When a claim's predicate is homed on a partition no process joined, for a ConditionalWrite and for a transaction's Operation
    Then each is refused as unavailable, and the transaction ends

  Scenario: A claim whose home's connection closes under it is unavailable
    When the connection to the home of a claim's predicate closes while it holds the claims of a ConditionalWrite and of a transaction's Operation
    Then each is refused as unavailable, and the transaction ends

  Scenario: A transaction's first request reaching no joined partition, or two, is refused
    When a transaction's first Write names only a blank node while no partition is joined, another's names a subject of a partition no process joined, and a third's first read names no subject of a tenant whose run spans two partitions
    Then the first two are refused as unavailable, and the read as reaching both partitions
