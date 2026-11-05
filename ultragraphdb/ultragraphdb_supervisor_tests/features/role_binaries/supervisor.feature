# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The supervisor starts, stops and reports the roles
  openspec/changes/stage-5-5-three-role-refactor/specs/role-binaries/spec.md

  Scenario: The start order reaches readiness
    When the supervisor starts the config service and the three roles
    Then the config service answers first, every role's `/ready` answers, the frontend's last, and no role waited on one that was waiting on it

  Scenario: A failing role ends the container with its code
    When the storage process exits with code 74 while the others run
    Then the supervisor stops the frontend and compute and exits with code 74

  Scenario: A role killed by a signal is reported as a shell would
    When the compute process is killed by SIGKILL while the others run
    Then the supervisor stops the frontend and storage and exits with code 137

  Scenario: SIGTERM shuts down in order
    When the supervisor receives SIGTERM
    Then the frontend stops accepting work first, compute drains next, storage flushes last, and the supervisor exits 0

  Scenario: SIGINT shuts down as SIGTERM does
    When the supervisor receives SIGINT
    Then the frontend stops accepting work first, compute drains next, storage flushes last, and the supervisor exits 0

  Scenario: SIGHUP shuts down as SIGTERM does
    When the supervisor receives SIGHUP
    Then the frontend stops accepting work first, compute drains next, storage flushes last, and the supervisor exits 0

  Scenario: SIGQUIT shuts down as SIGTERM does
    When the supervisor receives SIGQUIT
    Then the frontend stops accepting work first, compute drains next, storage flushes last, and the supervisor exits 0

  Scenario: A role stopping cleanly ends the container with code 0
    When the storage process is told to stop by someone other than the supervisor
    Then the supervisor stops the frontend and compute and exits with code 0

  Scenario: A signal during the start stops what was started
    When the supervisor receives SIGTERM while the frontend waits for its partition
    Then it starts nothing more, stops what it started, and exits 0

  Scenario: A variable naming a setting stays out of a role's environment
    When the supervisor starts with `ULTRAGRAPHDB_STORAGE_PARTITION_COUNT=2` in its environment
    Then every role reaches readiness, since a role would refuse the variable

  Scenario: A binary that is not there says so
    When the supervisor starts with no other binary beside it
    Then it exits with code 69 and an error naming the binary it looked for

  Scenario: The data layout matches a node's
    When the development container has started
    Then `/data` holds `meta/`, `seq/` and `storage/p1/r1/`, the paths a cluster node uses for the same roles

  Scenario: The first start draws the keys and a restart keeps them
    When the development container starts, stops and starts again over one data directory
    Then the config service's two keys and the role secret stand in its data directory at mode 0600, each at least 32 bytes and differing, and the restart kept them and reached readiness

  Scenario: A draw that cannot finish leaves nothing a later start takes for the secret
    When the supervisor cannot finish writing the admin key it draws
    Then it exits with code 74 with no file at the key's path, and the next start draws it and reaches readiness

  Scenario: A file the supervisor cannot read stops its start
    When the supervisor starts with `--config` naming a file that does not exist
    Then it exits with code 78 and an error naming the file

  Scenario: The supervisor refuses what it does not take
    When the supervisor starts with no arguments
    Then it exits with code 78 and an error saying it takes `--config <file>`

  Scenario: A config service over TLS is refused by name
    When the supervisor starts on a config service's settings asking for TLS
    Then it exits with code 78 and an error naming `config.plaintext` and saying to set it

  Scenario: A broken declared file ends the start with every defect
    When the supervisor starts on a central file declaring a storage file with no `data_dir`
    Then it exits with code 78, the error naming `storage.data_dir`, and started no role

  Scenario: A central file of two targets is refused by name
    When the supervisor starts on a central file declaring two targets
    Then it exits with code 78, the error naming both targets and saying it runs one, and started no role

  Scenario: Targets the store keeps are refused with the way out
    When the store holds two active targets and the central file has gone back to one
    Then the refusal names the store and retiring the target it says reaches readiness

  Scenario: A store holding no active target is refused with the way out
    When the store's one target was retired through the API
    Then it exits with code 78, the error naming the target and how to activate it, and started no role

  Scenario: A socket directory that is not there is refused with the way out
    When the supervisor starts on a profile whose socket directory does not exist
    Then it exits with code 78, the error naming the directory and saying what it must be

  Scenario: Each binary's help names the exit codes
    When each of the four binaries is asked for its help
    Then each exits 0 and lists the codes 0, 69, 70, 74 and 78, and the supervisor's the code of a role a signal killed
