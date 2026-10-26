## MODIFIED Requirements

### Requirement: The port logs what Dgraph logs
Library crates SHALL emit a structured `tracing` event, at Dgraph's level, wherever the Go code they port logs, and
wherever the port survives a failure an operator needs to see, with the facts Dgraph's message carries as fields; a
library crate SHALL never install a subscriber. A failure a caller receives as an error is not logged as well.

#### Scenario: An index build that fails is logged
- **WHEN** an index build running in the background fails
- **THEN** the applier emits an error event `error in building indexes, aborting` with the build's record as a field
