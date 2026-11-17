# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A partition goes on committing while the system group is lost
  openspec/changes/stage-5-8-system-group/specs/system-group/spec.md, "A lost system group is waited for until the
  deadline": a partition's part. The partition's store holds a version near the end of its open epoch, epoch 2, the latest
  close it holds being epoch 1, and no close reaches it while it commits, as none does once the process hosting the group
  is gone: an epoch's last commit sequence is 2^22 − 2, so the scenario starts near it rather than committing up to it.

  Scenario: A partition without closes commits until its epoch is full
    When the hosting storage process is killed, and a client commits through a session-less write to a partition of another storage process until that partition's open epoch is full
    Then each commit before that is acknowledged, and the first past it is refused with code 5 once `close_request_ms` has passed with no close
