# Group 3: the block source, the dictionary and the changeset applier on 128-bit IDs

Scope of this part of group 3 (tasks 3.2–3.4): the new crate `ultragraphdb_traits_blocks`, `ultragraphdb_store_quads`
and `ultragraphdb_store_apply` on `SystemId`, and `ultragraphdb_tool_test`'s changeset workload, which `store_apply`'s
test targets depend on in Bazel. `store_posting`, `traits_index`, `store_hnsw` and the HNSW fuzz target are the other
part's (`notes/group-3-posting-hnsw.md`). Logs and audit scripts are in the session scratchpad (`g3qa/`).

## `ultragraphdb_traits_blocks` (new)

`BlockSource::lease(&self, TypePrefix) -> Option<SequenceBlock>`: the next half-open block of a type's sequences for
this partition, never leased twice to any partition, or `None` when none can be leased now (unreachable, or the type's
sequences spent); it returns at once, a networked source handing out blocks it already holds. `SequenceBlock`
(`new(start, end) -> Option`, `start`, `end`, `count`, `contains`). Behind `test-utils` (on by default, in the Bazel
target's `crate_features`): `TestBlockSource`, counted blocks of a fixed length from 1 per type or scripted blocks,
unreachable on demand, recording its leases, its clones sharing one state. No error type: the caller refuses an
allocation naming the type (`QuadError::no_sequence`). Doc test included.

| | Unit | Doc |
|---|---|---|
| Phase 2 | 15 tests, 15 failed, every one `not implemented` | failed, `not implemented` |
| Final | 17 | passes |

Audit (B01–B11, each planted alone, `audit_blocks.txt`): all caught. B10 as first conceived (a wrapping counter) was
equivalent, `SequenceBlock::new` refusing the wrapped block; its saturating variant survived until
`test_a_block_reaching_past_the_last_sequence_is_not_cut_short` was added. Coverage: 0 uncovered lines (one branch of the
double's debug check needed `test_a_scripted_block_below_an_earlier_one_is_leased`).

## `ultragraphdb_store_quads`

- **IDs.** An IRI's ID is got or created while the write runs (`WriteTerms::iri` → `Dictionary::resolve`): the
  write's own, a pending entry (created by a concurrent execution, not yet committed), the dictionary's, or a new one,
  `SystemId::resource` (bucket from `Bucket::derive`) with a `Resource` sequence from `Sequences` over the
  `BlockSource`. One lock covers lookup, allocation and the pending insert, so concurrent writers take one ID and none
  is refused for it. `TermOrigin` (`Held`, `Pending`, `Created`) decides whether the write writes the entries (`Pending`,
  `Created`) and whether the ID is fresh and absent (`Created` alone).
- **Durability.** Every commit naming a pending or created term writes both its entries (blind, equal values), so they
  are durable no later than the first commit; `QuadStore::settle` forgets the pending entries a written batch holds;
  `QuadStore::release` writes a refused write's entries on their own (naming state: they name nothing that exists) and
  settles them. The pending map holds at most `PENDING_MAX` (2^16, `with_pending_max`); past it, `PendingFull`.
- **Sequences.** Per type, the current block and the block ahead, leased when the current is begun; a used-up block
  gives way to the one ahead; with none held and none leased, `NoSequence` naming the type. Nothing persisted: a restart
  leases new blocks, the old ones abandoned.
- **Names.** Blank nodes: bucket drawn by `OwnedBuckets::draw` among the owned buckets of the tenant's run, then
  `Names::mint_skolem` (104 bits), both from the store's `RandomSource`; reifiers: the subject's bucket. A skolem IRI
  pending or held is drawn again, at most `SKOLEM_DRAWS_MAX` (4) times, then `SkolemDraws`. Statement IDs:
  `SystemId::statement(subject, Statement sequence)`, the reifier's entries in the dictionary both ways.
  `WriteTerms::reifier` resolves a reifier to its statement ID (the write's own, pending or held; never creating).
- **Dictionary keyspace** 0x1B through `core_keys`: `term_key` (kind 1, SHA-256/16 of the term → ID), `id_key` (kind 2,
  ID → text), `literal_key` (kind 3). A blank node is a `Resource` ID whose text is a skolem IRI under the tenant's
  base (no flag byte); reads present it as `b{bucket}-{random}` (`skolem_blank_node`, shared with `store_apply`).
- **Absent terms** recorded by ID (`WriteTerms::absent() -> SystemId`); no dictionary key is ever in a read set.
- `StatementRecord` format 2 (16-byte IDs, IDs validated on decode); `object_posting`/`posting_object` over 128-bit keys,
  a reference key that is no valid ID refused as corruption naming the list.

Phase 1 surface, then phase 2 (`quads_phase2_*.log`):

| | Unit | BDD |
|---|---|---|
| Phase 2 | 91 tests, 89 failed (`not implemented`); passing: the statistics round trip (unchanged), the contract key constant | 22 scenarios, 22 failed, `not implemented` |
| Final | 94 | 22 scenarios, 50 steps |

Scenarios, verbatim (`features/term-dictionary/term_dictionary.feature`, `features/quad-storage/quad_storage.feature`):
"An aborted write leaves nothing that exists", "A read at an older version is unaffected by a newer term" (commit
versions stand for the epochs until group 4), "A dangling hash is reported", "A foreign reference resolves locally"
(partition 2 a second store; the registration's reply stood in for by `WriteTerms::copy`), "An IRI keeps its ID", "An
IRI's bucket is its own", "A blank node's skolem IRI names its bucket and nothing else" (golden IRI for seed 0 computed
in Python from SplitMix64's published outputs and Lemire's reduction: `c74b60-6aa1b965f406c45d188009454f`), "An aborted
creator leaves the IRI usable"; "A triple maintains four lists", "A bound literal object is a list read", "A vector has
no reverse entry", "Graphs do not mix", "Linking a new subject reads none of its lists". "Every key belongs to one
region" and "Forward keys of a predicate sort by subject" are `core_keys`' (group 2). Golden values: bucket of
`http://ex.org/a` 0xC7F1AB and the runs 0xC7/0xF3 from `core_ids/testdata/golden/buckets.json`; SHA-256 prefixes from
Python's hashlib; every key and record byte string written from its layout.

Every error variant provoked through the public API: `NoSequence` (`sequences_tests`), `PendingFull`, `NoOwnedBucket`,
`SkolemDraws` (blank and reifier), `UnknownSkolem` (`write_terms_tests`), `Corrupted` (missing and unreadable entries,
invalid postings, records), `Mapping` (a triple term), carried variants' texts (`quad_error_tests`). `Id` is reached
only by an IRI past 2^32 bytes (`CanonicalKey::iri`), which no small test builds.

Audit (Q01–Q39, `audit_quads.txt`, the suite run under Bazel per defect, sources diffed back to a pristine copy after):
all caught. Q04 first did not compile; its compiling variant Q04b (the run ignored) was caught.

The dictionary workload (simulator, faults) checks that every stored ID has its entry, no ID names two terms and no term
has two IDs; each restart seeds its random source afresh and leases from the same `TestBlockSource`. It found that two
starts drawing from one seed draw one name sequence, so a partition must seed afresh at every start (with four starts
on one seed, `SkolemDraws`). Its run time is unchanged (36 s before, 40 s after, measured on the pre-change tree).

Coverage (`scripts/coverage.sh`), uncovered lines among those changed since `HEAD`: `owned_buckets.rs:73`, the trailing
`None` of `draw` (unreachable: the drawn index lies below the owned count); `quad_write/mod.rs:109`, `QuadWrite::quads`,
which no test of this crate calls; `data_dictionary_describe.rs:61`, a named graph's statistics in `describe`, a type
change of an untested pre-existing branch. The other uncovered lines of the crate are pre-existing code whose behaviour
this change left alone (the data dictionary's describe, statistics codec, `seed`'s entry kinds).

## `ultragraphdb_store_apply`

On the new types throughout (`B: BlockSource` beside `S` and `C`; `Graph` for graphs; `SystemId` for predicates,
subjects, statement IDs, defaults, touched nodes, vector queues, `DropTarget::{Predicate(SystemId), Graph(Graph)}`,
`PredicatePrefix` and its rebuild-record bytes). Behaviour changes:

- `ChangesetApplier::new(lists, blocks, random, executor, seed)`; `release(&Executed)` for an execution the commit
  step refused; a round settles each applied execution's pending IDs; `execute` releases a refused changeset's terms.
- Statements by reifier: `Change::DeleteStatement { reifier }`, `StatementRef::Reifier`; new statement IDs in the
  subject's bucket (`next_statement_id(subject)`).
- `Reads::absent() -> &[SystemId]`.
- `OperationOutcome` (`Node { node, created }`, `Set`, `Counter(i64)`) replaces `(u64, bool)`, since a node's ID no
  longer fits the counter's field.
- `LevelDraw::of_node(seed, namespace, predicate: SystemId, node: SystemId)`, hashing both IDs' halves (golden values
  from a separate Python SplitMix64).
- `search_vectors -> Vec<SystemId>`; the dead list rewritten in the index's hexadecimal form (each kept node through
  `add_dead_node`); the entry point's heir its 16 bytes.

Phase 2 (`apply_p2.log`): unit 200 tests, 37 failed: 32 `not implemented` (27 `LevelDraw::of_node`, 4 `statement_of`,
1 `release`) and 5 on assertions about behaviour not yet written (settling, releasing a refused changeset); BDD 85
scenarios, 13 failed, `not implemented`; `workload_seeds` and `workload_faults` failed `not implemented`,
`workload_replay` passed; the doc test failed to compile on the old constructor (fixed with the example). Final: unit
202, BDD 85 scenarios and 230 steps, the three workloads, the doc test.

Scenarios: changesets (verbatim, both requirements): "A delete by pattern removes every object", "A new term's ID is
fixed before the commit", "Written regions are reported", "An insert records the statement it tested", "A new term
leaves no dictionary key in the read set"; term-dictionary: "A statement ID's reifier is a skolem IRI", "Two creators
share one ID" (the "exactly one commits" half asserted as the commit step's inputs: the second tested the statement the
first wrote, the first found `<new>` absent and the second wrote a statement of it; the commit step itself is group
4's); quad-storage "Statement IDs" (both, new text); vector-index "HNSW vectors live in posting lists" (all four, new
text, the entry point's bytes and the dead list's text checked against the spec's format). Index-maintenance's "Count
indexes" is group 7's (task 7.2).

Audit (A01–A18, `audit_apply.txt`): all caught but A06 (a new statement ID in its predicate's bucket), which survived;
`test_an_edge_with_properties_and_a_parallel_edge_take_statement_ids_in_their_subjects_bucket` was added and A06b caught.

Coverage, uncovered lines changed since `HEAD`: `changeset_write_operation.rs:160`, the blank-node fallback of
`node_term` (reached only when an operation's node has no entry the write or the store holds); `changeset_write/mod.rs`
263–264 and 350, a blank-node graph in `find_graph` and a typed literal's datatype decoded from the write's own entries
(pre-existing branches, retyped); `overlay_txn` `find`/`first_passing`'s found branch (the build's HNSW asks for a new
entry point only when one died during the build); `predicate_prefix` 120 and 125–126 (a key whose graph field is all
ones or no valid ID: only a corrupt key reaches them). The rest of the crate's 127 uncovered lines are pre-existing.

`cargo fmt -p` and `cargo clippy -p <crate> --all-targets -- -Wclippy::all` report nothing for the four crates.

## `ultragraphdb_tool_test`

Its `ChangesetWorkload` moved to the new API (a `TestBlockSource` outliving restarts, a fresh random seed per start,
statements deleted by the store's and the model's reifiers, ID-keyed index checks). Without it no `store_apply` test
target builds in Bazel (`all_crate_deps(normal_dev = True)`), nor `store_kv`'s.

## Decisions to review

1. **Deletes and reads get or create IDs.** A delete or a read naming an IRI the store does not hold creates its ID (no
   statement holds it, so nothing is read) and records it absent by ID, which is what the spec's "terms found absent,
   each recorded as no statement of the term" needs: no other key would name the absence. The cost: dictionary entries
   for IRIs only ever deleted or read. An unheld skolem IRI is the exception (`find` gives `None`, `iri` refuses it).
2. **Refused writes' entries are written** (`release`, and `execute` on a refusal), as the spec allows ("whether or not
   the dictionary holds an entry"), so no pending entry outlives its writers. Executions begun with
   `ChangesetApplier::begin` and dropped unfinished, and refusals the commit step makes, need `release` called; until
   group 4 calls it, their pending entries stay until a later write names the term (bounded by `PENDING_MAX`).
3. **No persisted sequence position.** A restarted partition abandons its blocks and leases new ones.
4. **Skolem draws bounded at 4** and a random source that must be seeded afresh per start.
5. **`OperationOutcome`** replaces the overloaded `(u64, bool)`; the wire's `OperationResult` (group 8) must follow.
6. **`StatementRecord::encode`** writes at most 2^16 − 1 properties (a debug check; more are not written): nothing caps
   a statement's properties upstream.
7. **The skolem flag is derived** from the text (a `Resource` ID named by a skolem IRI under the tenant's base), not
   stored.
8. **`DropTarget::Graph` holds a `Graph`**, so the default graph is droppable by target (PROTOCOL.md §12.2's record
   encoding is group 8's).
9. `tool_test` changed (group 4's list) because `store_apply`'s targets depend on it.

## Texts left that read as Dgraph's

Not touched here, listed for the user: `HnswError::key_not_found` (OverlayTxn answers a failed find with it, as the
store's own transaction does); `ChangesetError`'s texts (unchanged); "Transaction has been aborted. Please retry" is
gone from `store_quads` (its `Aborted` variant removed) but may live in other crates.
