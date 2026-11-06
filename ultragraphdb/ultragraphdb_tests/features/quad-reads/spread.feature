# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Reads over a spread tenant
  openspec/changes/stage-5-5-three-role-refactor/specs/quad-reads/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request.

  Scenario: A bound subject asks one partition
    When a read binds its subject to `http://ex.org/a` over two partitions
    Then compute asks only the partition of `http://ex.org/a`'s bucket, and returns its rows

  Scenario: A bound literal object fans out
    When `<a>` in partition 1 and `<c>` in partition 2 each hold `name` `"Ann"`, and a read binds its object to `"Ann"` and its predicate to `name`
    Then compute asks every partition of the tenant's run, reads no referrer set, and returns `<a>` and `<c>` merged in key order

  Scenario: An unbound subject merges every partition
    When a read leaves its subject and object unbound over two partitions that each hold matching rows
    Then compute returns the rows of both partitions merged in key order

  Scenario: A vector object reads the forward lists
    When `<a>` in partition 1 and `<c>` in partition 2 each hold `embedding` `v`, `<d>` holds another vector, and a read binds its object to `v` and its predicate to `embedding`
    Then compute asks every partition of the tenant's run for the forward lists of `embedding`, and returns `<a>` and `<c>` alone

  Scenario: A reverse read gathers every referrer
    When `<a>` in partition 1 and `<c>` in partition 3 each hold `knows` to `<b>`, homed in partition 2, and a read binds its object to `<b>` and its predicate to `knows` at a closed epoch
    Then compute reads `R(b)` at partition 2, asks partitions 1 and 3, and returns `<a>` and `<c>`

  Scenario: A latest-state read across partitions is refused
    When a transaction at latest-state level reads with its subject unbound over two partitions
    Then the read is refused with an error naming the level and the two partitions

  Scenario: A later latest-state read waits for the first read's partition
    When a transaction at latest level sends two reads beside each other, the first binding it to partition 1
    Then the frontend sends the second only after the first's first Result names partition 1, with `partition` 1, and compute refuses it with code 25 when it reaches another partition

  Scenario: A first read refused leaves the next read first
    When a transaction at latest level sends two reads, and the first is refused before any Result
    Then the second goes out as the transaction's first read and binds it
