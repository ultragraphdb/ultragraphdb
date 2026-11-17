# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Exactly-once through the frontend's connection to the system group closed
  openspec/changes/stage-5-8-system-group/specs/transactions/spec.md, "Exactly-once commits through sessions", under a failure of design D12, through the real frontend, compute and storage, the system group hosted by the first storage process, the client crates' own client sending every request. Each Commit writes a statement of a new blank node, so a Commit applied twice holds two. A Commit the fault leaves in doubt is resolved by its session's resend, sent again after a delay while it stays in doubt (`PROTOCOL.md` §11.5), and the data read back holds every Commit acknowledged or resolved committed exactly once and no other.

  Scenario: The frontend's connection to the system group closed and dialled again commits each once
    When three sessions each have a Commit at partition 2 as the frontend's connection to the system group closes, a fourth session commits before the frontend has dialled again, and the frontend dials again
    Then each Commit at partition 2 is acknowledged, every Commit in doubt is resolved by its session's resend, every Commit acknowledged or resolved committed is held exactly once and no other, and each session's next Commit passes its check at once
