# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Sequence blocks
  The scenarios of "Sequences are unique per type and leased in blocks per partition"
  (openspec/changes/stage-5-5-three-role-refactor/specs/system-identity/spec.md), verbatim. Bound on the group's side
  only: the blocks are leased from the metadata group over its log and handed to each partition through `LeasedBlocks`,
  its `BlockSource`; the partition allocating from them is a model of a partition's allocator taking sequences in
  order from the blocks it holds (group 3's store allocates the same way, against `TestBlockSource`, in its own
  suite).

  Rule: Sequences are unique per type and leased in blocks per partition

    Scenario: Two partitions never collide
      When partitions 1 and 2 each lease a block of the `Resource` type and allocate every sequence in it
      Then no sequence is allocated twice, and the two blocks do not overlap

    Scenario: A restarted partition does not reuse a sequence
      When a partition allocates sequences 1 to 10 of a block 1..1000, crashes, and restarts
      Then it allocates no sequence it allocated before the crash, and resumes from a sequence above 10 or a new block

    Scenario: A used-up block gives way to the next
      When a partition holding the block 1000..2000 and the next block 5000..6000 allocates after sequence 1999
      Then it allocates sequence 5000

    Scenario: A sequence outside a held block is refused
      When a partition holding only the block 1000..2000, with the metadata group unreachable, allocates after sequence 1999
      Then it refuses with an error naming the `Resource` type, and allocates nothing
