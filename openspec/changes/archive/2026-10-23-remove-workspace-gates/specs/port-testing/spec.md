## REMOVED Requirements

### Requirement: Spec scenarios are traced to feature scenarios
**Reason**: It read the specs and every crate's features from inside `make test`, through an unsandboxed, uncached
Bazel target.
**Migration**: None; a feature scenario keeps its spec scenario's name by convention.

### Requirement: Feature suites are declared for both build systems
**Reason**: It read every crate's manifests and the test script from inside `make test`, through an unsandboxed,
uncached Bazel target.
**Migration**: `scripts/bdd_counts.sh` compares the scenario counts Cargo and Bazel report for each crate.

## MODIFIED Requirements

### Requirement: Byte-exact behaviour is checked against golden vectors from the Go source
Where an output must equal Dgraph's byte for byte, a Go program built against the Dgraph source SHALL generate the
expected outputs into the crate's `testdata/golden/`, each file recording the Dgraph commit it came from, and the
Rust test SHALL compare against them.

#### Scenario: A golden mismatch names the input
- **WHEN** a Rust output differs from its golden vector
- **THEN** the comparison fails and reports the input, the expected bytes and the actual bytes in hexadecimal
