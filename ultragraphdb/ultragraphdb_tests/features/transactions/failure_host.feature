# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Exactly-once through the system group's host killed
  openspec/changes/stage-5-8-system-group/specs/transactions/spec.md, "Exactly-once commits through sessions", under a failure of design D12, through the real compute and storage, the system group hosted by the first storage process, the client crates' own client sending every request. Each Commit writes a statement of a new blank node, so a Commit applied twice holds two. A Commit the fault leaves in doubt is resolved by its session's resend, sent again after a delay while it stays in doubt (`PROTOCOL.md` §11.5), and the data read back holds every Commit acknowledged or resolved committed exactly once and no other.

  Scenario: The hosting storage process killed with commits in flight commits each once
    When three sessions each have a Commit at partition 2, held by the second storage process, as the storage process hosting the system group is killed, a fourth session commits while the group is gone, and the host is started again
    Then every Commit in doubt is resolved by its session's resend, every Commit acknowledged or resolved committed is held exactly once and no other, and each session's next Commit commits
