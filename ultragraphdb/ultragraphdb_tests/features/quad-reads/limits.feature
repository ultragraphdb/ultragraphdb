# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A read when its transaction passes its limit
  openspec/changes/stage-5-5-three-role-refactor/specs/quad-reads/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request. It shares the one wait of `transaction_ms` the first run runs.

  Scenario: A read under way when its transaction ended takes the stream's Error
    When a read of a transaction is under way, the transaction passes its limit, and compute's Error 1 of `detail` 1 reaches the frontend before the Bind stream's code 4
    Then the frontend holds that Error until the Bind stream ends, and the client's read is refused with code 4
