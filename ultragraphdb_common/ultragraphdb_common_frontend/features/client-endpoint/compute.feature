# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Reads are spread over the compute processes
  openspec/changes/stage-5-5-three-role-refactor/specs/client-endpoint/spec.md

  Scenario: Reads alternate between two compute processes
    When two compute processes have joined and a connection sends two reads
    Then each process receives one

  Scenario: A closed compute process is skipped
    When the connection to one of two compute processes closes under a read before any frame of it reached the client
    Then the frontend sends the read to the other, and the client receives its rows

  Scenario: No compute process can be reached
    When no connection to a compute process can be opened for a read
    Then the client receives code 17, and its connection stays open

  Scenario: A read is never resent after a frame
    When a compute process's connection closes after one frame of a read reached the client
    Then the read is refused with code 17, no other compute process receives it, and the client's connection stays open

  Scenario: A Result frame past its credit closes the frontend's role connection
    When a compute process sends a ComputeRead's stream a Result frame with the stream's credit spent, the frames its credit and a Credit allowed having reached the client and a second read being open on the connection
    Then the frontend closes the role connection, answers the first read code 17, and sends the second to the other compute process

  Scenario: A hung compute process answers code 17
    When a compute process accepts a read of `deadline_ms` 50 and sends nothing, `scan_stall_ms` is 100, and the read's stream holds credit
    Then the frontend cancels the ComputeRead and answers the read with code 17 once 150 ms have passed, and no other compute process receives it

  Scenario: A compute process waiting within the deadline is not hung
    When a read of `deadline_ms` 40,000 waits 35 s for a close, and compute sends its first frame once the close arrives
    Then the client receives the rows

  Scenario: A frame restarts a hung compute's timer
    When a compute process sends a read's frames with gaps below `scan_stall_ms`, past the read's deadline
    Then the read goes on to its `LAST`

  Scenario: A stream without credit holds off a hung compute's timer
    When a read's stream has spent its credit, and its client takes no batch for longer than `scan_stall_ms`
    Then the frontend does not answer it with code 17 on that account
