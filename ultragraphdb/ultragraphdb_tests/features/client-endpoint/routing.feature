# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

@both_carriers
Feature: Requests are routed through the three binaries
  openspec/changes/stage-5-5-three-role-refactor/specs/client-endpoint/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request; run over TCP and again over Unix sockets. The second scenario is beyond the spec: a one-frame write read back at the default level.

  Rule: Requests are routed to the partitions and compute that serve them

    Scenario: A write reaches its partition
      When a client writes `<http://ex.org/a> <http://ex.org/knows> <http://ex.org/b>`
      Then the frontend sends it to the partition owning the bucket of `http://ex.org/a`, and the commit's reply reaches the client

    Scenario: A one-frame write is read at the default level
      When a client writes `<a> <knows> <b>` in one frame, and reads `<a> <knows> ?o` in read-only transactions at the default level
      Then a read returns `<b>` at the snapshot level, and no read returns anything else
