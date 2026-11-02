Follows `openspec/protocols/tdd-protocol.md`'s five phases in order, each recorded in `notes/phases.md`, under the
workspace's one lint table.

## 1. The crate

- [x] 1.1 Workspace member `ultragraphdb_transport/*`, the crate's manifest, its Bazel targets
- [x] 1.2 Phase 1: the public surface with `unimplemented!()` bodies; `TestPki`, the suite's harness, implemented
- [x] 1.3 Phase 2: the scenarios as Gherkin in `features/transport_tls/`, unit tests for the corner cases enumerated in
      `notes/phases.md`, every error variant constructed through the public API; the failing run recorded
- [x] 1.4 Phase 3: the suite audited against defects planted in a throwaway implementation
- [x] 1.5 Phase 4: the implementation, judged by the audited suite
- [x] 1.6 Phase 5: `scripts/mutants.sh ultragraphdb_transport_tls`, every survivor killed or recorded

## 2. Telemetry over HTTPS

- [x] 2.1 The OTLP/HTTP client sends to `https://` endpoints through `TlsConnector`, trusting `TlsTrust::system()`
- [x] 2.2 `AGENTS.md`'s crate and dependency tables, the telemetry spec
