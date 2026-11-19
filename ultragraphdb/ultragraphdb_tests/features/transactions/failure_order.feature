# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Exactly-once through a compute node ahead of storage and a core behind the closes
  openspec/changes/stage-5-8-system-group/specs/transactions/spec.md, "Exactly-once commits through sessions", under a failure of design D12, through the real frontend, compute and storage, the system group hosted by the first storage process, the client crates' own client sending every request. Each Commit writes a statement of a new blank node, so a Commit applied twice holds two. A Commit the fault leaves in doubt is resolved by its session's resend, sent again after a delay while it stays in doubt (`PROTOCOL.md` §11.5), and the data read back holds every Commit acknowledged or resolved committed exactly once and no other.

  Scenario: A compute node started before storage commits each once
    When compute starts before any storage process, the storage process hosting the system group starts, and sessions commit at partitions 1 and 2 as the second storage process starts
    Then compute answers not ready until storage has started, every Commit in doubt is resolved by its session's resend, every Commit acknowledged or resolved committed is held exactly once and no other, and each session's next Commit commits

  Scenario: A Drop through an endpoint core one close behind is refused by the partition
    When a catalog version of the tenant takes effect while compute's endpoint cores hold none of the closes announcing it, and a client sends a Drop of the tenant's data through its session naming the version Describe gave
    Then the partition refuses the Drop with code 2, kind 5, removing nothing; once the closes arrive, the Drop sent again with its request number, naming the version Describe then gives, removes the data once, and the session's next request commits and is held
