# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The metadata group in the system group
  The scenarios of openspec/changes/stage-5-8-system-group/specs/metadata-group/spec.md this change added or rewrote,
  verbatim, bound through the system group; the scenarios it left as they were stay bound at the state machine in
  `ultragraphdb_cluster_metadata`.

  Rule: Catalog versions take effect at an epoch boundary

    Scenario: A default entry is no catalog version
      When version 3 of `t1` is current, a first write of `score` records its default entry, and a client offers version 4 built on version 3
      Then the default-entry record of (`t1`, `score`) is held, version 4 is accepted, and no prepared statement of `t1` is stale

    Scenario: A catalog version past one frame travels in several
      When a catalog version of 523,001 bytes is committed, and a storage process at the floor of `max_frame` reads it with Catalog
      Then two Catalog frames hold its bytes, the second alone flagged `LAST`, and the bytes read equal the bytes committed

    Scenario: A catalog version at its cap is held and read
      When a catalog version of 16,777,216 bytes is committed, the system group's host restarts, and a storage process reads the version with Catalog
      Then the group holds the version after recovery, and the bytes read equal the bytes committed

  Rule: Principals live in the metadata group

    Scenario: An unknown name keeps one salt across a restart
      When a client authenticates as the unknown name `nobody`, the system group's host and compute restart, and a client authenticates as `nobody` on another core
      Then both challenges carry one salt, and the metadata group's log holds the secret it derives from

  Rule: The metadata group holds the tenants

    Scenario: The first start creates tenant 0
      When the system group's host starts on a metadata group holding no tenant
      Then the group holds tenant 0 durably before it listens, Placement's runs list it, and a restart creates none

    Scenario: A Bind naming an unknown tenant is refused
      When a Bind names tenant 9, for which the partition's placement map lists no run
      Then the partition refuses it with Error 1, the message naming tenant 9, and holds no transaction
