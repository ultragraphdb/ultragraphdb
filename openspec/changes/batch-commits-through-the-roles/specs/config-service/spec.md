## MODIFIED Requirements

### Requirement: One central file declares every settings file
The config service's `targets_file` setting SHALL name a required TOML file, the central file, holding one table per target,
each key a role or instance and each value a settings file relative to the central file's directory. At every start, before it
serves anything, the service SHALL read the central file and every file it declares and validate each target whole, and SHALL
refuse to start with code 78 when the central file is missing or unreadable, declares no target, declares a target with no file, or
declares a file that is not there, or fails a check of the requirement above. On a directory holding no revision of a target, the service
SHALL store the target's files as revision 1 and activate it. On a target holding a revision it SHALL leave the stored revisions as they
are, logging the target's name and active revision when the declared files differ from them, since from then on the API is the one
authority, and an edit made through it is saved by downloading it. The profiles `dev`, every role on one host over Unix sockets,
`dev-net`, the same over TCP, SHALL ship as central files with their settings, and the profile `kube`, each role a pod of the Helm
chart `deploy/helm/ultragraphdb` at its headless Service's name, SHALL be that chart's, checked as they are.

#### Scenario: No central file stops the start
- **WHEN** the service starts with `targets_file` naming a file that does not exist
- **THEN** it exits with code 78 and an error naming the setting and the path it looked at

#### Scenario: A central file declaring nothing stops the start
- **WHEN** the central file holds no target, or a target with no file
- **THEN** it exits with code 78, the error naming the central file and saying what it must declare, with the target when one is empty

#### Scenario: A declared file that is absent stops the start
- **WHEN** target `ci` declares `storage_1 = "ci_storage_1.toml"` and no such file exists
- **THEN** it exits with code 78, the error naming the target, the instance, the value as written and the path it resolved to

#### Scenario: Every absent file is named at once
- **WHEN** two declared files are absent and a third is malformed
- **THEN** the one error lists all three, so one run tells the administrator everything to fix

#### Scenario: A central file begins a target once
- **WHEN** a service starts declaring target `ci`, and again after a declared file changed validly
- **THEN** `ci` holds revision 1 alone, and the second start logs that the file differs and the revision still active
