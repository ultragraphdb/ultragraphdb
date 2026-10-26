# telemetry Specification

## Purpose
Logging and tracing: structured events where Dgraph logs, emitted through `tracing` by every library crate, and
the subscriber a binary installs once, with its level filter, output format and optional OTLP span export.
## Requirements
### Requirement: The port logs what Dgraph logs
Library crates SHALL emit a structured `tracing` event, at Dgraph's level, wherever the Go code they port logs, and
wherever the port survives a failure an operator needs to see, with the facts Dgraph's message carries as fields; a
library crate SHALL never install a subscriber. A failure a caller receives as an error is not logged as well.

#### Scenario: An index build that fails is logged
- **WHEN** an index build running in the background fails
- **THEN** the applier emits an error event `error in building indexes, aborting` with the build's record as a field

### Requirement: A binary installs logging and span export once
`ultragraphdb_common_telemetry` SHALL install the global subscriber from a configuration the caller passes: a level
directive, plain or JSON lines on standard error, and, when an OTLP endpoint is given, spans exported over OTLP; the
guard it returns SHALL flush the exporter when dropped.

#### Scenario: JSON lines carry the event's fields
- **WHEN** telemetry is installed for JSON output at level `info` and an `info` event with a field `key` is emitted
- **THEN** one JSON line holds the level, the target, the message and `key`, and a `debug` event emits nothing

#### Scenario: Spans reach the exporter with their parent
- **WHEN** telemetry is installed with an exporter and a span is entered inside another
- **THEN** the exporter receives both spans, the inner one naming the outer as its parent

