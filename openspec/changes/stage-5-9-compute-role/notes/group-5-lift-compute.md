# Stage 5.9, group 5, task 5.1 (first half): the Ultra Style lift of `ultragraphdb_common_compute`

Base commit `962b3a41`. This file records the lift of `ultragraphdb_common/ultragraphdb_common_compute` into
`openspec/protocols/ULTRA_STYLE.md`, a commit of its own that changes no behaviour, before group 5 changes the crate. The
crate was lifted once before, on 2026-11-16 (stage 5.8, task 6.1, `notes/group-6.md` of the archived change): this lift
takes what that one left and what changed since, the code of stage 5.8's task 8.3 in `link.rs` and `joining.rs` included.
`ultragraphdb_common_frontend` is lifted beside it by another step and is no part of this record. Counts are those Bazel's
test logs report.

## The audit against `openspec/protocols/ULTRA_STYLE.md`

Scope: all of `src/` at `962b3a41`, 56 files, 5,574 lines, 218 functions, and the test tree, 55 files. The crate holds no
`src/utils_test/`. Every `file:line` below is as at the base unless it says otherwise.

How it was measured, as stage 5.8's groups measured: every file of `src/` read in full against the rules, with the search
patterns of group 4's checklist (`dyn`, `fn(`, recursion, `loop`/`while`, unbounded collections, `assert!`, `let _`,
`.ok()`, casts, cells and shared pointers, `..=`, division, one-letter names); rule 4 by a script counting each function's
lines and by Clippy's `too_many_lines`; every `async fn`'s statement of what a dropped future leaves by a script, 54 of 54;
Clippy under Bazel through the aspect with the workspace's table, and again with the five lints Ultra Style states beyond
it (`-Dclippy::indexing_slicing`, `cast_possible_truncation`, `must_use_candidate`, `missing_errors_doc`,
`missing_panics_doc`).

| Rule | Result | Evidence |
|---|---|---|
| 1, acyclic control flow, static dispatch | **gap, closed** | `read_catalog`'s `lost` closure took `&dyn std::fmt::Display` (`types/compute_core/compute_core_catalog.rs:59`), added after the first lift and carrying no reason. Its three callers all pass a `TransportError`, so it takes `&TransportError`. No recursion, no stored callback elsewhere |
| 2, loops | conforms | every `loop` and `while` says what ends it, those of task 8.3 included (`closes`, `utils/joining.rs:236`; `read_frames`, `types/link.rs:236`); `read_catalog` counts to `CATALOG_FRAMES_MAX` |
| 2, queues and tables | **gap, closed in part** | no bound was stated for `ComputeCore`'s `addresses`, `links` and `dialling` maps and its `DialWaiters` (`types/compute_core/mod.rs:31`, `:46-48`), for `ReadPins::keeping` (`types/read_pins.rs:31`) and for the tasks `Link::watch` spawns (`types/link.rs:172`); each now says what bounds it. Left open: the lookups below |
| 2 and 3, a bound | left open | `ComputeCore::value_index` and `reverse_counted` hold a whole index list in memory (`subjects`, `listed`, `sums`, `named`; `compute_core_lookups.rs:193`, `:210`, `:240`, `:259`), as `PartitionRows::finish_gate` and `values` hold what their Scans answer: no named maximum, and one is a refusal the lookups do not make today, a behaviour change. Stage 7's query core is their caller |
| 3, planes | conforms | `src/lib.rs` states them (the first lift). The per-row copies of the data plane (`Page::add_entry`'s key, `Names::hold`) are the executor's design, which task 5.4 changes; see *Found wanting* for the one that is a defect |
| 4, 70 lines | **kept** | 0 of 218 above 70 at the base, the longest `reverse_parts` at 65. The exhaustive match of the *Types* row took it to 76, so the match is `object_at_home`, a function of its own, and `reverse_parts` is 68. `too_many_lines` clean |
| 5, invariants | **gap, closed in part** | the crate held one `debug_assert_eq!` and no compile-time check. Added, each a check its function relies on and nothing proves by type: `Frontier::less`'s two places are in the heap; `Page::add_row` follows an entry; `PartitionRows::open_pending` has a range to come; `RangeSpec::alone` is alone; `prefix_end`'s key sorts after its prefix; `const _: () = assert!(PinTarget::FRAMES >= 1)`. No run-time `assert!` under `src/`. Left: the crate returns no `InvariantError`, since a read's refusal is an `ErrorReply`, which has no `Invariant` variant; how the executor fails-stop is task 5.4's |
| 6, smallest scope, shadowing | **gap, closed** | two shadowings reused a name for another meaning, which `shadow_unrelated` does not see in a pattern: `Frontier::less` bound a heap place `a` and then the stream at it as `a` (`types/frontier.rs:72-74`), and `merge`'s `order` bound a partition's index and then its row (`utils/run_read.rs:460-463`). The places are `first` and `second`, what they hold `lower` and `upper` |
| 7, results | **gap, closed** | `let _index = table.iri(&term)` (`types/partition_rows/mod.rs:167`) dropped a value without saying why; it does now. Every other `let _`, `drop(…)` and `.ok()` says why or folds a refusal into an `Option` its caller reads, as the first lift recorded |
| 7, a value silently replaced | left open | `Epoch::new(…).unwrap_or(Epoch::ZERO)` (`compute_core_lookups.rs:45`, `utils/run_read.rs:317`) and `ReadAt::at_epoch`'s `map_or(0, …)` (`types/read_at.rs:23`) read an epoch above `Epoch::MAX` as epoch 0. Refusing it is a behaviour change |
| 8, macros, `cfg` | conforms | no `macro_rules!`, no `cfg` |
| 9, ownership | **gap, closed** | `RowView` derived `Copy` above one cache line: `const _: () = assert!(size_of::<RowView<'_>>() <= 64)` failed to compile. Nothing copied one implicitly, so it drops `Copy` and keeps `Clone`. Every `Rc` and cell says why (the first lift; `Found` in `joining.rs:91-92`) |
| 10, zero warnings | conforms | aspect clean with the workspace table before and after; the one `#[expect]` (`utils/clock.rs:9`) carries its reason |
| Types: simple signatures | **gap, closed** | three private functions returned more than their callers read: `ResultSink::fits_exactly` a `Result<bool, _>` that was never `Err` (`types/result_sink.rs:95`), now `bool`; `wants` an `Option<()>` (`partition_rows_terms.rs:58`), now `bool`; `PartitionRows::no_rows` a `Result` that was always `Ok` (`partition_rows_plan.rs:94`), removed, its three callers calling `end` |
| Types: exhaustive matches | **gap, closed** | four `_` arms over the wire's `RequestTerm` or a pair of options name every case now: `utils/run_read.rs:287`, `:464`; `partition_rows_terms.rs:245`; `types/frontier.rs:75`. `wildcard_enum_match_arm` was clean before, since it does not look inside an `Option` |
| Types: no mixed-up arguments | **gap, closed in part** | `ReadPins::add(link, tenant, principal, version)` took three `u64` in a row (`types/read_pins.rs:56`); it takes the `PinTarget` it built from them, whose fields its two callers name. Left open, private: `Page::add_row`'s two `Option<SystemId>`, `partitions_detail` and `cross_partition`'s two `u32`, `PartitionRows::chosen`'s `(u64, u64)`. Left open, public: `ReadTimes::new` (two `Duration`), `read_times_of` (two `Option<u64>`), `ComputeCore::catalog` (`tenant`, `version`) and `reverse_counted` (`tenant`, `count`): a newtype there is a signature the binary and `ultragraphdb_compute_tests` follow, and task 5.4 gives the executor its surface |
| Types: casts, `#[must_use]`, `# Errors` | conforms | every `as` says why it is lossless; `cast_possible_truncation`, `must_use_candidate`, `missing_errors_doc` clean over the library |
| Concurrency | conforms | 54 of 54 `async fn` say what a dropped future leaves; no atomic, no lock; the clock is read at `utils/clock.rs` under its `#[expect]`, and `hold_peers` takes its clock |
| Naming: no abbreviations | **gap, closed** | `a`, `b` (above; `partition_rows_frames.rs:114`, now `left`, `right`), `p` (`utils/run_read.rs:329`, now `member`), `cx` (`types/link.rs:268`, now `context`) |
| Naming: units and qualifiers last | **gap, closed in part** | three limits and two defaults did not say so: `RANGES_PER_SCAN`, `NAMES_PER_SCAN`, `NAME_BYTES` take `_MAX`, as `TEXTS_MAX` and `CATALOG_FRAMES_MAX` have it; `SCAN_STALL_MS`, `READ_RETAIN_MS` take `_DEFAULT`. The copy of `RANGES_PER_SCAN` in `tests/types/wanted_names_tests.rs` follows. Kept: `max_frame`, `initial_credit`, `deadline_ms`, the protocol's own field names ("one meaning per name") |
| Comments | **gap, closed** | five private functions had no documentation: `ResultSink::empty_batch`, `header`; `Names::want_text`, `want`; `with_texts` (`types/row_terms.rs:174`); `PinTarget::open` |
| Division | **gap, closed** | four divisions did not state their rounding: `ReadTimes::pin_after`, `pin_every`, `ScanStream::open`'s `answer_wait() / 2`, `Frontier::push`'s parent |
| Layout (`AGENTS.md`, one type per module) | **gap, closed in part** | four types sat in a file of functions or of another type's methods; each has its module: `Opened` (`types/opened.rs`) and `Asked` (`types/asked.rs`) out of `utils/run_read.rs`, `LookupPartition` (`types/compute_core/lookup_partition.rs`) out of `compute_core_lookups.rs`, `RowCells` (`types/partition_rows/row_cells.rs`) out of `partition_rows_terms.rs`. New files, no file moved. Left open: `types/row_terms.rs` holds `RowIds` and six free functions and no `RowTerms`; splitting it moves its tests, and no file moves in this lift |
| Tests | **gap, closed in part** | nine tests returned `()`: five in `tests/types/frontier_tests.rs`, one in `lookup_at_tests.rs`, three in `tests/utils/keys_tests.rs`; each returns `TestResult`. No `unwrap`, `expect`, `panic!`, `#[allow]` or `#[expect]` in the test tree. Left open: the doubles live under `tests/` (`tests/played`, `tests/played_reads`, `tests/apart`, `tests/utils/played_*.rs`) where `AGENTS.md` has them under `src/utils_test/`; a move, and task 5.2 brings the frontend's doubles beside them |

### Skipped, since stage 5.9 removes the code

Task 6.4 removes `ComputeSession`'s role session and `joining.rs`' asks, task 4.3 `Link` and `ComputeCore`'s own
connections. In them only what was cheap was lifted: `Link::watch`'s bound and `yield_once`'s `cx` in `types/link.rs`, and
the bounds of `ComputeCore`'s maps. Skipped there:

- `OpenStream` beside `Link` in `types/link.rs`, and `Found` and `Asker` in `utils/joining.rs`: three types in files not
  their own.
- `ComputeSession::reads` (`types/compute_session.rs:30`), a map with no stated bound; the connection's `max_streams` bounds it.
- `hold`'s `deadline / ASKS_PER_DEADLINE` (`utils/joining.rs:216`), a division that does not state its rounding, and
  `POLL`, `ASKS_PER_DEADLINE` and `LINK_BUFFERS`, limits not named as limits.
- `drop(client.request(join).await?)` (`utils/joining.rs:71`, `:333`), an answer dropped without a reason.

### Found wanting

One defect the audit found and did not fix, since the fix changes the page's representation on the read path and is no lift:

- **`Page::add_row` copies an object's encoding once per statement ID** (`types/page.rs:54-55`, called from `take_record`,
  `partition_rows_frames.rs:97`). An object record of a vector, whose encoding reaches about 262 KiB (`VECTOR_MAX`, 65,535 elements of four bytes), with
  the statement IDs that fit beside it in a frame of one mebibyte, about 98,000, becomes about 25 GiB of page from one
  frame: Ultra rule 3's bound on what one frame costs does not hold. Past 4 GiB, `PageRow`'s `u32` offsets saturate
  (`types/page.rs:57-59`, `unwrap_or(u32::MAX)`) and `row_at` reads such a row as absent. Holding the encoding once per
  record, the rows naming its range, bounds a page by its frame; it belongs with task 5.4, whose step bound (design D3) is
  one frame.

### The lift and its evidence

Local, and no behaviour changed: comments, documentation, names, five debug checks and one compile-time check, three
private signatures narrowed, one `dyn` and one `Copy` removed, four types given their modules, nine tests returning
`TestResult`. No public signature changed, so no dependant was touched. After the lift `src/` holds 60 files, 5,671
lines, 218 functions.

Files changed, under `ultragraphdb_common/ultragraphdb_common_compute/`, 28 with 180 lines added and 159 removed:
`src/types/compute_core/compute_core_catalog.rs`, `compute_core_lookups.rs`, `mod.rs`; `src/types/frontier.rs`, `link.rs`,
`mod.rs`, `names.rs`, `page.rs`, `pin_target.rs`, `range_spec.rs`, `read_pins.rs`, `read_times.rs`, `result_sink.rs`,
`row_terms.rs`, `row_view.rs`, `scan_stream.rs`; `src/types/partition_rows/mod.rs`, `partition_rows_fill.rs`,
`partition_rows_frames.rs`, `partition_rows_plan.rs`, `partition_rows_terms.rs`; `src/utils/keys.rs`, `read_times_of.rs`,
`run_read.rs`; `tests/types/frontier_tests.rs`, `lookup_at_tests.rs`, `wanted_names_tests.rs`; `tests/utils/keys_tests.rs`.

Files added, 87 lines: `src/types/asked.rs`, `src/types/opened.rs`, `src/types/compute_core/lookup_partition.rs`,
`src/types/partition_rows/row_cells.rs`. No file was moved or removed.

| Target | Before, at `962b3a41` | After the lift |
|---|---|---|
| `common_compute:ultragraphdb_common_compute_tests` | 179 passed | 179 passed |
| Clippy aspect, the workspace table, `common_compute/...` | clean | clean |
| Clippy aspect with the five lints | the library clean; three `indexing_slicing` in `tests/types/partition_rows/partition_rows_frames_tests.rs:160-162`, the aspect's limit stage 5.8 recorded (it reads rules_rust's `.clippy.toml`, not the repository's, which allows indexing in tests) | the same |
| `rustfmt --check`, the 32 files touched or added | | clean |
| `make build` | | passes |
| `ultragraphdb_compute_tests:bdd`, one invocation | | 90 scenarios, 180 steps passed |

The debug checks run in these targets, which Bazel builds with debug assertions; none fired.

## Notes for the commit

One commit, the lift alone: the 28 files changed and the 4 added above, and this file.

```
common_compute: Ultra Style lift (stage 5.9, task 5.1)

The audit of ultragraphdb_common_compute against ULTRA_STYLE.md, the second after stage 5.8's task 6.1, with no behaviour
changed. Closed: read_catalog's dyn closure takes a TransportError; RowView, above one cache line, drops Copy; two
shadowings that reused a name for another meaning and four one-letter names renamed; four wildcard arms name every
case, reverse_parts kept within 70 lines by object_at_home; fits_exactly and wants return bool, no_rows is removed;
ReadPins::add takes the PinTarget it built from three u64; limits and defaults named as such (RANGES_PER_SCAN_MAX,
NAMES_PER_SCAN_MAX, NAME_BYTES_MAX, SCAN_STALL_MS_DEFAULT, READ_RETAIN_MS_DEFAULT); five debug checks and one
compile-time check where a function relies on what no type proves; the bounds of ComputeCore's maps, ReadPins' pins
and Link's watches stated; four divisions state their rounding; Opened, Asked, LookupPartition and RowCells each in a
module of their own; nine tests return TestResult.

Left for the tasks that change the code: the lookups' unbounded lists, an epoch above Epoch::MAX read as 0, public
signatures taking two values of one type, row_terms.rs' layout, the doubles under tests/, and what tasks 4.3 and 6.4
remove. Found wanting: Page::add_row copies an encoding once per statement ID. The record is
openspec/changes/stage-5-9-compute-role/notes/group-5-lift-compute.md.

179 unit tests before and after; the clippy aspect clean; compute_tests' 90 scenarios pass.

Co-Authored-By: Claude Fable 5.2 <noreply@anthropic.com>
```
