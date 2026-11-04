# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The config service and its CLI as processes
  openspec/changes/stage-5-5-three-role-refactor/specs/config-service/spec.md

  Scenario: A second process over the directory is refused
    When a config service runs and another starts over the same data directory
    Then the second exits with code 69 and an error naming the directory

  Scenario: The image holds its binary alone
    When the config service's dependency graph is read
    Then it names neither `ultragraphdb_store_kv` nor fjall

  Scenario: A broken declared file stops the start
    When the config service starts on a fresh directory with a declared file failing a check
    Then it exits with code 78, lists every defect, stores nothing and serves nothing

  Scenario: No central file stops the start
    When the service starts with `targets_file` naming a file that does not exist
    Then it exits with code 78 and an error naming the setting and the path it looked at

  Scenario: A central file declaring nothing stops the start
    When the central file holds no target
    Then it exits with code 78, the error naming the central file and saying what it must declare

  Scenario: A declared file that is absent stops the start
    When target `ci` declares `storage_1 = "ci_storage_1.toml"` and no such file exists
    Then it exits with code 78, the error naming the target, the instance, the value as written and the path it resolved to

  Scenario: Every absent file is named at once
    When two declared files are absent and a third is malformed
    Then the one error lists all three, and it exits with code 78

  Scenario: A central file begins a target once
    When a service starts declaring target `ci`, and again after a declared file changed validly
    Then `ci` holds revision 1 alone, and the second start logs that the file differs

  Scenario: A restart holds what was acknowledged
    When a revision is acknowledged and activated, and the service is killed and restarted
    Then the revision is stored, and the active revision is the one last acknowledged

  Scenario: A rollback activates an older revision
    When target `ci` holds revisions 1 and 2, with 2 active, and an administrator activates 1
    Then an instance's read names revision 1 in `X-Revision`, and revision 2 is still stored

  Scenario: A revision edited from a stale one is refused
    When two administrators edit from revision 1, and the first adds revision 2
    Then the second's revision is refused with status 409 naming revision 2, and nothing is stored

  Scenario: A broken file refuses the revision
    When an administrator adds a revision whose `storage_1` lacks `data_dir` and whose `compute_1` names `partiton`
    Then it is refused with status 422 listing both defects, each naming its instance and setting, and nothing is stored

  Scenario: A request without a key is refused
    When a request reaches a target's instance with no bearer key
    Then it is refused with status 401, and its body names no setting

  Scenario: The read key changes nothing
    When a request adds a revision with the read key
    Then it is refused with status 403, and nothing is stored

  Scenario: An unknown instance is told so
    When a request names an instance the active revision does not hold
    Then it is refused with status 404 naming the target, the revision and the instance

  Scenario: A retired target serves nothing
    When a target is retired and a role of it starts
    Then the request is refused with status 404 saying the target is retired, and the target's revisions are still stored

  Scenario: A key too short stops the start
    When the service starts with an admin key of 8 bytes
    Then it exits with code 78 naming the key's file and the 32 bytes it requires

  Scenario: A target with no active revision says so
    When a role asks for an instance of a target whose only revision has not been activated
    Then it is refused with status 404 saying the target has no active revision

  Scenario: An oversized request is refused
    When a request's declared body is larger than the bound
    Then it is refused with status 413 before its body is read, and nothing is stored

  Scenario: The health answers need no key
    When a request asks for `/health` and `/ready` with no key
    Then both answer 200

  Scenario: The check finds what the service would refuse
    When an administrator runs `ultragraphdb_config_cli` over settings the service would refuse
    Then it prints the same defects the service would, and exits with code 78

  Scenario: The check passes what the service would take
    When an administrator runs it over valid settings
    Then it lists each target with its instances, says it did not compare against stored revisions, and exits with code 0

  Scenario: A stray file is pointed out
    When the directory holds `ci_storge_1.toml`, which no target declares
    Then the check warns naming the file, and its exit code is unchanged
