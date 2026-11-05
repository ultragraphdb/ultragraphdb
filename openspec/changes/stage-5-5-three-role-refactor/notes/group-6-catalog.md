# Group 6, the catalog side (`core_catalog`, `traits_catalog`, and the partition in `store_apply`, `transactions_txn`, `transactions_commit`)

Agent "catalog" of wave 5, from `cabae5ec`. Scope: task 6.1, and of 6.2–6.5 everything a partition does with the
catalog. Logs are in the session scratchpad (`catalog/`).

## 6.1 The Ultra Style lift of `core_catalog`

Bazel before and after, the same counts: unit 13, BDD 6 scenarios / 12 steps. `cargo fmt -p` and `cargo clippy -p
ultragraphdb_core_catalog --all-targets` report nothing. The diff is `catalog/lift.diff` (3 files). Review of the rules
no lint checks:

- **Rule 1.** No function calls itself or reaches itself through another.
- **Rule 2.** `Catalog::declare_type`'s walk up the hierarchy was a `while let` that a catalog assembled from stored
  entries holding a parent cycle never ends: it is now a `for` bounded by the number of types plus one, refusing a
  longer chain as a cycle. `preorder`'s `while let` pops each type once (each has one parent, so it is pushed once), so
  it ends within the types; a comment says so. The decoder's loops iterate counts read from the bytes, each turn taking
  bytes, so they end at the entry's end.
- **Rule 3.** Control plane: the catalog changes on a declaration, a version or a drop, never per write.
- **Rule 5.** Debug checks added: the encoder's output decodes back to its declaration (paired with the load), the
  reader's cursor stays within the entry, a type and a predicate declared are held, `preorder` numbers each type once.
- **Rule 7.** The reader indexed (`take(1)?[0]`, `b[0], b[1]`, `b[0..4]`) and added unchecked (`self.at + n`, a length
  read from the entry): it now reads fixed arrays through `try_from` and computes the end with `checked_add`; a
  `u32` length converts through `usize::try_from`. Found here, fixed with the versions' work (phase 4): the encoder
  wrote a list's count as `n as u16` and a string's length as `len as u32`, silently corrupting a declaration of more
  than 65,535 tokenizers, options, domains or roles. `check_lengths` now refuses such a declaration with the new
  `CatalogErrorEnum::TooLarge`, in `Catalog::check_predicate` and in every version encoded.
- **Rule 8.** No `cfg` in `src/`.
- **Rule 9.** No `Arc`, interior mutability or trait object in `src/`.

## What was built

**The partition-side interface**, `ultragraphdb_traits/ultragraphdb_traits_catalog` (no dependencies), posted as
`SURFACE READY`: `CatalogSource` (`version_at(namespace, epoch)`, the version in effect, `None` for the initial
catalog; `version(namespace, version)`, for a replayed build's start record; an associated `Error`), `HeldVersion`
(tenant, number, effective epoch, opaque contents), `DefaultEntry` (tenant, predicate IRI, the encoded default
declaration), and behind `test-utils` the double `TestCatalogSource` with `CatalogUnreachable`. The metadata agent
implemented `CatalogSource` on `MetadataState` and takes `DefaultEntry` as its command's payload.

**Catalog versions** (`core_catalog`): `CatalogContents` — a version's declarations by IRI, since the frontend and every
partition read it and share no dictionary — encoded as format 1, tenant, version, count, then each declaration's
entry value with its length; `decode` refuses bytes of another format, ending early, running on, naming another
tenant or version, or holding a declaration that does not decode, with `CatalogErrorEnum::CorruptedVersion`
naming the tenant and version. `Catalog::version`/`with_version`.

**The partition** (`store_apply`):
- No catalog key is written: a write's declarations and default entries, a build's swap and undo, a build end's
  proposal and the drops change the catalog held in memory only. `catalog(ns)` no longer reads the store; it is the
  installed catalog, or empty until a version is loaded or the first write gives the initial one.
- `load_catalog(contents)` installs a version at start, before replay: the initial catalog beneath it, every IRI looked
  up in the dictionary (one the dictionary does not hold is corruption: the round that installed the version gave each
  its ID), nothing checked, built or written.
- `execute_catalog(contents)` is the round of a version taking effect: an execution declaring each declaration (checked
  against the data, starting a build, removing indexes) and removing each predicate declaration the version no longer
  makes (`Execution::undeclare`), but for `rdf:type` and implicit default entries. A declaration the data or the catalog
  refuses is returned as a refusal and its predicate keeps its declaration; refused whole with `Rebuilding` while a
  build runs. Applied, it installs the catalog as the version's number.
- `take_default_entries()`: each round installing a default entry of a predicate the installed catalog did not hold asks
  for its `DefaultEntry` once; a refused or released write asks for none; replayed rounds ask again.
- `recover_builds()` replaces finishing the rebuild records at open (which with no stored catalog would have dropped
  every index a record names): called after the catalogs are loaded, it drops the indexes a record names that the
  loaded version does not declare and runs the build of the ones it declares again from the data at the watermark.
- `restore_build` is removed (it read the catalog entry a build end's record no longer carries; the commit crate never
  called it). `build_record()` names the build running.

**The transaction** (`transactions_txn`): `Transaction::write` refuses a changeset holding a declaration with
`TxnErrorEnum::Declaration`, "declarations are committed through the metadata group's catalog", before the execution
sees it, so nothing is written and the transaction goes on (user's decision, task 6.3). `TxnError::code()` maps it,
like every refusal but the past-limit, too-large and cross-partition ones, to `ErrorCode::Refused`. A transaction begins
against the catalog version the partition holds installed. `Prepared::catalog(applier, contents, buffers)` is the
version's proposal: a kind-3 record `StartBuild(version)` with the record's `catalog` set to it, its read set what the
checks read, with the refusals.

**The commit step** (`transactions_commit`): `Pipeline::install(ticket, contents, now_us)` proposes a version taking
effect and returns the refusals for the host to report; a build running leaves the pipeline as it was, a store failure
stops it (`PipelineErrorEnum::CatalogVersion`). `Replay::run_with_catalog(partition, applier, log, capacity, source)`
replays a `StartBuild(v)` record by executing version `v` from the source again, its build started again from the
record's version, and a `CompleteBuild` record by ending the build running; `run`/`run_in` replay with no source
(`NoVersions`), a start record then refused with the new `RecoveryErrorEnum::Catalog`. Replay no longer forgets the
catalogs after each round.

**The workload** (`tool_test`): a reopened store loads the workload's declarations as a version and recovers its builds,
instead of finding them in catalog keys. `workload_differential`, `workload_faults`, `workload_replay` and
`workload_seeds` pass.

## The moved tests (task 6.3)

Moved onto the catalog path, none dropped: txn `test_a_declaration_indexing_a_predicate_with_data_starts_a_build` (now
`Prepared::catalog`, checking the kind-3 start record); commit `test_a_second_build_waits_for_the_first_builds_end`
(two versions' proposals), `test_a_builds_swap_takes_its_version_from_the_step_and_a_later_write_commits` (a close
announcing version 2, then `Pipeline::install`; versions now of epoch 2), and
`test_a_builds_end_that_cannot_be_proposed_stops_the_pipeline`: a build's end reads nothing of the store any more, so
the failure is provoked at the build's other proposal, its start, the version's checks reading the data, which stops
the pipeline. The BDD steps "a round admits a declaration of `ns` that starts an index build of `p`…", "a declaration
of `ns` commits before it" and "a term index on `name` is being built…" are rebound on versions announced by a close and
installed by `Prepared::catalog`, the first two under the spec's new texts.

## Scenarios bound

- `core_catalog` (`features/schema-storage/catalog_corruption.feature`): "A catalog version that does not decode refuses
  the load", over `TestCatalogSource`. The removed requirement's feature (`catalog_storage.feature`) was `git mv`ed to
  `store_apply/features/schema-storage/catalog_versions.feature` and rewritten to the new requirement.
- `store_apply`: schema-storage "A first write records the predicate", "A failed write records no predicate",
  "Concurrent first writes both commit" (`schema_storage.feature`), "The catalog survives a restart", "A removed
  declaration stays removed after a restart", "No catalog key is written to a partition" (`catalog_versions.feature`);
  schema-changes Drops (all six), "A tenant's data drop removes its split parts", "Adding an index rebuilds it from
  existing data" (four of five), "Type changes" (all five, "A contradiction written before the change takes effect
  refuses it" new), "A failed rebuild restores the previous schema and its indexes", in the spec's texts. Steps in the
  new `tests/schema_storage_steps.rs`.
- `transactions_commit`: commit-rounds "Catalog changes, drops and index builds are ordered in rounds" (all nine, four
  new), schema-storage "Recovery asks again for a default entry" (`features/schema-storage/recovery_defaults.feature`),
  schema-changes "Each partition swaps on its own", "Two partitions switch at the same epoch", "A partition holds the
  version before it stamps its epoch" (`features/schema-changes/epoch_boundary.feature`). Steps in the new
  `tests/catalog_steps.rs`.

Every scenario binds against `TestCatalogSource`: the design's crate table gives no store or transactions crate a
dependency on `ultragraphdb_cluster_metadata` (only the partition and role crates sit above it), so the double stays,
and the group's half of a scenario ("the metadata group holds …") is the double's here, the metadata agent's crate's in
a cluster. In the drop scenarios the test commits the initial catalog as the frontend would.

Final Bazel: `//ultragraphdb_core/... //ultragraphdb_store/... //ultragraphdb_transactions/... //ultragraphdb_traits/...
//ultragraphdb_tool/...` without the integration tag, 73 targets, all pass (`workload_differential` among them):
`core_catalog` unit 31 and BDD 4 scenarios; `traits_catalog` unit 16; `store_apply` unit 230 and BDD 89 scenarios;
`transactions_txn` unit 97; `transactions_commit` unit 217 and BDD 68 scenarios. `bazel build --keep_going //...` fails
only on other agents' in-flight targets (`client_core` against wire's new role commands, `error_protocol`'s tests).

## Phases

Phase 1 surfaces with bare `unimplemented!()`: `traits_catalog` (whole crate), `CatalogContents`,
`CatalogError::{corrupted_version, too_large}`, `check_lengths`, `Catalog::{version, with_version}`,
`ChangesetCore::{load_catalog, execute_catalog}`, `CatalogExecution`. Phase 2 observed red: `traits_catalog` 15 of 16
failed `not implemented`; `core_catalog` 19 of 31. Deviation: the `store_apply` tests of the version API were written
after its stubs but run only once implemented, and the txn and commit changes (the refusal, `Prepared::catalog`,
`Pipeline::install`, the replay of build records) were written with their tests, without a red run.

Fuzz: `fuzz/ultragraphdb_core_catalog`, target `catalog_contents_decode` (decoded or refused as a corrupted version
naming what was asked, never a panic; accepted bytes re-encode to themselves), seeded from
`testdata/fuzz_seeds/catalog_contents_decode/` (10 seeds written by `testdata/seeds.py` from the format's text alone),
tied to the encoder by `test_every_fuzz_seed_decodes_as_its_name_says`. Built with `cargo +nightly fuzz build`, seeds
replayed with `-runs=0`; a 20-second sanity run in the scratchpad found nothing.

## Choices to review

1. **Every version taking effect is a `StartBuild(version)` record**, also when it starts no build in the partition:
   replay must execute every version at its place in the log (its removals, its refusals, the catalog the next rounds
   derive under), and record kind 3 is the only logical record naming a version. A record code of its own ("install a
   catalog version") would be clearer; it is the wire's and `PROTOCOL.md`'s.
2. **`CompleteBuild`'s argument is still the store's build record number**, as group 4 wrote it, not the catalog version
   §12.2 names. Replay ends whichever build runs, and replays an undo as a swap when the backfill now succeeds (the
   restoring version follows either way).
3. **A version waits while a build runs**, refused with `Rebuilding`; the host proposes it again after the build's end.
   Until then the tenant's transactions, executed against the installed version, abort at the version's epoch with a
   catalog conflict. Two builds in one version: the second declaration is refused like a contradicted one.
4. **The host drives the catalog path**: `Pipeline::install` is called once the close announcing a version is
   delivered; a conflict (a commit changed what the checks read) is answered under the host's ticket, and the host
   proposes the version again. No automatic install from a `CatalogSource` inside the pipeline.
5. **Default entries after a crash** are asked again only for replayed rounds, as the spec says: a round the store had
   applied before the process died, its command unsent, is not replayed, so its record can be missing in the group.
   The partition's behaviour does not depend on it (an undeclared predicate behaves as the default).
6. **A version's removals** undeclare predicates only; entity types and relations a version no longer names stay.
7. **`recover_builds` rebuilds every declared index a record names**, complete ones included, and queries read the
   declaration without those indexes until the swap.
8. **`TxnError::code()`** maps every refusal but past-limit (`PastLimit`), too large (`TooLarge`) and cross-partition
   (`CrossPartition`) to `ErrorCode::Refused`.
9. **Left in place, needing the user to delete**: `core_catalog`'s `catalog_store.rs` (`Catalog::load`, `entries`,
   `removal`, `stored_entry`) and its tests, the stored catalog no partition writes; `store_apply`'s physical replay
   (`prepare_replay`, `RecordWrites`' catalog section), which the commit crate does not call and which would write
   catalog keys if fed them.
10. Coverage was not measured with `cargo llvm-cov`.

## Mutation testing

`scripts/mutants.sh <crate> <file> --diff cabae5ec`, one run each over 23 files (`core_catalog` 7, `traits_catalog` 3,
`store_apply` 6, `transactions_txn` 4, `transactions_commit` 3), then one fix pass and one `--re` run for three files.
Fix pass (tests in `store_apply`'s `changeset_core_catalog_tests.rs`): a store failing during a version's checks fails
the version whole (`refuses_declaration` → `true`, 2 survivors); `build_record` names the running build's record (3);
`Executed::catalog_version` is the installing version's number and `None` for a write (3). The `--re` run caught all
ten. Several files' mutants were unviable only (`catalog_error/mod.rs`, `write_set_builder/mod.rs`, `txn_error/mod.rs`:
the changed lines are constructors and matches a mutant cannot compile against).

Survivors, for task 12.1:

- `core_catalog/src/utils/catalog_codec.rs` (3): `strings`' `len() > STRING_MAX` (`→ Ok(())`, `>=`, `==`), the
  4 GiB string bound, which a test can reach only with a string of 2^32 bytes (tests stay small).

## Record codes (decision 1+2), finished by the coordinator

`PROTOCOL.md` §12.2 (the catalog agent's edit) and the code now agree: `RecordOperation::CatalogTakesEffect(version)`
is operation 6, and `CompleteBuild { version, outcome }` operation 7, the version the one in effect while the build
ran (a later version waits for a build) and `BuildOutcome` (`Swap` 1, `Undo` 2) at argument byte 8, bytes 9–15 zero.
The write-set builder writes the outcome; replay ends a build as its record says, an undo again an undo. Tests: every
code and argument, a refused outcome byte, trailing bytes, a txn build end naming the record's catalog version. Mutants
deferred to 12.1.

## Decision 9, open

`store_apply`'s physical replay no longer has a catalog section (`RecordWrites` holds none). `core_catalog`'s
`catalog_store.rs` stays: `store_quads`' Describe (`data_dictionary_describe.rs`) still reads the catalog with
`Catalog::load`, which no partition writes any more, so it reads the initial catalog. Moving Describe onto the
installed in-memory catalog, and then deleting `catalog_store.rs`, is the remaining work (decided 2026-11-03).
