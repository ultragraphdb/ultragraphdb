# The stage 6 change against the core principles

* Status: 2026-11-10, all eleven recommendations accepted by the user and applied the same day (design decision 15,
  task 0.15). Where a recommendation left a choice, the one applied:
  - P2: code 7.
  - P4: a number of its own, `UGS1040`, past `rule_read_max`, 4,096 instances; `@acyclic` without `max_depth` stays
    legal under that bound.
  - P5: a full compile queue answers code 5 (Busy), added to `PROTOCOL.md`'s code table.
  - P6: the retained bytes per tenant within 16 times `catalog_bytes_max`.
  - P7: the `Resource` prefix, the IRI's bucket and sequences from 2^63, below which `Resource` leases stay. The
    sequences sit in a seventh section of the version, the predicate index, and in no entry: Describe carries entries
    as they stand, and a system ID never leaves the database.
  - P10: the lifts are group 2b.
  - P11: `ultragraphdb_core_body`.
* Method: the change (proposal, design, tasks, the capability specifications), `SCHEMA.md`, `CATALOG.md` and
  `PROTOCOL.md`, read against the three tenets and the six key properties, and checked against the code at `d1aad303`
  by three read-only surveys: the frontend and the metadata group, the partition's commit path, and compute with the
  wire's body code. Every claim about the code below carries its file and line.
* Already held: `CATALOG.md` settles the catalog's own bytes by the tenets (`review-resolutions.md`, R3): one encoding
  verified on load, views over the bytes, programs evaluated in a loop without allocation, Describe copying entries as
  they stand. The findings below are about what surrounds it: where the work runs, what the commit reads, what is
  copied, and what the plan assumes of the code it changes.

## Summary

| # | Tenet | Finding | Kind |
|---|---|---|---|
| P1 | Correctness | A revision can break `@limit` unchecked: `SCHEMA.md` 8.2 checks it at an update of the aggregated attribute only, and Gen5's `rank` is read by a condition | Specification defect |
| P2 | Safe | A compiled version is committed unverified, and a version no role can decode stops every role, again at every restart | Specification gap |
| P3 | Safe | The parser is specified as recursive descent; Ultra rule 1 forbids recursion | Contradiction with the rules |
| P4 | Safe, Speed | `@limit`, `@no_overlap` and `@acyclic` read beyond the instance at commit with no stated cost and, for two of them, no bound | Decision needed |
| P5 | Speed, Scale | The Install compile is placed on the owner thread, which also runs the sequencer and the metadata group | Specification defect |
| P6 | Speed | A catalog version is copied at every hop and deep-cloned per transaction; at 16 MiB the copies are the cost | Design gap |
| P7 | Speed, coordination | A governed tenant still resolves predicates by name on the commit path | Decision needed |
| P8 | Speed | `regex::Regex` shared across cores contends on its internal cache pool | Decision needed |
| P9 | Speed | The lexer and parser serve UQL on the data plane, where rule 3 asks for no allocation per request | Design gap |
| P10 | Safe, Speed | The commit path's crates are far from Ultra Style; their lifts are one line each in the tasks and gate groups 6 to 8 | Plan gap |
| P11 | Scale | The body reader `CATALOG.md` reuses is private to the wire crate, and the dependency it needs is a cycle | Decision needed |

## Findings

### P1. A revision can break `@limit` unchecked

- **Finding.** `SCHEMA.md` section 8.2 checks `@limit` "at an insert, update of the aggregated attribute, delete or
  retraction". An update of an attribute the `where` condition reads changes which instances are counted, and it is not
  listed. Section 8.4.2 makes a revision "no rule of its own", so nothing else catches it. Section 9.4 step 2 lists
  the operations that could violate a constraint from the same reading, so the classification marks such an update
  free of the constraint.
- **Consequence.** Gen5's `Relation @limit(count() <= 1 per (subject, type) where rank = preferred)` covers all 87
  relation types. A revision setting `rank = preferred` on a second instance commits, and the subject holds two
  preferred relations of one kind. In stage 8 the wrong classification would route that update coordination-free.
- **Recommendation.** The row reads "at an insert, an update of the aggregated attribute or of an attribute its
  condition reads, a delete or a retraction". Section 9.4 step 2 and the `ugs-enforcement` requirement follow, with a
  scenario: Gen5, a second instance revised to `preferred`, refused with `UGS1023`.
- **Lands in.** `SCHEMA.md` 8.2 and 9.4; `ugs-enforcement`, "Acyclic relations, limits and no-overlap"; the corpus
  (task 0.10).

### P2. A compiled version is committed unverified

- **Finding.** `ugs-catalog` has the metadata group treat a version's bytes as opaque, and has every role that cannot
  decode a version "its log or its metadata group names" stop. Nothing decodes the compiler's output before it is
  committed.
- **Consequence.** One encoder defect commits a version that every partition, compute process and endpoint core
  refuses. Each stops, and at restart replays the log naming the version and stops again. The cluster cannot come back
  without editing the metadata log. Fail-stop is right for corruption; here it amplifies a compiler bug into a
  cluster-wide outage.
- **Recommendation.** Install decodes the bytes it compiled with the decoder every role uses, runs all eleven steps of
  `CATALOG.md` section 6, and encodes the decoded version again to equal bytes, before it asks the owner to commit.
  A failure is an invariant error of the compiler (Ultra rule 5): the Install is refused, nothing is allocated or
  committed, and the fault is logged with the step and offset. The check costs one decode on the control plane, and it
  is CATALOG.md's own round-trip test, run on every Install instead of only in the suite.
- **Decide.** The refusal's code. Code 7 (Refused) with the message naming the compiler fault is the closest existing
  one; no code means "server fault" today.
- **Lands in.** `ugs-catalog`, "Install compiles a tenant's sources into a new version", with a scenario (a corrupted
  encoder output, injected through the test utilities, is refused and leaves the tenant without a version); design
  decision 11.

### P3. The parser is specified as recursive descent

- **Finding.** Task 2.3 asks for "the recursive-descent parser of section 4 with every nesting bounded", and
  `ugs-syntax` says the parser "SHALL hold its recursion within the nesting bound". Ultra rule 1
  (`openspec/protocols/ULTRA_STYLE.md`): every function is reached through an acyclic call graph, and a recursive
  structure is walked "with an explicit stack whose depth has a named bound". Every crate this change creates conforms
  from its first commit.
- **Consequence.** As written, group 2 is built against the repository's first rule, or carries an exception nobody
  decided.
- **Recommendation.** Parsing and every walk of the compiler use explicit stacks bounded by the source bounds of
  `SCHEMA.md` section 3.7:
  - Declarations: a loop with a nesting stack of at most 64.
  - Expressions: an operator-precedence loop that emits the postfix form `CATALOG.md` section 4 stores, typed with a
    type stack in the same walk the decoder uses to verify depth. One form then serves parsing, typing, storage,
    verification and evaluation, a program's depth is bounded by the nesting bound by construction, and no expression
    tree is built for a check. Path derivations and function bodies, stored as text, are checked by the same loop.
  - Inheritance, imports, the function call graph (`UGS0118`) and relation trees: worklists bounded by the type, source
    and parameter bounds.
  - The UQL subset's parser likewise.
- **Lands in.** `ugs-syntax`, "A source is bounded UTF-8 text"; tasks 2.3 and 10.3; design decision 6.

### P4. Rules that read beyond the instance have no stated cost or bound

- **Finding.** The rules a commit checks by reading other instances in the partition:
  - **Cardinality.** Count indexes serve it (`SCHEMA.md` 12.4): one read.
  - **`@limit`.** "The aggregate" (9.3) has no storage: as specified it is a read of every live instance of the group,
    with its condition evaluated on each.
  - **`@no_overlap`.** A read of the group's live instances and their valid intervals.
  - **`@acyclic`.** A walk "bounded by `max_depth`" (12.4). `max_depth` is optional (6.7). With it the walk's depth
    is bounded and its breadth is not.
  - **`restrict` and `cascade`.** Bounded by the count index and by `max_write_set` (`PROTOCOL.md` code 6), so they
    are already settled.
- **Consequence.**
  - **Speed.** Under Gen5's `Relation @limit`, every preferred insert reads the subject's instances of that kind:
    *n* reads at a subject holding *n* of them, and the ingestion of one hot subject is quadratic.
  - **Safety.** A walk or read without a bound breaks Ultra rule 2, so its latency is the data's to decide.
  - "Design for it" in `ULTRA_STYLE.md` asks for this sketch before code; task 7.8 only measures after the fact.
- **Recommendation.**
  1. **`@limit` keeps its aggregate.** Per group, a count or a sum lives in the partition's marks keyspace and is
     written in the same local commit as the change, as a count index is: one read and one write per affected limit,
     whatever the group's size. A condition's change of truth moves an instance in or out of the count, so P1's
     update is the same write. Stage 9.5's tightening scan builds the aggregates of a new `@limit`.
  2. **`@no_overlap` and `@acyclic` walk within named bounds,** and a commit whose walk would pass one is refused,
     fail-closed, naming the bound. Gen5's two `@no_overlap` groups (a post's holders, a unit's parents) and its one
     `@acyclic` (`max_depth: 32`) are small. A refusal for a large group is honest; a latency set by the data is
     not.
- **Decide.**
  - The refusal past a walk bound: a new `UGS10xx` ("a rule's read passes its bound"), or `UGS1039`, widened from
    expressions to rules.
  - Whether `@acyclic` without `max_depth` stays legal. If it stays, it takes the walk bound as its implicit depth.
- **Lands in.** `SCHEMA.md` 6.7, 8.2, 8.6, 10.4 and 12.3–12.4; `ugs-enforcement`; tasks 7.5 and 8.3.

### P5. The Install compile runs on the owner thread

- **Finding.** `ugs-catalog` ("The metadata group's owner SHALL compile the sources") and `metadata-group` ("Its owner
  SHALL compile an Install's sources") place the compile on the owner, and `client-endpoint` asks only that it run
  "off the connection's turn". The owner is one thread, `frontend-owner`, that runs the metadata group and the sequencer
  in one turn (`ultragraphdb_common/ultragraphdb_common_frontend/src/utils/owner_thread.rs:121-149`;
  `src/types/owner/owner_turn.rs:79-127`). Outside a sync it answers every ask inline, `BeginCommit`,
  `CheckSession` and `TenantFacts` among them (`owner_syncing.rs:61-110`; `owner_answer.rs:30-98`). The frontend has no
  thread for CPU-heavy control-plane work.
- **Consequence.** While Gen5 compiles, no epoch closes and no commit anywhere in the cluster begins. Every partition's
  latency then rises by the compile time. `catalog_bytes_max` is 16 MiB of output, so a large schema stalls for longer.
  Today the owner already decodes catalogs inline to answer Describe (`owner_catalog.rs:62-79, 225-239`).
- **Recommendation.**
  - **The compile.** It runs on a control-plane thread the frontend role starts at startup and hands work by a bounded
    channel (Ultra: threads injected, queues bounded). Its inputs are the sources, the tenant's base version and the
    `prefix_next` of that version. Stage 6 allocates prefixes once per tenant, from 3 in declaration order, so the
    version is fully determined off the owner.
  - **The owner.** It checks the base, commits the bytes with the prefixes they allocate, and answers. Its turn grows
    by one append, as any catalog commit's does.
  - **Describe.** It is answered from the version's bytes by the endpoint core (P6), with no decode on the owner.
  - **Determinism.** The state machine's apply stays deterministic and cheap, which VSR's replicas need later: what a
    replica replays is the bytes, never a compile.
- **Lands in.** `ugs-catalog` and `metadata-group` (the owner commits, the compile runs beside it); `client-endpoint`,
  "The frontend serves Install"; task 5.2; design decision 2.

### P6. A catalog version is copied at every hop

- **Finding.**
  - **The metadata group.** It stores a version's bytes twice, once in `head` and once in `state`
    (`ultragraphdb_cluster/ultragraphdb_cluster_metadata/src/types/metadata_state/metadata_state_apply.rs:200-202`).
    A snapshot clones the whole state (`metadata_group_snapshot.rs:15-17`), and each Catalog reply copies the bytes
    (`owner_answer.rs:322-349`).
  - **The partition.** It deep-clones its `Catalog`, a set of `BTreeMap`s, into every execution
    (`ultragraphdb_store/ultragraphdb_store_apply/src/types/changeset_core/mod.rs:220`), and in five other places.
    It reaches its catalogs through a `Mutex<BTreeMap<u64, Arc<Catalog>>>` (`mod.rs:62-64`).
  - **The limits.** A version is capped at 523,000 bytes today (`metadata_command/mod.rs:185-187`). `RETAINED_MAX` is
    1,024 versions a tenant (`metadata_state/mod.rs:80-99`).
- **Consequence.**
  - At the specified 16 MiB, the bound is 1,024 × 16 MiB × 2 copies, 32 GiB a tenant, before snapshots.
  - A per-transaction copy of the catalog is the opposite of decision 11's views, built once.
  - The endpoint cores need the version for every governed-tenant check of a one-frame write and for every UQL
    compile. If each asks the owner per frame, the data plane makes a cross-thread round trip.
- **Recommendation.**
  - **One immutable buffer.** A verified version is one allocation per process: immutable, shared read-only, `Arc<[u8]>`
    with rule 9's reason (immutable after verification; every core reads it, none writes it).
  - **Who holds it.** The metadata group's `head`, `state` and snapshots hold that buffer. A Catalog reply and Describe
    write slices of it. Each core's views borrow it. An execution holds the version and never a copy.
  - **The bound.** The metadata group bounds its retained versions in bytes, not only in count.
  - **The cores.** Each endpoint core and each partition holds the version in effect per tenant, refreshed when a close
    announces a new one, as `RoleLinks` holds the placement map today
    (`ultragraphdb_common/ultragraphdb_common_frontend/src/types/role_links.rs:18-36`).
- **Lands in.** Design decision 11, with a paragraph on ownership; `metadata-group`, the retained bytes' bound; tasks
  4.6 and 5.2.

### P7. A governed tenant still resolves predicates by name

- **Finding.** Each IRI of a write goes through these steps:
  - a `BTreeMap<String, SystemId>`, then the dictionary's `Mutex`, then a store read
    (`ultragraphdb_store/ultragraphdb_store_quads/src/types/write_terms/mod.rs:111-136`; `dictionary/mod.rs:89-123`);
  - at commit, each inserted or removed quad's predicate ID is turned back into its IRI by a store read, to find its
    declaration (`changeset_write_logical.rs:153-166`);
  - a predicate whose bucket another partition owns is kept by IRI (`unresolved`).

  Compute resolves predicate IRIs by a dictionary scan per read, per partition
  (`ultragraphdb_common/ultragraphdb_common_compute/src/types/partition_rows/mod.rs:155-236`).
- **Consequence.** The catalog lookups of decision 11 compare no name, but the step before them still does, with a lock
  and a store read per term. That cost repeats for every write and read in the tenant, and predicates whose home is
  another partition need registration.
- **Recommendation.** In a governed tenant, the metadata group allocates each declared predicate its ID with the
  version, as it allocates type prefixes: in the log, once, never reissued.
  - **Why it is possible.** A governed tenant holds no statement at its Install (`ugs-catalog`), so no earlier ID stands
    in the way. `CATALOG.md` format 1 is unimplemented, so the Predicate entry takes the field now at no cost.
  - **Resolution by IRI.** A predicate IRI resolves by a binary search over the version's Predicate entries, which are
    sorted by IRI already.
  - **The table.** The ID-to-entry table is complete at load and identical in every partition and compute process.
  - **The gain.** No commit or read resolves a predicate through the dictionary, takes its lock or registers it.
  - **Ungoverned tenants.** They keep the dictionary.
- **Decide.** This changes `system-identity` and `term-dictionary`. The encoding of a predicate ID (its prefix,
  sequence and bucket) is the design's to choose.
- **Lands in.** `CATALOG.md` 5.5; `SCHEMA.md` 7.1 and 10.2; `system-identity`, `term-dictionary` and `metadata-group`
  (the allocation); tasks 4.3, 4.4 and 6.4.

### P8. `regex::Regex` shared across cores contends on its cache pool

- **Finding.** Decision 9 pins `regex`. A `regex::Regex` matches through an internal pool of caches. The thread that
  created it takes a fast path; other threads take a lock and allocate a cache on first use.
- **Consequence.** A pattern shared by the partitions breaks share-nothing on the data plane.
- **Recommendation.** Use `regex-automata`'s `meta::Regex`, the engine `regex` is built on, with the same syntax
  through `regex-syntax`. Each partition builds its patterns and one `Cache` per pattern when it loads a version, on
  the control plane, and passes the cache by `&mut` at every match: no lock, and no allocation after load.
  `regex-automata` is already in the dependency tree under `regex`.
- **Decide.** Whether this refinement stays within the acceptance of 2026-11-10.
- **Lands in.** Design decision 9; `AGENTS.md`'s dependency table; task 7.4.

### P9. The lexer serves UQL on the data plane

- **Finding.** `ultragraphdb_schema_uql` reuses `ultragraphdb_schema_syntax`'s lexer (decision 5), and the frontend
  compiles every Query of `language` 3 on the connection's core. That is session-layer request handling, which Ultra
  rule 3 places on the data plane.
- **Recommendation.**
  - **Tokens and syntax nodes borrow from the source** as spans; no string is copied for a name or a literal until a
    value is built. This holds for UGS on the control plane too, so the two languages share one lexer of one shape.
  - **The UQL compile writes into buffers each connection holds,** sized at startup by the frame bound (a Query's text
    is within `max_frame`) and the nesting bound of 64. The plan is cleared and reused per request.
- **Lands in.** Design decisions 5 and 6; tasks 2.2 and 10.2.

### P10. The commit path's lifts are larger than the tasks read

- **Finding.**
  - **No plane statements.** The crates groups 6 to 8 change hold none: `ultragraphdb_store_apply` (8,986 lines of
    `src/`), `ultragraphdb_transactions_commit` (6,005), `ultragraphdb_transactions_txn` (2,984) and
    `ultragraphdb_core_catalog` (1,531). `ultragraphdb_store_quads` (4,783) has one.
  - **Rule 3 is unmet.**
    - Transaction buffers start empty and "grow with the first transaction"
      (`ultragraphdb_transactions_txn/src/types/txn_buffers/mod.rs:17-21`).
    - Each transaction builds its `BTreeMap`s and `BTreeSet`s afresh (`changeset_write/mod.rs:96-116`).
    - A quad key is a `Vec` (`logical_quad.rs:54-62`), as is a dictionary key (`dictionary_keys.rs:15-34`).
  - **Rule 2 is unmet.** The commit queue is `usize::MAX` "unless the node sets it", and no production caller sets it
    (`ultragraphdb_transactions_commit/src/types/commit_step/mod.rs:191, 249-250`).
  - **The tasks.** They give each lift one line (6.1, 7.1, 8.1) inside the group that then builds on it.
- **Consequence.**
  - If whole-instance validation is built on these crates as they stand, it inherits their allocation and is
    retrofitted later.
  - If the lifts happen inside groups 6 to 8, each group carries a refactor of several thousand lines beside its
    feature, against crates other groups of the same wave also touch.
- **Recommendation.**
  - **A wave of its own.** The lifts of `ultragraphdb_transactions_txn`, `ultragraphdb_transactions_commit`,
    `ultragraphdb_store_apply`, `ultragraphdb_store_quads` and `ultragraphdb_cluster_partition` run as a wave between
    wave 1 and group 4. They touch none of group 3's crates, so they run beside it.
  - **Its scope.** Planes stated, buffers sized at startup, the commit queue bounded, keys on fixed buffers, the
    catalog held as P6 has it.
  - **Its gate.** Behaviour unchanged and the suites green. Throughput is compared with
    `openspec/benches/benches-2026-11-09.md`, and a change is kept on evidence.
  - Groups 6 to 8 then build validation on lifted code. The removal of the three-kind encoding (task 4.5) stays in
    group 4.
- **Lands in.** `tasks.md`: a group between 2 and 3 (or 3a) and the wave line; 6.1, 7.1 and 8.1 narrowed to what is
  left.

### P11. The body reader `CATALOG.md` reuses is private to the wire

- **Finding.** `CATALOG.md` lends the wire's body layout and string list to the catalog "so one reading discipline
  serves the wire and the catalog".
  - **Where they live.** The reader `Sections` and the string list's reader and writer are `pub(crate)` in
    `ultragraphdb_protocol_wire` (`src/utils/sections.rs:16-74`, `src/types/string_list.rs:33-68`,
    `src/utils/string_writer.rs:13-42`).
  - **Why a dependency fails.** `ultragraphdb_protocol_wire` dev-depends on `ultragraphdb_core_catalog`
    (`Cargo.toml`). A dependency of the catalog on the wire is therefore a dev-dependency cycle.
- **Recommendation.** Group 1 moves the three into a crate of their own under `ultragraphdb_core/` (for instance
  `ultragraphdb_core_body`), created in full with its tests and fuzz seeds. Both the wire and the catalog then depend on
  it, and the catalog does not depend on the wire.
- **Decide.** The crate and its name.
- **Lands in.** Task 1.2; design decision 6's crate table; `AGENTS.md`.

### Outside this change

`OWNER_CORE` is 1, and endpoint core 1 also asks for CPU index 1, so with `pin_cores` on, the owner thread (sequencer
and metadata group) shares a CPU with an endpoint core
(`ultragraphdb_common/ultragraphdb_common_frontend/src/utils/owner_thread.rs:21-22`;
`ultragraphdb/ultragraphdb_frontend/src/service.rs:228, 245`). This is for the user to place; it is not stage 6's.

## The plan once these are decided

1. **Specifications first.** These changes come before any group starts:
   - P1, P2, P3, P4, P5 and P7 into `SCHEMA.md`, `CATALOG.md` and the capability specifications;
   - P6, P8, P9 and P11 into the design;
   - P10 into `tasks.md`;
   - `openspec validate --strict` green afterwards.
2. **The rest of group 0.**
   - Task 0.5: `QUERY-ULTRA-QL.md` section 12's marks and `ROADMAP.md`'s sentence.
   - Task 0.8: `specification/schema/BUILD.bazel`.
   - Task 0.11: `lake build ApgModel`, which holds no `sorry`, and the oracle's export by `lake exe apg_oracle`.
   - Tasks 0.9 and 0.10: the two corpora, the largest pieces of group 0. They are written independently of any compiler,
     so they run as agents beside wave 1. The diagnostic corpus (0.9) is needed before group 2's and group 3's
     phase 2, and the instance corpus (0.10) before group 7's.
3. **Waves.**

   | Wave | Groups | Contents |
   |---|---|---|
   | 1 | 1 and 2 | The wire and syntax; group 1 includes P11's extraction |
   | 2 | 2b, P10's lifts, and 3 | Disjoint crates |
   | 3 | 4 | |
   | 4 | 5 and 6 | |
   | 5 | 7, then 8 | |
   | 6 | 9 | |
   | 7 | 10 and 11 | |
   | 8 | 12, then 13 | |

   Each wave is gated by `make build` and `make test` and committed by the user.
4. **Measured before it is relied on.**
   - Gen5's compile time (task 3.6): it bounds what an Install costs the compile thread.
   - Gen5's encoded bytes, moved from task 4.6 to task 4.4, as soon as the encoder emits it: the cap of 523,000 bytes
     in the code and one-frame Catalog replies decide whether Gen5 needs continuation frames before group 5.
