# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Compute's readiness follows its system group
  openspec/changes/stage-5-9-compute-role/specs/role-binaries/spec.md, "Readiness, health and shutdown": compute's
  `/ready` withdrawn while its connection to the system group is down. Compute and a storage process hosting the
  group as processes, settings served by the config double.

  Scenario: Compute is unready while its system group is unreachable
    When compute is ready, and the hosting storage process is killed and started again within `join_deadline_ms`
    Then compute's `/ready` answers 503 once its connection to the system group has closed while its `/health` answers 200, and its `/ready` answers 200 again once the group answers it, Members lists every partition and one answers
