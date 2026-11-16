# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The system group
  The system group of openspec/changes/stage-5-8-system-group/specs/system-group/spec.md, every scenario of every
  requirement but the two groups 4 and 8 run, verbatim: the group run on the test's runtime over the simulated storage, or
  on a thread of its own over real logs, its role connections served by `SystemSession` over the TCP loopback. What a
  scenario says of a host process or the network binds the group's half here; `notes/group-3.md` names the half groups 4
  and 8 bind. "A system table without its directory stops the start" is bound in `ultragraphdb_common_role`, as group 2
  bound it.

  Rule: One storage instance hosts the system group

    Scenario: The hosting instance serves the books
      When `storage_1` holds a `[system]` table and `storage_2` none, and both start
      Then `storage_1` listens at `system.role_address` and answers a Placement request there, and `storage_2` listens at its partitions' addresses alone

    Scenario: The frontend holds no book
      When the frontend binary's dependency graph is listed
      Then it names neither `ultragraphdb_cluster_metadata`, `ultragraphdb_cluster_sequencer` nor `ultragraphdb_transactions_log`

  Rule: The system group runs on a core of its own over logs of its own

    Scenario: The books recover in the storage process
      When the group holds a catalog version, two block leases, a principal and twelve closes, and its host is killed and restarted
      Then the group's state after recovery equals its state before the kill, read at the same log position, and the sequencer's last closed epoch is 12

    Scenario: The group's logs lie beside the partitions'
      When the development container has started with `system.data_dir = "/data/system"`
      Then `/data` holds `system/meta/`, `system/seq/` and `storage/p1/r1/`

    Scenario: A partition's sync holds up no close
      When a partition's log sync is delayed past `epoch_ms_max` while the group's logs sync at once
      Then the sequencer closes its epochs on time, and the delayed partition receives them once it reads again

  Rule: The system group serves the books' role commands at its role address

    Scenario: A partition of the hosting process dials the group
      When the hosting storage process starts with partition 1
      Then partition 1 opens a role connection to `cluster.system_address`, joins, subscribes and receives the closes on it

    Scenario: A partition of another process is served alike
      When `storage_2`, which hosts no group, starts with partitions 3 and 4
      Then both join at `cluster.system_address`, Members names their addresses, and each receives every close

    Scenario: A command the endpoint does not serve is refused
      When a role connection to the system group sends a Bind
      Then it is refused as `CommandNotServed` naming Bind

    Scenario: A command another role requests is refused
      When a connection whose role is `storage` sends a Verify
      Then it is refused as `CommandNotServed` naming Verify

  Rule: The registry is asked by Session, Check and Ended

    Scenario: A held identity is refused whoever holds it
      When a Session registers a session for `alice`, and the same Session is sent again
      Then the second is refused with Error 1 naming the session, and the registry holds the session once

    Scenario: A session of another principal is refused
      When the registry holds a session for `alice` and a Session names it for `bob`
      Then it is refused with Error 1 naming the session, and the session stays `alice`'s

    Scenario: An unknown principal registers nothing
      When a Session names a principal the metadata state machine does not hold
      Then it is refused with code 7 naming the principal

    Scenario: A frame of checks is answered whole and in order
      When one Check frame holds a kind 2 check of `alice`'s session, a kind 2 check of a session never registered, and a kind 1 check of `bob`'s session naming `alice`
      Then one reply holds the outcomes 1, 12 and 21 in that order, and `alice`'s session alone is marked

    Scenario: A second commit of a marked session is refused
      When a session is marked by a passed check of kind 2, and another check of kind 2 names it before any Ended
      Then the second check's outcome is 14

    Scenario: A resend's check sees the original in flight
      When a session is marked by a passed check of kind 2 on one connection, and a check of kind 1 names it on another
      Then the kind 1 check's outcome is 14, and it marks nothing

    Scenario: A marked session is not evicted
      When the registry holds `sessions_max` sessions, the least recently registered is marked, and one more is registered
      Then the least recently registered without a mark is evicted, and the marked one stays

    Scenario: An Ended sent again changes nothing
      When an Ended names a marked session twice in two frames
      Then the first takes the mark away, the second is passed over, and neither is answered

  Rule: A mark outlives its connection by twice the transaction limit

    Scenario: A closed connection's marks hold for the limit
      When a connection marks a session and closes, and a check of kind 1 names the session on another connection before twice `transaction_ms` has passed, and again after it
      Then the first check's outcome is 14 and the second passes

    Scenario: An orphaned mark is ended from another connection
      When a connection marks a session and closes, and another connection sends an Ended naming the session
      Then the mark ends, and a check of kind 2 naming the session passes

    Scenario: Another open connection's Ended leaves a mark
      When one connection marks a session and another sends an Ended naming it while the first is open
      Then the session stays marked

    Scenario: A restarted group answers no resend at once
      When the system group's host restarts, and a check of kind 1 naming a registered session of its principal arrives before twice `transaction_ms` has passed since it began to listen
      Then the check's outcome is 14, a check of kind 2 naming an unmarked session passes, and a Session that would evict is refused with code 14

  Rule: The system group goes on answering checks while a log syncs

    Scenario: A check is answered while a close syncs
      When the sequencer's sync of a close is in flight, and a Check and a Session arrive
      Then the Check is answered before the sync ends, and the Session only after it

    Scenario: An eviction appended is seen by the next check
      When an eviction is appended to the metadata log and its sync is in flight, and a Check names the evicted session
      Then the check's outcome is 12

  Rule: A client's proof is checked in the system group

    Scenario: A proof that holds is answered with its signature
      When a Verify carries `ann`'s name, the exchange's AuthMessage and the proof `ann`'s password gives
      Then the reply holds `verified` 1, `ann`'s principal and a ServerSignature of 32 bytes

    Scenario: An unknown name and a wrong proof answer alike
      When one Verify carries a wrong proof for `ann` and another any proof for the unknown name `nobody`
      Then both replies hold `verified` 0, `principal` 0 and no ServerSignature

    Scenario: No frame carries a stored key
      When a Challenge and a Verify for `ann` are answered
      Then neither reply holds `ann`'s stored key or server key, and no other command the endpoint serves returns them

  Rule: The system group builds the catalog versions it is asked for

    Scenario: A declaring transaction's version is built on the version it names
      When version 3 of `t1` is current and a Declare of kind 1 names `built_on` 3 with one declaration
      Then the group commits version 4 as pending, and answers once a close has assigned its epoch, `version` 4 and `commit_version` the first version of that epoch

    Scenario: A version built on a stale one is refused
      When version 3 of `t1` is current and a Declare of kind 1 names `built_on` 2
      Then it is refused with code 2, kind 5, the message naming versions 2 and 3, and version 3 stays current

    Scenario: A Drop that changes nothing builds no version
      When a Declare of kind 2 names a Drop of a predicate the catalog does not declare
      Then the reply holds `version` 0 at once, and the tenant's current version is unchanged

    Scenario: A check outlives the connection that asked for it
      When a Declare of kind 1 tightens a predicate, its connection closes before any partition reports, and a partition then reports that the data refuses the version
      Then the group holds a version restoring the previous declaration, and a Catalog request of `version` 0 naming the latest closed epoch never returns the tightened one

    Scenario: Defaults lists the recorded predicates in order
      When `t1` holds default-entry records for `score` and `nick`, and a Defaults names `t1`
      Then the reply lists `nick` before `score`, and `position` names the log position they were read at

    Scenario: The storage binary links no schema compiler
      When the storage binary's dependency graph is listed
      Then it names no crate under `ultragraphdb_schema/`
