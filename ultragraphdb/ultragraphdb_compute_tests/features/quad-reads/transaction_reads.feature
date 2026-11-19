# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Quad reads of a transaction through compute
  openspec/changes/stage-5-5-three-role-refactor/specs/quad-reads/spec.md, of a read-write transaction a client opens at the compute process's client endpoint, over real storage processes and a played system group.

  Rule: A read through compute is a read of its transaction

    Scenario: A scan through compute holds no list in the transaction
      When a transaction scans 10,000 lists through compute
      Then its read set holds one range and no list key, and it holds none of those lists in memory afterwards

    Scenario: A read past the read set's room ends its transaction
      When a transaction's reads leave no room for a span
      Then the read is refused with `TooLarge`, no entry of that span reaches the client, and the transaction's next request is refused

    Scenario: A span recorded after a Cancel only widens the commit's check
      When a Commit cancels a read whose Scan records one more span before the Cancel reaches the partition, and another transaction wrote into that span
      Then the commit aborts with a conflict of kind Range, and no frame of that span reached the client
