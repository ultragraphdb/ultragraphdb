# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A read-write transaction stays within one partition
  openspec/changes/stage-5-5-three-role-refactor/specs/partitions/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request.

  Scenario: A transaction reaching a second partition aborts
    When a transaction writes a statement whose subject partition 1 owns and then writes one whose subject partition 2 owns
    Then the transaction aborts with `CrossPartition` naming partitions 1 and 2, and holds nothing

  Scenario: Write skew across partitions is refused, not committed
    When `alice` is homed in partition 1 and `bob` in partition 2, both `on-call` `true`, and a read-write transaction reads `on-call` of `alice` and then of `bob`
    Then the second read aborts the transaction with `CrossPartition` naming partitions 1 and 2, and it writes nothing

  Scenario: A reverse read over a spread tenant aborts a read-write transaction
    When a tenant's run spans partitions 1 and 2, `<b>` is homed in partition 2, and a read-write transaction bound to partition 1 reads the subjects of `knows` pointing at `<b>`
    Then the transaction aborts with `CrossPartition` naming partitions 1 and 2

  Scenario: A reference to another partition's entity stays in one partition
    When a transaction bound to partition 1 writes `<a> <knows> <b>`, `<b>` homed in partition 2
    Then it registers the reference at partition 2, commits in partition 1 alone, and does not abort

  Scenario: A Drop over several partitions is refused
    When a tenant's buckets span partitions 1 and 2, and a client drops the tenant's data
    Then the Drop is refused with `CrossPartition` naming partitions 1 and 2, and nothing is dropped

  Scenario: A delete by pattern of another partition's subject is refused
    When a transaction bound to partition 1 deletes the pattern `<b>`, `knows`, and `<b>` is homed in partition 2
    Then the transaction aborts with `CrossPartition` naming partitions 1 and 2, and every `knows` of `<b>` is still held

  Scenario: A delete by pattern leaves other partitions' edges
    When `<a>`, homed in partition 1, holds `<a> <knows> <b>`, and a transaction in partition 2, `<b>`'s home, deletes the pattern `<b>`, `name`
    Then it commits in partition 2 alone, `<b>` holds no `name`, and `<a> <knows> <b>` is still held
