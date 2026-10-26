# port-testing Specification

## Purpose
How the port proves it equals Dgraph: every spec scenario runs as a Gherkin scenario of the same name under
`cucumber`, a ported Go file's tests are ported first, byte-exact outputs are compared against golden vectors generated
from the Go source, and Cargo and Bazel run the same suites with equal counts.
## Requirements
### Requirement: Byte-exact behaviour is checked against golden vectors from the Go source
Where an output must still equal Dgraph's byte for byte, a Go program built against the Dgraph source SHALL generate the
expected outputs into the crate's `testdata/golden/`, each file recording the Dgraph commit it came from, and the Rust
test SHALL compare against them. That holds for the UID-list codec and its set operations, the tokenizers, the scalar
types the tokenizers take, the posting codec and the HNSW kernels and layout; keys, the term encoding and the quad
layout are the port's own and SHALL be checked against their specifications instead.

#### Scenario: A golden mismatch names the input
- **WHEN** a Rust output differs from its golden vector
- **THEN** the comparison fails and reports the input, the expected bytes and the actual bytes in hexadecimal

#### Scenario: Keys carry no golden vector
- **WHEN** the key tests run
- **THEN** they check the quad layout against its specification and read no golden file generated from Dgraph

