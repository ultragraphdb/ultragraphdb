# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Compute runs a client's read on its connection's core, one frame a step
  openspec/changes/stage-5-9-compute-role/specs/quad-reads/spec.md, through the executor a client's session hands its reads to, over played partitions; the client's connection is played by the scenario, which gives a read room for its next batch as the connection's credit allows.

  Scenario: A commit is relayed between a read's frames
    When a read merging rows of four partitions is under way on a core, and a Commit of another connection arrives on that core
    Then the Commit is relayed to its partition before the read's next Result frame is built

  Scenario: A read's step stays within one frame
    When a read's partitions each have ten Entries frames ready
    Then the read consumes one frame, yields, and consumes the next only after every other ready task of the core has run a step

  Scenario: A fan-out stays on its read's core
    When a read fans out over three partitions
    Then its Scans, its merge and its Result frames all run on the core its client's connection arrived on

  Scenario: An unbound subject answers partition by partition
    When a read leaves its subject and object unbound over two partitions that each hold matching rows
    Then compute returns every row of partition 1 and then every row of partition 2

  Scenario: A batch stops at the client's frame
    When a client whose Hello names `max_frame` 524,288 reads more rows than one such frame holds
    Then every Result frame it receives is at most 524,288 bytes, and every row arrives

  Scenario: A row too large for the client's frame is refused
    When a client whose Hello names `max_frame` 524,288 reads a row holding a literal of 600,000 bytes
    Then the read is refused with code 6, `detail` 524,288

  Scenario: A resume key that does not fit goes alone
    When a read stops at its limit, and its rows fill the last batch so that its resume key does not fit
    Then the stream's last batch holds `rows` 0 and the key, spending one credit, and the batch before it is not flagged `LAST`

  Scenario: A read's stream starts with the client's credit
    When a client's Welcome names `initial_credit` 4, and a read's answer takes more frames
    Then compute sends 4 frames, and one more for each frame of the client's Credit

  Scenario: A Cancel stops a read in the turn it is read
    When a client cancels a read whose Scans are open at two partitions
    Then compute cancels both Scans in the turn it reads the Cancel, and no further Result frame is sent

  Scenario: A silent partition ends its reads with code 17
    When a partition accepts a Scan of `deadline_ms` 50 and sends nothing, and `scan_stall_ms` is 100
    Then compute closes the connection once 250 ms have passed, the read is answered code 17, and no Scan of it is sent again

  Scenario: A partition answering within the bound is not hung
    When a partition sends a Scan's frames with gaps below twice `scan_stall_ms`, past the read's deadline
    Then the connection stays open and the read reaches its `LAST`
