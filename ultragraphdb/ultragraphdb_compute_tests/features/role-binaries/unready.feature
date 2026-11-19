# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Compute's readiness follows its system group
  openspec/changes/stage-5-9-compute-role/specs/role-binaries/spec.md, "Readiness, health and shutdown": compute's
  `/ready` withdrawn while a connection to the system group is down, and given only once every runtime's holds. Compute
  and a storage process hosting the group as processes, settings served by the config double.

  Scenario: Compute is unready while its system group is unreachable
    When compute is ready, and the hosting storage process is killed and started again within `join_deadline_ms`
    Then compute's `/ready` answers 503 once its connection to the system group has closed while its `/health` answers 200, and its `/ready` answers 200 again once the group answers it, Members lists every partition and one answers

  Scenario: Compute is ready only once every runtime's link holds
    When compute's runtimes' connections to the system group open seconds apart, and as soon as its `/ready` answers, clients open four logins for each of its runtimes at once
    Then every login authenticates, and none is refused with code 17
