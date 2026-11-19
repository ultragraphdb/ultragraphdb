# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Exactly-once through compute killed during a tightening's check
  openspec/changes/stage-5-8-system-group/specs/transactions/spec.md, "Exactly-once commits through sessions", under a failure of design D12, through the real compute and storage, the system group hosted by the first storage process, the client crates' own client sending every request. Each Commit writes a statement of a new blank node, so a Commit applied twice holds two. A Commit the fault leaves in doubt is resolved by its session's resend, sent again after a delay while it stays in doubt (`PROTOCOL.md` §11.5), and the data read back holds every Commit acknowledged or resolved committed exactly once and no other.

  Scenario: Compute killed during a tightening's check commits each once
    When `nick` is being checked single-valued against partition 2, three sessions each have a Commit at partition 2, and compute is killed and started again
    Then every Commit in doubt is resolved by its session's resend through the new compute node, every Commit acknowledged or resolved committed is held exactly once and no other, each session's next Commit commits, and the tightening takes effect once
