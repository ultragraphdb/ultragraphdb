## ADDED Requirements

### Requirement: Spec scenarios are traced to feature scenarios
The traceability test SHALL match every `#### Scenario:` of the specs under `openspec/specs/` and
`openspec/changes/*/specs/` with the Gherkin scenarios of the same name under `**/features/`, and SHALL fail on an
archived spec's scenario without a feature and on a feature scenario no spec declares. A scenario of a change not
yet archived and without a feature SHALL be reported as pending.

#### Scenario: An archived scenario without a feature fails
- **WHEN** a spec under `openspec/specs/` declares a scenario that no `.feature` file names
- **THEN** the traceability test fails and names the scenario and its spec file

#### Scenario: A feature scenario without a spec fails
- **WHEN** a `.feature` file names a scenario that no spec declares
- **THEN** the traceability test fails and names the feature file and the scenario

#### Scenario: A change's scenarios without features are pending
- **WHEN** a change under `openspec/changes/` declares scenarios that no `.feature` file names yet
- **THEN** the traceability test passes and lists them as pending, by change

#### Scenario: Scenario names are unique
- **WHEN** two specs declare scenarios with the same name
- **THEN** the traceability test fails and names both spec files

### Requirement: Feature suites are declared for both build systems
Every crate holding a `features/` folder SHALL declare its feature suite as `[[test]] name = "bdd"` with
`harness = false` in `Cargo.toml` and as a `rust_test` with `use_libtest_harness = False`, the feature files as
`data`, tagged `bdd-test`, in its `BUILD.bazel`; `scripts/test.sh` SHALL run the `bdd-test` tag after the unit tests.

#### Scenario: A crate with features declares both suites
- **WHEN** the declaration test reads each crate holding a `features/` folder
- **THEN** each declares the Cargo test and the Bazel target as required, and the test names any that does not

#### Scenario: The test script runs the BDD suites
- **WHEN** the declaration test reads `scripts/test.sh`
- **THEN** it finds a Bazel test run filtered on `bdd-test` after the run filtered on `unit-test`

### Requirement: Byte-exact behaviour is checked against golden vectors from the Go source
Where an output must equal Dgraph's byte for byte, a Go program built against the Dgraph source SHALL generate the
expected outputs into the crate's `testdata/golden/`, each file recording the Dgraph commit it came from, and the
Rust test SHALL compare against them.

#### Scenario: Every golden file names its Dgraph commit
- **WHEN** the golden files of the workspace are read
- **THEN** each records the Dgraph commit it was generated from

#### Scenario: A golden mismatch names the input
- **WHEN** a Rust output differs from its golden vector
- **THEN** the comparison fails and reports the input, the expected bytes and the actual bytes in hexadecimal
