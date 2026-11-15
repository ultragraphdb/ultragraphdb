# Roadmap: UltraGraphDB

| Stage                                                                                  | Status        | Specs                                                                                                                                        |
|----------------------------------------------------------------------------------------|---------------|----------------------------------------------------------------------------------------------------------------------------------------------|
| 1. Foundations, shared types and fault simulator                                                         | ✅             | `openspec/changes/archive/2026-10-23-port-stage-1-foundations`                                                                               |
| 2. Storage core                                                                        | ✅             | `openspec/changes/archive/2026-10-25-port-stage-2-storage`                                                                                   |
| 3. Website and public repository                                                       | ✅             |                                                                                                                                              |
| 4. Data model and quad storage                                                         | ✅             | `openspec/changes/archive/2026-10-26-stage-4-data-model-and-quad-storage`                                                                    |
| 5. Transactions, WAL and the zero-copy binary protocol                                 | ✅             | `openspec/changes/archive/2026-11-01-stage-5-transactions-and-binary-protocol`<br>`openspec/changes/archive/2026-11-02-transport-tls`        |
| 5.5. Refactor to prepare for the three-role cluster (frontend, compute, storage)       | ✅             | `openspec/changes/archive/2026-11-09-stage-5-5-three-role-refactor`<br>`openspec/changes/archive/2026-11-09-batch-commits-through-the-roles` |
| 5.7 Benchmark three-role cluster architecture                                          | ✅             | `openspec/changes/stage-5-8-system-group`                                                                                                                                              |
| 5.8 Merge system and storage group into storage service                                | `progressing` | `openspec/changes/stage-5-8-system-group`                                                                                                    |
| 5.9 Merge frontend and compute node into one service, named compute                    | `open`        | `openspec/changes/stage-5-9-compute-role`                                                                                                    |
| 6.5 The tile store: a graph-native storage engine replacing the KV store               | `open`        | `openspec/changes/stage-6-5-tile-store`                                                                                                      |
| 6.6 UGS: the full data schema, DDL, and its implementation                             | `paused`      | `openspec/changes/stage-6-6-ugs-data-definition`                                                                                             |
| 6.7 Harden transactions with chaos testing against multiple UGS schemata               | `open`        |                                                                                                                                              |
| 7. Query core with RBAC: SPARQL and GQL where they overlap; Add RBAC early             | `open`        |                                                                                                                                              |
| 7.5. Fuzzy test and harden query core                                                  | `open`        |                                                                                                                                              |
| 8. Basic clustering with VSR replication in a single container for the DeepBrain MVP   | `open`        |                                                                                                                                              |
| 9. Full SPARQL and GQL conformance                                                     | `open`        |                                                                                                                                              |
| 9.5. Schema evolution with checked data migration                                      | `open`        |                                                                                                                                              |
| 10. Full distributed clustering: sharding and replication at scale!                    | `open`        |                                                                                                                                              |
| 10.5. Bolt                                                                             | `open`        |                                                                                                                                              |
| 11. API node: generated Protobuf, JSON Schema and GraphQL served by a stateless binary | `open`        |                                                                                                                                              |

The stages run in the order of the table. Stages 5.8, 5.9 and 6.5 each change what the one after builds on, so none
starts before the one above it is complete. UGS was stage 6 until 2026-11-12 and is now stage 6.6.

The architecture these stages build is
`openspec/designs/UltraGraphDB Distribution Architecture.md`.

Next

- verify and implement stage 5.8
- verify and implement stage 5.9