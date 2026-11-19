# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Quad reads through compute under faults
  openspec/changes/stage-5-5-three-role-refactor/specs/quad-reads/spec.md, through the compute process's client endpoint, over real storage processes and a played system group.

  Rule: Compute relays what a partition refuses, and refuses a frame it cannot trust

    Scenario: A partition waiting its turn in a merge fails no read
      When a partition's Scan stalls behind the merge, once between two keys and once inside a list
      Then the read reaches `LAST` with each row once, and the Scan went on from where it stopped

    Scenario: A Scan that stalled before giving a key refuses the read
      When a partition's Scan stalls before giving a key
      Then the read is refused with code 24, `detail` that partition

    Scenario: A corrupt stored record fails its own read alone
      When a statement's stored record does not read, and a client's connection then reads another list of that partition
      Then the first read is refused with code 17, and the second is answered over the same connection to the partition

    Scenario: A limit asks only its own rows' names
      When a read with limit 2 reads a list of many objects held by hash
      Then it asks the partition's dictionary for the names of those rows alone

    Scenario: A client holding no credit holds a frame's worth of names
      When a client of no further credit reads a list whose objects' texts come to many frames
      Then the partition sent no more than a few frames of them

    Scenario: Concurrent first reads of a core share one connection per partition
      When two lookups start together on a core that holds no connection
      Then each partition took one connection

  Rule: A read outlasting the retention keeps its version

    Scenario: A pin refused holds up no other
      When a lookup outlasts its retention over three partitions, and one partition refuses its pin
      Then the other partitions' pins go on taking frames
