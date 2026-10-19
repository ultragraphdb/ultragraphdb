## ADDED Requirements

### Requirement: Every spec scenario is an executable feature scenario
Each `#### Scenario:` in a spec under `openspec/specs/` or an active change SHALL have a Gherkin scenario of the
same name in a `.feature` file of the crate that implements it, run by `cucumber` from that crate's
`tests/bdd.rs`.

#### Scenario: A spec scenario without a feature fails the build
- **WHEN** a spec declares a scenario that no `.feature` file in the workspace names
- **THEN** the traceability test fails and names the scenario and its spec file

#### Scenario: A feature scenario without a spec is reported
- **WHEN** a `.feature` file names a scenario that no spec declares
- **THEN** the traceability test fails and names the feature file and scenario

### Requirement: Feature scenarios run under Cargo and Bazel with equal counts
A crate's feature scenarios SHALL run under `cargo test` and under a Bazel `rust_test` tagged `bdd-test`, and
both SHALL report the same number of scenarios executed.

#### Scenario: Cargo and Bazel run the same scenarios
- **WHEN** a crate's BDD suite runs under `cargo test -p <crate> --test bdd` and under `bazel test` with
  `--test_tag_filters=bdd-test`
- **THEN** both report the same count of passed scenarios and zero skipped scenarios

#### Scenario: make test runs the BDD suites
- **WHEN** `make test` runs
- **THEN** it runs every target tagged `bdd-test` after the doc tests and unit tests

### Requirement: Scenarios and ported tests precede the code they test
A ported Go file's tests SHALL be ported, and the scenarios for its behaviour written, before the file itself is
ported; each SHALL be observed failing before it passes.

#### Scenario: A task group records the failing run
- **WHEN** a task group ports a Go file
- **THEN** its first task adds the scenarios and ported tests, and its tasks record that they failed before the
  code task made them pass

### Requirement: Byte-exact behaviour is checked against golden vectors from the Go source
Where an output must equal Dgraph's byte for byte, a Go program built against the Dgraph source SHALL generate
the expected outputs into `testdata/golden/`, and the Rust test SHALL compare against them.

#### Scenario: Golden vectors regenerate deterministically
- **WHEN** the golden generator runs twice against the same Dgraph commit
- **THEN** it writes identical files, and each file records the Dgraph commit it came from

#### Scenario: A golden mismatch names the input
- **WHEN** a Rust output differs from its golden vector
- **THEN** the test fails and reports the input, the expected bytes and the actual bytes in hexadecimal
