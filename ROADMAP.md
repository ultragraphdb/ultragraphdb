# Roadmap: UltraGraphDB

| Stage                                                                                                                | Status                                                     |
|----------------------------------------------------------------------------------------------------------------------|------------------------------------------------------------|
| 1. Foundations and shared types                                                                                      | ✅                                                          |
| 1.5. Fault simulator                                                                                                 | ✅                                                          |
| 2. Storage core                                                                                                      | ✅                                                          |
| 3. Website and public repository                                                                                     | ✅                                                          |
| 4. Data model and quad storage                                                                                       | ✅                                                          |
| 5. Transactions, WAL and the zero-copy binary protocol                                                               | ✅                                                          |
| 5.5. Refactor to prepare for the three-role cluster (frontend, compute, storage)                                     | ✅                                                          |
| 5.6 Replace remaining Go ports with rust native equivalent sematic                                                                        | `open`                                                          |
| 5.7 Benchmark three-role cluster architecture                                                                        | ✅                                                          |
| 5.8 Merge system and storage group into storage service                                                              | `open`: `openspec/changes/stage-5-8-system-group`          |
| 5.9 Merge frontend and compute node into one service, named compute                                                  | `open`: `openspec/changes/stage-5-9-compute-role`          |
| 6.5 The tile store: a graph-native storage engine replacing the KV store                                             | `open`: `openspec/changes/stage-6-5-tile-store`            |
| 6.6 UGS: the full data schema, DDL, and its implementation                                                           | `paused`: `openspec/changes/stage-6-6-ugs-data-definition` |
| 6.7 Harden transactions with chaos testing against multiple UGS schemata                                             | `open`                                                     |
| 7. Query core with RBAC: SPARQL and GQL where they overlap; Add RBAC early       | `open`                                                     |
| 7.5. Fuzzy test and harden query core                                                                                | `open`                                                     |
| 8. Basic clustering of compute and storage with VSR replication in a single container, the base of the DeepBrain MVP | `open`                                                     |
| 9. Full SPARQL and GQL conformance                                                                                   | `open`                                                     |
| 9.5. Schema evolution with checked data migration                                                                    | `open`                                                     |
| 10. Full distributed clustering: sharding and replication at scale!                                                  | `open`                                                     |
| 10.5. Bolt                                                                                                           | `open`                                                     |
| 11. API node: generated Protobuf, JSON Schema and GraphQL schemas served by a stateless binary                       | `open`                                                     |

The stages run in the order of the table. Stages 5.8, 5.9 and 6.5 each change what the one after builds on, so none
starts before the one above it is complete.

No stage is numbered 6. UGS was stage 6 until 2026-11-12 and is stage 6.6. The architecture these stages build is
`openspec/designs/UltraGraphDB Distribution Architecture.md`.


Next
- Replace remaining go semantics with Rust natives
- Add loom tests to test identified concurrency code for race conditions 
- verify and implement stage 5.8