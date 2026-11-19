# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Compute and a lost system group
  openspec/changes/stage-5-9-compute-role/specs/system-group/spec.md, "A lost system group is waited for until the deadline":
  compute's part, over the role binaries as processes, one storage process hosting the system group.

  Scenario: A compute node started before storage waits
    When compute starts while no process listens at `cluster.system_address`, and the hosting storage process starts within the deadline
    Then compute dials until it is answered, and its `/ready` answers after storage's

  Scenario: A group gone past the deadline ends its dependants
    When the hosting storage process is killed and does not return within `join_deadline_ms`
    Then every compute node shuts down through its ordinary path and exits with code 69 naming the system group's address

  Scenario: A commit during the outage is refused, not lost
    When compute's connection to the group is closed and a client commits through its session
    Then the Commit is answered code 17 and reaches no partition, and the same Commit succeeds once the group answers again

  Scenario: An outage bans no source
    When one address authenticates 80 times while the system group cannot be reached, and once after it answers again
    Then each of the 80 is answered Error 17 and counted as no failure, and the last authenticates
