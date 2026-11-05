# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

# Each needs an image in a container, so its binding comes with task 11.4, which runs them in Docker. `tests/bdd.rs` leaves out the tag.
@image
Feature: The role-binaries scenarios that need an image
  openspec/changes/stage-5-5-three-role-refactor/specs/role-binaries/spec.md

  Scenario: A role image holds its binary alone
    When the storage image is built
    Then it holds the storage binary as its entrypoint on the glibc base, for amd64 and arm64, runs as user 65532, and holds no shell and no other role's binary

  Scenario: The development container uses Unix sockets
    When the development image starts with its default profile
    Then the frontend and compute connect through `unix:///run/ultragraph/<role>.sock`, each partition through `unix:///run/ultragraph/storage-p<partition>.sock`, and a write and a read through the frontend succeed

  Scenario: The same frames cross TCP
    When the three roles run in three containers configured with `tcp://` addresses
    Then the same write and read succeed, and the frames each role exchanges are byte-identical to the Unix-socket run's, apart from the transport's own framing

  Scenario: The seccomp profile enables io_uring
    When a role starts under the shipped seccomp profile
    Then it starts on io_uring and logs that it does
