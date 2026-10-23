## ADDED Requirements

### Requirement: The port logs what Dgraph logs
Library crates SHALL emit a structured `tracing` event, at Dgraph's level, wherever the Go code they port logs, and
wherever the port survives a failure an operator needs to see, with the facts Dgraph's message carries as fields; a
library crate SHALL never install a subscriber.

#### Scenario: A schema key that does not parse is logged
- **WHEN** the schema is loaded from a store holding a schema key that does not parse
- **THEN** the load skips the key and emits an error event `Error while parsing key` with the key as a field

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
