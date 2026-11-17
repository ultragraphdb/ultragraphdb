# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A storage process and a broken connection to the system group
  openspec/changes/stage-5-8-system-group/specs/system-group/spec.md, "A lost system group is waited for until the deadline":
  a storage process that does not host the group, over a group the test plays, which breaks the connection the process
  subscribes on or reads a catalog version on. The frontend's part is bound in ultragraphdb_frontend_tests, compute's in ultragraphdb_compute_tests.

  Scenario: A connection error ending a subscription's connection is joined again
    When the system group ends the connection a storage process that does not host it subscribes on with Error 1 on stream 0
    Then the process joins and subscribes again on a new connection, and runs on

  Scenario: A frame on a stream storage has not opened closes the subscription's connection
    When the system group sends a storage process that does not host it a frame on a stream it has not opened, on the connection it subscribes on
    Then the process closes that connection before it joins again on a new one, subscribes again, and runs on

  Scenario: A late frame among a catalog version's frames is dropped
    When the system group answers a storage process that does not host it a catalog version in two Catalog frames, and sends between them an Error on a stream the process has ended
    Then the process reads the version whole on that connection, joins, and runs on

  Scenario: A catalog version's frame on stream 0 closes the connection
    When the system group sends a storage process that does not host it the second of a catalog version's Catalog frames on stream 0
    Then the process closes that connection before it reads the catalog again on a new one, joins, and runs on

  Scenario: A catalog version's frame on a stream storage has not opened closes the connection
    When the system group sends a storage process that does not host it the second of a catalog version's Catalog frames on a stream it has not opened
    Then the process closes that connection before it reads the catalog again on a new one, joins, and runs on
