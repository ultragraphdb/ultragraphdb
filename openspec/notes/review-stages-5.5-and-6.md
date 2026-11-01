# Review: preparing stages 5.5 and 6

Nov 1, 2026. A critical reading, after Paul and Elder's *Miniature Guide to Critical Thinking*, of four documents:
`ROADMAP.md`, `UltraGraphDB Distribution Architecture.md`, `UltraGraphDB Single-Container Deployment.md` and
`UltraGraphDB Schema and Query Language.md` (UGS Part 1 in full, UQL's transactions and consistency sections), checked
against the specs stage 5 archived and the code. Its purpose is to prepare stage 5.5 (refactor for the clustering
architecture) and stage 6 (UGS: full data definition and schema implementation). It reports findings; §7 records
what was resolved and the decisions left for the user.

## 1. Purpose and question at issue

| Stage | Purpose, as the documents state it | Question at issue |
|---|---|---|
| 5.5 | Prepare the stage-5 code for frontend, compute and storage roles replicated by VSR | Which seams must exist before UGS and clustering, so neither forces the other to be rewritten? |
| 6 | Implement UGS: one schema that defines the graph for GQL, SPARQL and UQL and declares every invariant | Which part of UGS version 1 is built in stage 6, and is the note precise enough to build from? |

Answer in brief: the stages share one decision neither document makes, the layout of a system ID (§4.1), and it
belongs first. UGS needs a specification of its own before stage 6 (§6): the note is a language design with its
rationale, not a specification a suite can be written against.

## 2. Elements of thought

**Point of view.** All three architecture notes are written from DeepBrain's needs: multi-domain knowledge, a shared
ontology, agents as clients, counterfactual reasoning downstream. UGS says so ("purpose-built, not general-purpose";
"workloads that do not depend on a shared ontology are not design targets"). That is a legitimate, stated frame, and
the review judges the documents inside it.

**Assumptions** (stated and unstated):

1. A typed, closed-world schema suits data that arrives as open-world RDF; `@open` is the escape hatch.
2. Invariant-confluence analysis can be automated for the whole constraint set: keys, cardinalities, acyclicity, escrow
   bounds and delete semantics. The cited work (Bailis 2015, Whittaker and Hellerstein 2019, Hamsaz 2019) automates
   parts of this. Automating all of it in one compiler is the project's own contribution, not an off-the-shelf result.
3. The cluster is large enough for coordination avoidance to pay. The repository's working assumption is 3 to 8
   gateway replicas and 8 concurrent writers. At that scale the machinery's worth comes from maturing early, the
   reason the user gave for building distributed from the start, more than from throughput.
4. Epochs a few milliseconds long are cheap enough to close and broadcast. The adaptive bounds are an open question in
   the distribution note.
5. A 64-bit ID can carry a type prefix, a replica group and a sequence number with room for all three (§4.1).
6. Schema evolution and modules leave the contiguous type-ID ranges intact (§4.4).

**Concepts** the documents rely on, each of which needs one definition before code:

- the system ID and the opaque handle;
- epoch versus commit version;
- placement and colocation;
- segmented, coordinated and coordination-free paths;
- monotonic-fast versus snapshot reads;
- live, retracted and open instances;
- precision and extents of an interval;
- the meaning of a role with no cardinality marker.

**Information.** The literature cited for coordination avoidance, RAMP-TAO, TigerBeetle and Docker's io_uring seccomp
change is used fairly as far as checked from memory: the Bailis TPC-C figure (12.7 million New-Order transactions per
second on 200 servers, 25 times prior results) and moby #46762. No page references are given, and this review did not
verify the RAMP-TAO and SIGMOD 2024 figures.

**Inferences and implications** follow in §3 to §6.

## 3. Intellectual standards: findings

### Clarity

1. **"3 node clustering" (ROADMAP stages 5.5 and 8) has two readings.** It can mean three node *types* (frontend,
   compute, storage), or three *replicas* in a VSR group. The single-container topology runs one metadata replica, one
   sequencer, two storage groups of one replica and one compute instance: three roles, five processes' worth of state,
   no group of three. Stage 8's exit criterion depends on which is meant.
2. **Valid time is declared two ways:** `@valid(attr)` and `@bitemporal(valid: attr)`. The note does not say whether
   `@bitemporal` implies `@valid`, or whether both may appear on one type.
3. **A role without a cardinality marker:** the grammar makes `card` optional (`ident card`, `card = [ ... ]`), but no
   text gives the default (exactly one, or any number).

### Accuracy (checked against the repository)

1. **ROADMAP's prose predates its own table.** The table renumbers the stages (6 UGS, 7 query core, 8 basic clustering,
   9 conformance, 10 full distribution), but the text below still says:
   - "The DeepBrain MVP runs on the single node of stage 7";
   - "Stage 6 builds only what SPARQL and GQL share";
   - "follow in stage 8; Bolt follows in stage 9";
   - "Fuzzing in stage 6 centres on the query core".

   Bolt has no row any more. "Improvements, single node" item 3 still names "n−1 worker cores and one commit step".
   "Improvements, clustering" places VSR, Jepsen and the storage/compute separation in stage 10, while stage 8 now
   clusters. "clusgtering" is misspelt.
2. **The plan says otherwise.** `UltraGraphDB Dual Query Engine Plan.md`, which ROADMAP names as the plan, still
   carries the single-server stage table (rows 5.5 to 10) and its Threads section.
3. **musl or glibc.** The distribution note ships "the same static musl binary" on every node. A decision of
   2026-10-25 (repository memory: "ultragraphdb ships against glibc; musl/mimalloc tuning declined") says glibc.
4. **TLA+ or Lean.** The distribution note's verification strategy calls for "a TLA+ specification of the epoch,
   change-stream, watermark and delete protocol". Stage 5 built its formal model in Lean 4 (`lean/RoundModel`), and
   ROADMAP names no tool.
5. **UGS against the code and the archived specs:**

   | UGS says | The repository has | Consequence |
   |---|---|---|
   | "Closed by default … data that does not match the schema is rejected" | `schema-storage`: "a write to a predicate the catalog does not declare SHALL record a default entry for it"; stage 5 made that an insert-if-absent | The implicit schema is removed or confined to `@open` |
   | "System IDs never appear in IRIs"; keyless entities get opaque handles | `core_mapping::Names`: `node_iri = base + "node/" + uid`, `skolem_iri = base + ".well-known/genid/" + uid` (PROTOCOL.md §4.4, §10.2) | Every minted IRI exposes a UID today |
   | System ID = type prefix + replica group + sequence | `term-dictionary`: 64-bit UID, top 16 bits a type prefix, zero in this stage; no group bits | The UID layout changes (§4.1) |
   | `Vector<BF16, n>`, int8 quantization with BF16 rescoring | `vector-index`: vectors stored as `ugdb:float32vector`, HNSW over f32 | A storage format and an index variant to add |
   | Transactions strictly serializable (archived `transactions` spec) | Cross-group reads at the latest closed epoch; session tokens for read-your-writes | Strictness holds within a group, not across groups (§4.3) |

### Precision

1. **The ID layout has no bit widths** (§4.1).
2. **The opaque handle is "a keyed encryption of the system ID, stable for the entity's lifetime".** The note leaves
   open which key, held where, per namespace or per cluster, and what happens when the key leaks. A key that is never
   rotated is what keeps a handle stable, so rotation changes every keyless IRI.
3. **`today()` and `now()` "return the time of the snapshot epoch", and `as of time t` reads "the last epoch closed
   before t".** Both require the sequencer to record a wall time per epoch. The distribution note calls the epoch
   protocol "clock-free", so where that time comes from, and how a skewed clock is handled, is unstated.
4. **Imprecise closed-open intervals.** The note defines outer and inner extents, but not how a bound stated to a
   year combines with a closed-open end: is `.. 2027` exclusive of all of 2027, or of its end?
5. **Lexical structure.** The reserved words omit words the grammar uses as keywords: `statement`, `module`,
   `extends`, `quantity`, `unit`, `Optional`, `exists`, `order`, `by`. The grammar lets annotations stand both before
   and after a relation body, with no rule for which go where.
6. **The type table merges two mappings in one cell:** for `List<T>` and `Set<T>` the RDF view reads "`rdf:JSON`
   literal; one triple per member", without saying which belongs to which. `UInt128` is missing beside `Int128`.

### Relevance

The UQL part of the schema note is relevant to stages 5.5 and 6 in three places only: its transaction rules
(single-group transactions interactive, multi-group ones submitted whole), its consistency clauses, and its
schema introspection. The rest serves the stage after 10.

### Depth

1. **`@colocate(role)` meets predicate sharding.** The note keeps "placement by predicate, as in Dgraph" for the KV
   stage, while `@colocate(asset)` "partitions a relation by the group of the participant". A Dgraph key is
   (predicate, subject), and a tablet is a whole predicate. Colocating a predicate by its participant splits it across
   groups by the participant's group. That makes the placement function (predicate, group of a UID) → group, which
   works only if a UID carries its group (§4.1). The inverse direction of a colocated relation (`Organization.assets`
   for `Owns @colocate(asset)`) then lives in each asset's group, so reading it scatters across groups. The note does
   not discuss that cost.
2. **Incremental maintenance of folds.** `last`, `count` and `sum` maintain incrementally over append-only occasions.
   `min` and `max` over mutable data need recomputation on delete, which "maintained incrementally" does not cover.
3. **Escrow for `@limit(sum … per buyer)`.** Treaty shares, their renegotiation, and recovery when a group fails while
   holding a share form a protocol of their own (Homeostasis). The note names it in one line.
4. **Rank-based acyclicity checks with `@colocate(root)`.** The rank structure and its maintenance under subtree moves
   are named, not described; `@acyclic(open, max_depth)` adds a materialized subtree height.
5. **Writes through GQL and SPARQL.** "Standard queries stay standard", but a SPARQL Update can write one role triple
   of an n-ary instance at a time, or a binary relation's reifier before its triple. The note says GQL and SPARQL
   writes go through the same classification. It does not say that every instance is validated whole at commit, or
   how triples assemble into instances.

### Breadth

The documents look at the design from the engine's side and DeepBrain's. The operator who runs a cluster depends on it
as much and is not addressed. The view below lists what an operator needs, what the documents already give, and what
is missing; its items are resolved in §7.

#### An operator's view

An operator deploys the roles, keeps them healthy, plans capacity, rolls out schema versions and binaries, and
recovers from failure.

| Need | What the documents give | What is missing |
|---|---|---|
| Deploy the roles | One binary with role flags; profiles from `dev` to one role per process; scratch OCI images for frontend and compute; storage on a signed unified kernel image in a microVM (Firecracker or Cloud Hypervisor) | Who builds, signs and patches the kernel image, and how storage nodes are scheduled where no microVM orchestrator runs. Whether a Kubernetes deployment (operator, StatefulSets, volumes) is a target |
| Know the cluster is healthy | `/health` and `/ready` in the single container; telemetry through OTLP (`ultragraphdb_common_telemetry`) | The signals to watch, per role. Candidates:<br>• epoch close latency and the age of the latest closed epoch<br>• watermark lag per group and per incoming change stream (how stale derived data is)<br>• the share of writes taking the coordinated path<br>• the rate at which escrow shares run out and are renegotiated<br>• VSR view changes per group<br>• repairs made by the background verifier<br>• the sequencer's queue depth |
| Plan capacity | The coordination report of every schema compilation ("3 of 17 write operations need cross-group coordination") | The report counts operations, not load: an operator needs the share of the expected *traffic* that each path takes, and the sequencer's throughput ceiling. Storage growth: `@append_only` and `@bitemporal` instances are never physically discarded, and other types keep an MVCC retention window, so the store grows without bound unless a legal purge runs |
| Roll out a schema version | Versions committed through the metadata group at an epoch boundary; tightening changes run a validation scan; destructive changes deprecate, then drop after finalize | How long a validation scan takes on a large graph and whether writes wait for it. Whether a schema version can be rolled back, and what happens to data written under it. A placement change starts a relocation, but the report shows which writes get cheaper or dearer, not how much data moves, for how long, or how to throttle it |
| Roll out a binary | Storage first, compute second, frontends last; a state-machine version switch through the log; a finalize step after which downgrade is one-way | How long a cluster may run mixed versions, and how the "one version back" negotiation is tested before a rollout |
| Recover from failure | VSR groups survive a minority failure; during a sequencer view change, single-group work continues and multi-group snapshot reads fall back to the latest closed epoch with staleness reported | Runbooks: losing a majority of a group, losing the metadata group, a group that cannot close an epoch. How the background verifier's repairs are reported and when they need a human |
| Back up and restore | "Data moves by backup and restore or by export and import" (single-container note); stage 5 decided the port restores only its own backups | What a cluster backup is: a snapshot of every group at one closed epoch, with the metadata group's catalog at that epoch. How a backup taken under one placement restores under another |
| Manage secrets | Security by tier; TLS off on local carriers | The opaque handle's key: where it lives (a KMS or a file), who may read it, and what rotating it does (§3 Precision 2: every keyless IRI changes). The certificates each tier needs |
| Tune for the hardware | The power-loss-protection flag; io_uring with a polling fallback where Docker's seccomp profile blocks it | Which settings an operator changes and which only a developer changes: epoch length bounds, round sizes, the MVCC retention window, escrow share sizes |

The single-container note is the one document that serves the operator well: it states what is identical to a cluster
and what is not, and keeps the data layout per role and replica the same as on a cluster node.


### Logic

1. **Contiguous type-ID ranges against evolution and modules.** IDs are "type-prefixed in hierarchy order, so a
   supertype is one contiguous ID range"; the catalog numbers types "in a preorder walk of the type hierarchy". Yet
   adding a type is "additive; online; no validation", and a module "may declare types beneath any type it extends".
   Adding a subtype under an existing type in a later version shifts the preorder numbers of every type after it.
   Either prefixes are allocated with reserved gaps (bounding subtypes per type and modules per core), or supertype
   ranges stop being contiguous and become sets of ranges. As written, the two rules cannot both hold.
2. **"Types are fixed for life, because the type is part of the ID".** That is consistent, and it means renumbering
   types is impossible once data exists, which sharpens point 1.
3. **Snapshot reads lag one epoch, and stage 5's strictness claim** (§3 Accuracy 5): both can be true only if
   strictness is scoped to a group.

### Significance

In order of consequence:

1. the system-ID layout (§4.1);
2. the cut of UGS version 1 that stage 6 builds (§6);
3. strictness's scope (§4.3);
4. the type-prefix allocation rule (§3 Logic 1);
5. where the catalog lives, store keyspace or metadata group (§4.5).

Everything else is detail that a specification settles.

### Fairness

The documents' strengths:
- One coherent frame from schema to placement.
- Decisions tables that record what was chosen.
- Standard languages kept first-class.
- The single-container **parity rule**: no role branches for running alone, and frames fully encoded on every carrier.
  This is the strongest idea in the set; it is what keeps the single machine honest.
- Supernode mitigation thought through before tiles exist.
- Diagnostics designed as part of the language.

This review's own frame: it reads for what blocks implementation, so it under-weights what the documents do well.

## 4. Implications for stage 5.5

What the refactor has to settle, as questions. §7 and `UltraGraphDB Shard-per-Core Write Path.md` answer them: 4.1 by
D1 as revised (P7, P15, P16), 4.2 by P1–P3 and P17, 4.3 by P13 and P14, 4.4 by P20, 4.5 by D4.

### 4.1 The system ID

Today a UID is 64 bits: a 16-bit type prefix (zero) and a 48-bit sequence from the term dictionary's leases. UGS wants
type prefix + replica group + sequence, IDs generated per group without coordination, and IDs that never leave the
database. Open:

- What are the widths? For example 16 / 12 / 36 gives 4,096 groups and 68 billion IDs per type and group. Is that
  enough for DeepBrain's largest type?
- Do relation instances (statement IDs) share the space, as UGS's "one type-prefix space" says?
- Does the term dictionary keep allocating IRIs' UIDs, or does an entity's ID come from its type and group, with IRIs
  mapped to it?
- Do posting lists keep sorting by UID, so that a type or a group is one key range?

This decision feeds UGS (type prefixes), placement (group bits) and the opaque handle, so it comes first.

### 4.2 The placement function

What maps a key to a group: by predicate now, by (predicate, the group of a participant's UID) for `@colocate`, by
GSL coordinates with tiles. Stage 5.5 can introduce the function as a seam with one group behind it.

### 4.3 The commit step as a group's state machine

The round gains an epoch. The ordered-log interface meets VSR, and ROADMAP's Aeron questions (op number or byte
position, NAK repair, segment sizes) live here. Open:

- Does a group keep clock-following commit versions inside, with epochs across groups (backlog item 5)?
- Is the archived `transactions` spec's strict serializability restated as per group?

UQL already settles the transaction shape: single-group transactions interactive, multi-group ones submitted whole.
That answers backlog item 5's second question.

### 4.4 Roles in the code

Which stage-5 crates belong to which role:
- **frontend:** the endpoint (backlog item 2), the metadata group, the sequencer;
- **compute:** query execution;
- **storage:** the transactions, the commit step, the log, the store;
- **the pattern engine:** split between compute and the storage group's narrow kernels.

The transport's in-process carrier (single-container note) belongs here; the transport is already sans-I/O.

### 4.5 The catalog's home

Stage 4 keeps the catalog in each namespace's catalog keyspace. The distribution note puts schema versions, the
classification and the mapping contract in the metadata group, committed at epoch boundaries. If stage 6 builds UGS
on the store-side catalog, stage 8 moves it. Should stage 5.5 introduce the metadata group's state machine, with one
replica, as the catalog's home?

### 4.6 Single-server assumptions to remove

- the implicit schema on first write;
- IRIs minted from UIDs;
- "n−1 workers" in the docs;
- ROADMAP's stale prose;
- the plan's stage table.

## 5. ROADMAP corrections

Applied on 2026-11-01 (§7, S4 and S5): stages 5.5 and 8 name the three roles; the prose under the table follows the new
numbering; VSR, the storage/compute separation and Aeron's stream position move to stage 8; the single-node item no
longer names n−1 workers; "clusgtering" is fixed; the plan's stage table points to ROADMAP's. Where the DeepBrain MVP
runs and where Bolt goes are decision D8.

## 6. Stage 6: does UGS need its own specification?

Yes. The note specifies intent well: what each construct is for, how it maps, what it costs. A test suite cannot yet
be written from it. The TDD protocol needs every scenario as Gherkin before code, and the note has none. What a UGS
specification would add:

| Part | What the note has | What the specification needs |
|---|---|---|
| Lexical structure and grammar | An EBNF sketch | A complete grammar, reserved words matching it, every annotation with its argument types and where it may stand |
| Static semantics | A list of validation checks | Every rule a schema must satisfy, each with its error code (`UGS0204` style) and a scenario |
| Identity | Prose | The ID layout (§4.1), type-prefix allocation under evolution and modules (§3 Logic 1), the opaque handle's key and scheme |
| Compilation to the catalog | "One source, five outputs" | How each construct becomes catalog entries, predicates, keys and indexes in the existing store, replacing the implicit schema |
| Runtime enforcement | A constraint table with write paths | When and where each constraint is checked at commit, and its abort reason (the `transactions` spec's error table) |
| Standard views | Two mapping tables | The exact RDF vocabulary, GQL graph type and SHACL shapes generated, checked by round-trip scenarios through the mapping contract |
| Evolution | One table | Compatibility rules per change, the validation scan, deprecation and finalize, type-prefix stability |
| Classification | The algorithm in five steps | The conflict graph's definition and the path of each operation. This needs groups, so it fits stage 8 more than stage 6 |

The cut of version 1 that stage 6 builds is a decision for the user. One possible line, to start from:

| Construct | Stage 6 builds | Parses and validates only | Later |
|---|---|---|---|
| Entities, single inheritance, abstract types, keys, `@unique`, required and optional attributes, value checks, enums, `@values`, records, lists, sets | ✓ | | |
| Relations, roles, navigation names, cardinality, `@set` and `@multi`, `@symmetric`, relation inheritance and `as` | ✓ | | |
| Delete semantics, `@append_only`, `@immutable` | ✓ | | |
| `@acyclic` within one group | ✓ | | rank checks across groups (stage 8) |
| Statements, union roles, catalog types as participants | ✓ | | |
| Namespaces, IRIs, `@maps_to`, the RDF view | ✓ (with §4.1 and the opaque handle) | | |
| Indexes (range, fulltext, ann) | range and the existing vector index | fulltext, quantized ann | |
| Graphs, `@shared`, `@anchor`, `@replicated` | single-group meaning | | `@replicated` copies (stage 8) |
| `derive` closures and folds, `fn` | | ✓ | materialization through change streams |
| Time types, `@valid`, `@no_overlap`, imprecise intervals | types and `@no_overlap` | `possibly` and `certainly` | |
| `@bitemporal`, `@retracts`, `@on_retract` | | ✓ | needs epoch time (§3 Precision 3) |
| `@limit` with escrow, placement, `@colocate`, write and query classification | | ✓ | stage 8 |
| `Geo`, `Measure`, `quantity`, `unit` | | ✓ | engine geo stage; units with the catalog |
| Modules | | ✓ | after the type-prefix rule |
| GQL graph type and SHACL generation | | | with the query core (stage 7) |

**Suggested order:**
1. The ID layout and the type-prefix rule, as one short decision record. Stages 5.5 and 6 both build on it.
2. Stage 5.5's seams (§4).
3. The UGS specification, split into openspec capabilities: grammar, types, identity, constraints, compilation, views,
   evolution.
4. Stage 6, built test-first against it.

## 7. Resolution

Sorted on 2026-11-01. Bucket 1 holds what had an obvious answer, applied and marked solved; bucket 2 holds the real
decisions, each with its trade-off, a recommendation and the consequences.

### Bucket 1: solved

| # | Finding | Resolution | Applied in |
|---|---|---|---|
| S1 | "3 node clustering" had two readings | Three roles: frontend, compute, storage (user, 2026-11-01) | `ROADMAP.md` stages 5.5 and 8; backlog item 1 |
| S2 | TLA+ or Lean | Lean (user) | Distribution note, verification strategy; backlog item 8 |
| S3 | Deployment targets; who builds and signs images | Kubernetes and Docker, bare metal later; CI builds, signs and publishes OCI images with Bazel's `rules_img` (the server's base by `rules_apko`, D7); the microVM runtime and minimal kernel become tracked options (user) | Distribution note, node runtime; backlog item 1 |
| S4 | ROADMAP prose and clustering items predate the table | Renumbered; VSR, the storage/compute separation and Aeron's stream position at stage 8; n−1 workers removed; typo fixed | `ROADMAP.md` (the MVP sentence waits for D8) |
| S5 | The plan's stage table predates the renumbering | A pointer to ROADMAP's table above it; the rows kept as source material | Dual Query Engine Plan, *Stages* |
| S6 | `today()`, `now()`, `as of time` need a wall time per epoch | Each closed epoch records the sequencer's wall-clock time at its close, never below the previous epoch's; ordering never reads the clock | Distribution note, sequencer epochs |
| S7 | Valid time declared two ways | `@bitemporal(valid: attr)` implies `@valid(attr)`; both on one type are refused | UGS note, language clarifications |
| S8 | A role's cardinality had no default | Always written; a role without one is a compile error (invariants are declared, never inferred) | UGS note, clarifications and grammar |
| S9 | Reserved words missed keywords the grammar uses | Added `module`, `extends`, `statement`, `quantity`, `unit`, `Optional`, `exists`, `order`, `by` | UGS note, lexical structure |
| S10 | The type table merged two RDF mappings; `UInt128` missing | `List` an `rdf:JSON` literal, `Set` one triple per member; unsigned integers stop at `UInt64` | UGS note, type system and clarifications |
| S11 | Imprecise end bounds of closed-open intervals | A bound stated to a precision ends within its unit (outer extent at its end, inner extent at its start) | UGS note, precision and certainty |
| S12 | Writes through GQL and SPARQL may build partial instances | Writes in one transaction assemble into instances, each validated whole at commit, aborting with the instance and the rule | UGS note, clarifications |
| S13 | `min` and `max` folds under deletes | Recomputed from the remaining instances on update or delete; incremental over append-only data | UGS note, clarifications |
| S14 | The inverse navigation of a colocated relation scatters | Stated, and listed in the coordination report | UGS note, placement |
| S15 | The implicit schema on first write contradicts "closed by default" | UGS's own principle governs: the implicit entry goes in stage 6, and `@open` types stage undeclared data | Stage 6 specification (§6) |
| S16 | Minted IRIs embed UIDs | UGS's rule governs: stage 6 mints IRIs from keys and opaque handles; the handle scheme is D6 | Stage 6 specification (§6) |
| S17 | Escrow and rank-based acyclicity are named, not described | Specified with the cross-group protocol | Backlog item 5 |
| S18 | Interactive transactions across groups (backlog item 5) | Answered by UQL: single-group transactions interactive, multi-group ones submitted whole | Backlog item 5 |
| S19 | The operator's view (signals, runbooks, backup, schema rollout, data movement, storage growth, secrets, settings, mixed versions) | Requirements for the roles' and verification specifications; secrets, TLS included, through a typical KMS (AWS KMS, Google Cloud KMS, HashiCorp Vault), rotatable without a restart (user) | Backlog items 1 and 8 |

### Bucket 2: decisions for review

#### D1. The system ID: 64 or 128 bits

- **Status: adopted (2026-11-01).** Recorded in the schema note, the distribution note and backlog item 10.
- **Revised and adopted (2026-11-01):** a 16-bit type prefix, a 24-bit bucket (8 bits from the namespace, 16 from the
  key), a 64-bit sequence unique per type, leased per (type, partition), and 24 reserved bits, for partitions per core on
  large nodes (`UltraGraphDB Shard-per-Core Write Path.md`, P7, P15, P16).

- **Trade-off.** 64 bits keep every format stages 1 to 5 built: Dgraph's key encoding, `UidPack`, the posting format,
  the term encodings, the set operations, and the Go golden vectors that pin them. But a type prefix, a replica-group
  field and a sequence then share 64 bits: 16 / 12 / 36 gives 65,536 types, 4,096 groups and 68 billion IDs per type
  and group. 128 bits leave room for all three with bits to spare, at the cost of rewriting those formats. Because IDs
  never leave the database (UGS), clients never see the width; the cost is storage and the rewrite. Sorted IDs in a
  posting list are delta-encoded, so their size grows less than twofold.
- **Recommendation.** 128 bits: a 16-bit type prefix, a 16-bit bucket, a 64-bit sequence and 32 reserved bits (zero;
  candidates are a tile coordinate and growth room). Make the group field a **bucket**: a virtual partition that the
  placement map assigns to a physical group, so rebalancing moves buckets and never changes an ID. Write the layout as
  a short decision record before stage 5.5 begins.
- **Consequences.** Stage 5.5 rewrites:
  - the key encoding (Dgraph keys embed 64-bit UIDs);
  - the posting and `UidPack` formats (new version bytes);
  - the set operations in `core_algo`;
  - the term dictionary's allocation, which becomes per (type, bucket) counters;
  - the wire protocol's internal term encodings;
  - HNSW's node IDs.

  The `uid-lists` and `key-encoding` specs stop matching Dgraph byte for byte, so their Go golden vectors are retired
  or regenerated. A 128-bit ID is one AES block, which makes D6's handle simple.

#### D2. Type prefixes under schema evolution and modules

- **Status: adopted (2026-11-01).** Recorded in the schema note and backlog item 10.

- **Trade-off.** UGS promises that a supertype is one contiguous ID range, numbered in preorder. A subtype added in a
  later version or a module would shift every later number, and since a type is part of every ID, nothing can be
  renumbered. Either:
  - (a) each type reserves a block for future subtypes, which bounds how many subtypes and modules a type can take and
    wastes prefixes; or
  - (b) prefixes are allocated in declaration order and never change, and a supertype is the set of its descendants'
    prefixes: usually one range, a few once later versions add subtypes.
- **Recommendation.** (b).
- **Consequences.**
  - A type test or a supertype scan checks a handful of ranges instead of one, computed by the catalog per schema
    version.
  - UGS's "one contiguous ID range" becomes "a few ranges".
  - Modules and later versions add types freely.
  - The catalog's preorder numbering (`catalog` spec) becomes declaration-order allocation.

#### D3. The guarantee across groups

- **Status: adopted (2026-11-01).** Recorded in the distribution note and backlog item 5.

- **Trade-off.** Stage 5's `transactions` spec promises strict serializability. Across groups, the distribution
  architecture reads at the latest closed epoch, up to one epoch behind, so a read can miss a commit acknowledged
  moments before. Making every cross-group read strict means closing an epoch per read, a round trip to the sequencer.
- **Recommendation.**
  - Strict serializability within a group.
  - Serializability across groups at a closed epoch, the default, with session tokens for read-your-writes.
  - A per-request option that closes the current epoch first, for a strict cross-group read (the distribution note
    already sketches it).
- **Consequences.**
  - The archived `transactions` and `commit-rounds` specs are restated in stage 5.5's change.
  - UQL's default clause and the wire protocol gain the option.
  - Reads stay cheap by default.
  - The guarantee stays far above read committed, as the bounded-transactions decision requires.

#### D4. The catalog's home

- **Status: adopted (2026-11-01).** Recorded in the distribution note and backlog item 10.

- **Trade-off.** Stage 6 can build UGS on the per-namespace catalog keyspace that stage 4 built, and stage 8 can then
  move it into the metadata group. That makes 5.5 smaller, but schema storage gets built twice. Alternatively, stage
  5.5 introduces the metadata group's state machine with one replica, holding catalog versions, ID ranges and the
  placement map. Stage 6 then builds on it once, and stage 8 only adds replication.
- **Recommendation.** The metadata group in 5.5, a log-backed state machine like the commit step, with one replica.
  It treats catalog contents as opaque versioned values, so its interface does not wait for the UGS specification.
- **Consequences.**
  - Stage 5.5 grows by one state machine.
  - The `catalog`, `schema-storage` and `schema-changes` specs move to the metadata group.
  - Schema versions take effect at epoch boundaries from the start.

#### D5. The cut of UGS version 1 that stage 6 builds

- **Status: adopted (2026-11-01).** §6's table is the cut; ROADMAP stage 6 states it.

- **Trade-off.** The note's version 1 is large: time with imprecision, geometry, units, bitemporality, modules, folds,
  escrow and placement. Building all of it in stage 6 delays the query core and the cluster. Building too little
  leaves DeepBrain's schema unwritable.
- **Recommendation.** §6's table:
  - **built:** the modelling core, local constraints and the RDF view;
  - **parsed and validated:** time imprecision, bitemporality, geometry, units, folds, functions and modules;
  - **deferred:** everything that needs groups or change streams.

  The UGS specification comes first, as openspec capabilities.
- **Consequences.**
  - Stage 6's exit is: schemas compile, enforce their local constraints, and map to RDF through the mapping contract.
  - DeepBrain's schema can be written in full, with its advanced constructs checked but inert until their stages.

#### D6. The opaque handle and its keys

- **Status: postponed (2026-11-01), as personal data is; no key encryption.** The schema note records it. Keyless entities and relation instances need some interim name in the RDF view until a handle scheme is chosen.

- **Trade-off.** A keyed encryption of the ID needs no storage, but its key is a secret, and rotating the key changes
  every keyless IRI. A random handle recorded in the term dictionary needs no key, but costs an entry, both ways, for
  every keyless entity and relation instance; relation instances are the most numerous.
- **Recommendation.** Keyed encryption, with the key version carried in the handle.
  - With 128-bit IDs (D1), AES encrypts an ID as one block.
  - Keys live in the KMS, through envelope encryption with the data key cached.
  - Rotation creates a new version that mints new handles; old versions stay for decryption only, so every IRI ever
    minted keeps resolving.
- **Consequences.**
  - IRIs survive rotation.
  - Destroying an old key version breaks the handles it minted, so key versions are retired, never destroyed.
  - A node without KMS access cannot resolve keyless IRIs until it regains access.
  - With 64-bit IDs the same scheme needs format-preserving encryption (NIST FF1), a further reason for D1.

#### D7. musl or glibc

- **Status: adopted (2026-11-01):** glibc with mimalloc for the server, on a glibc base built from Wolfi packages by
  `rules_apko` in `bazel/oci_images/glibc_base`; musl static on scratch for tools. The investigation, on the aarch64
  Linux machine (12 cores, kernel 7.2, OrbStack):
  - **Which role binary builds for musl:** all three. The roles are one binary with role flags, so the question is the
    dependency graph, and its shipped part compiles no C: the two crates flagged are `io-uring` (pure Rust, pre-generated
    bindings; `bindgen` only behind an unused feature) and `linux-raw-sys` (pure Rust). The one C build in the graph is
    `alloca`, through criterion, for benchmarks, and the hermetic LLVM toolchain compiles it for musl too. With mimalloc
    added, `libmimalloc-sys` compiles C through the same toolchain, as in scrith.
  - **Proof:** `wal_bench` and `store_bench` (fjall, compio and io_uring, the log and the commit pipeline: the storage
    path) build for `x86_64-` and `aarch64-unknown-linux-musl` with `-c opt` as static ELF files, and the aarch64 build
    ran on Linux: 9,994 of 9,994 commits at 2,000 per second, no aborts.
  - **Performance,** `wal_bench` at an offered 50,000 commits per second of 50 relations, 10 s per run, in the VM's
    `/tmp`, a RAM-backed tmpfs (re-measured on disk below):

    | Build | Commits/s | Round p99 (µs) |
    |---|---|---|
    | musl, its own allocator | 446, 460 | about 24,000 |
    | musl with mimalloc | 603, 663, 654 | about 22,500 |
    | glibc, its own allocator | 623, 633, 659, 670, 637 | about 43,000 |
    | glibc with mimalloc | 833, 869, 842 | about 22,600 |

    mimalloc brings musl level with glibc's default build; glibc with mimalloc is about 32% faster again. The remaining
    gap is not the allocator but `memcmp`: a profile puts 18.5% of all time in libc, almost all `memcmp` from fjall's
    memtable skiplist comparing keys; glibc's `memcmp` is vectorised and musl's is a byte loop (backlog item 11). One benchmark, on a VM's
    virtual storage, aarch64 only.
  - **Mimalloc adopted for musl (user, 2026-11-01):** the workspace pins `mimalloc` with `local_dynamic_tls`, the bench
    binaries set it as their global allocator, and `static_binary` and `scratch_multiarch_img` (taken from scrith)
    build them; `wal_bench_multiarch` is the first image.

  - **Re-measured on the VM disk (`/var/tmp`) after the zero-run stored key and the dictionary cache**, both builds
    with mimalloc, relations per second, three runs each:

    | Relations per commit | glibc with mimalloc | musl with mimalloc | musl against glibc |
    |---|---|---|---|
    | 1 | 15,828, 17,695, 16,116 | 16,212, 16,588, 16,251 | level (the fsync per commit bounds both) |
    | 10 | 41,479, 40,581, 42,015 | 36,225, 29,586, 36,980 | −17% (−12% on the medians) |
    | 50 | 42,708, 41,341, 41,258 | 34,724, 33,663, 33,739 | −18.5% |

    The gap survives the allocator and the shorter keys: wherever the CPU bounds the path, musl's byte-wise `memcmp`,
    which the profile found under fjall's memtable comparisons, costs about a sixth of a core's throughput. Under
    shard-per-core the CPU bounds the path, so the gap is hardware: about 20% more cores for the same writes.

- **Trade-off.**
  - *musl, static, on scratch* (scrith's `static_binary` and `scratch_multiarch_img`, adopted 2026-11-01): one file,
    no base image to patch, the smallest attack surface, the same image rules as scrith, MIT-licensed. It costs about
    18% of every partition core's write throughput.
  - *glibc, dynamically linked, on a minimal glibc base*: the vectorised string routines, so the full throughput, at the
    cost of a base whose glibc needs patching. The candidates:
    - Google's distroless `cc`: free, but its updates arrive on Google's schedule and its build is not ours.
    - Chainguard's `glibc-dynamic`: the same contents from Wolfi packages, but only its `latest` tag is free, and
      Chainguard's registry terms are less favourable.
    - Echo's images: subscription only, no public registry, no published minimal glibc base, and no public terms on
      redistributing images built from them.
    - The same contents built here from Wolfi packages with `rules_apko`, Chainguard's own Bazel rules: Wolfi is free
      and open source, so the image is ours under the packages' own licences (glibc LGPL, dynamically linked; libgcc
      GPL with the runtime exception; the CA bundle MPL and MIT), and Bazel pins every package by checksum.
  - *glibc, statically linked, on scratch*: glibc's name-service and `dlopen` paths break when static, and statically
    linking the LGPL glibc obliges shipping what lets a user relink against another glibc.
  - The three role binaries share the base, so the choice covers the frontend and compute roles too, both CPU-bound on
    queries.
- **Recommendation (adopted).**
  - The server binary ships against glibc with mimalloc, dynamically linked, on the Wolfi glibc base:
    `bazel/oci_images/glibc_base` holds its `apko.yaml` (`wolfi-baselayout`, `glibc`, `ld-linux`, `libgcc`,
    `ca-certificates-bundle`, user 65532, no shell, no package manager) and `apko.lock.json`, which pins every package
    by version and checksum for both architectures.
  - Updating the base is `bazel run //bazel/oci_images/glibc_base:lock`, which re-resolves the packages and rewrites the
    lock; the commit of the new lock is the update, and every image built on the base follows it.
  - `glibc_multiarch_img` in `//bazel/container:img.bzl` builds on the base through `rules_img`; musl stays the target
    for client tools and benchmarks through `static_binary` and `scratch_multiarch_img`.
  - Re-measure on bare metal in stage 8; a musl with vectorised string routines would reopen the choice.
- **Proof (2026-11-01).** The base builds for amd64 and arm64 (one layer of 2.7 MB per architecture, glibc 2.44,
  libgcc 16.2). `wal_bench_glibc_multiarch`, the glibc `wal_bench` on that base, loads into Docker with
  `wal_bench_glibc_load` (16.6 MB), has no shell, runs as user 65532, and commits 41,340 relations/s at 50 per commit,
  the VM's own glibc figure. Docker's default seccomp profile refuses io_uring, so the run needed
  `--security-opt seccomp=unconfined`: the server's deployment ships a seccomp profile allowing `io_uring_setup`,
  `io_uring_enter` and `io_uring_register`, beside the io_uring restrictions the distribution note plans.
- **Consequences.**
  - Two image rules: scratch for musl tools, the Wolfi base for the server; `rules_apko` joins `rules_img`.
  - The distribution note's "static musl binary" and the "io_uring without libc" decision change accordingly.
  - The minimal-kernel plan stays tracked under musl in case microVMs return.

#### D8. Where the DeepBrain MVP runs, and where Bolt goes

- **Status: adopted (2026-11-01).** ROADMAP cleaned up: the MVP on stage 8's single container, Bolt as stage 9.5.

- **Trade-off.** ROADMAP still says the MVP runs "on the single node of stage 7", but the parity rule forbids a
  single-node mode. The first stage whose topology the MVP could run on is stage 8, the three roles in one container.
  An earlier MVP would need a single-process path, which the architecture rules out. Bolt lost its row; its old exit
  (Neo4j drivers running the TCK) depends on the conformance work of stage 9.
- **Recommendation.** The MVP on stage 8's single container; Bolt as stage 9.5, after conformance and before full
  distribution.
- **Consequences.**
  - The MVP moves later than the earlier plan.
  - Stage 8's exit includes running the MVP's queries and scrith's catalogue reads.
  - ROADMAP gains a Bolt row.
