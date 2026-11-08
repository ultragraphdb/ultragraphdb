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

  Scenario: A refusal reaches a client still sending
    When a client goes on sending a body after the refusal of its declared size has arrived
    Then each of its writes is taken, it reads status 413 and then the end of the connection, and nothing is stored

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

  Scenario: A bad config-service file stops its start
    When the config service starts with `--config` naming a file that does not exist
    Then it exits with code 78 and an error naming the file

  Scenario: The profiles ship and pass
    When an administrator checks the `dev` and `dev-net` profiles
    Then each passes, listing its frontend, storage and compute

  Scenario: An edit made through the API is downloaded and saved
    When an administrator activates a revision through the API and downloads target `ci`
    Then the answer holds that revision's files as received, names the revision and a file to save them as, and taken back it adds a revision

  Scenario: A download needs the admin key
    When a request downloads a target with the read key
    Then it is refused with status 403

  Scenario: A target with no active revision has nothing to download
    When an administrator downloads a target whose only revision has not been activated
    Then it is refused with status 404 saying the target has no active revision

  Scenario: A restore rebuilds a folder a service can begin from
    When an administrator adds a revision of target `ci` through the API, activates none, and restores the service into a folder
    Then the folder holds the latest revision's files alone, says which is active, and a fresh service over it serves those settings

  Scenario: A restore leaves out a retired target
    When a service holds a retired target and one with no active revision beside `ci`
    Then the restore holds `ci` and the one with no active revision, and says it left out the retired one

  Scenario: A restore with the read key writes nothing
    When a restore is run with the read key
    Then it exits with code 78 saying the token was refused, prints no token, and no folder appears

  Scenario: A restore adds to no folder
    When a restore names a folder holding a file
    Then it exits with code 74 saying so, and the folder is as it was

  Scenario: A restore of a service that is down says so
    When a restore names a URL where nothing answers
    Then it exits with code 69 naming the URL

  Scenario: A restore of a service holding retired targets alone writes nothing
    When a service's one target is retired and a restore is run
    Then it exits with code 78 saying the service holds no target but retired ones, and no folder appears

  Scenario: A restore refuses a listing line that names no target
    When a restore reads a listing holding a line that names no target
    Then it exits with code 78 naming the URL and the line, and no folder appears

  Scenario: A restore refuses a listing naming a revision that is no number
    When a restore reads a listing naming revision `two` of target `ci`
    Then it exits with code 78 naming `two` as the revision, and no folder appears

  Scenario: A restore refuses a status the service answers with no success
    When a restore's listing is answered with status 500
    Then it exits with code 78 naming the URL, the status and what the server said, and no folder appears

  Scenario: A restore refuses settings this release refuses
    When a restore reads a revision of target `ci` whose `storage` lacks `data_dir`
    Then it exits with code 78 naming the central file it wrote and `storage.data_dir`

  Scenario: A restore over TLS reads the trust file it names
    When a restore names an `https` URL and `--ca-file` naming no file
    Then it exits with code 78 naming the trust file, and no folder appears

  Scenario: A restore over TLS reads the system's trust file
    When a restore names an `https` URL with `SSL_CERT_FILE` naming no file
    Then it exits with code 78 naming the trust file, and no folder appears

  Scenario: The CLI states its usage
    When an administrator runs `ultragraphdb_config_cli` with no argument
    Then it prints both forms of its command line and exits with code 64

  Scenario: The config service ends with the process it names
    When a config service starts with `ULTRAGRAPHDB_PARENT_PID` naming a process that is not its parent
    Then it stops as on SIGTERM and exits with code 0

  Scenario: The config service takes a port held a moment
    When a config service starts at an address another process holds, which lets it go within the bound
    Then it logs each retry naming the address, binds it, and answers `/health`
