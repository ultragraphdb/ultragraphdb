# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Quad lookups and retention through compute
  openspec/changes/stage-5-5-three-role-refactor/specs/quad-reads/spec.md, through the compute process's client endpoint, over real storage processes and a played system group.

  Rule: Compute executes quad-pattern reads over storage

    Scenario: A value-index lookup asks every partition
      When partitions 1, 2 and 3 each hold a subject whose `name` the `term` index lists under `ann`, and compute's API looks up `ann`
      Then compute asks the three partitions, and the API returns the three subjects merged in key order

  Rule: Resume keys continue their own read

    Scenario: An unlimited read that fails restarts
      When a read with no limit is refused with code 24 after three batches, and its caller runs it again
      Then no batch held a resume key, and the second run returns from the first row

  Rule: Compute relays what a partition refuses, and refuses a frame it cannot trust

    Scenario: A partition's refusal is relayed with its detail
      When a partition refuses a Scan with Error 1 of `detail` 1
      Then compute relays the Error with its `detail`

    Scenario: A frame compute refuses closes the connection
      When a partition's answer holds a frame compute refuses, while another read waits on that connection
      Then compute closes the connection, answers the read with code 17 and sends it no Scan again

    Scenario: A partition that stops answering fails the read in time
      When a partition takes a Scan and sends nothing more, its connection open
      Then compute answers the read with code 17 naming the partition, within twice its `scan_stall_ms`, and dials it again

  Rule: A read outlasting the retention keeps its version

    Scenario: A read past the retention keeps its version
      When a read of two graphs stalls on its client's credit past `read_retain_ms`, and goes on
      Then the read reaches `LAST`, and no Scan of it is refused with code 3

    Scenario: A late read keeps its version between Scans
      When a read at snapshot level has its dictionary Scan answered, and its list Scan arrives past `read_retain_ms` while later epochs close
      Then the read reaches `LAST` with its row, and no Scan of it is refused with code 3

    Scenario: A read at latest level holds no version at first
      When a read at latest level and a read at snapshot level each answer within a quarter of `read_retain_ms`
      Then the read at snapshot level opened one Scan holding its version, and the read at latest level none
