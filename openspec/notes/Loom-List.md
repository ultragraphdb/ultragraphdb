# Loom List

The types that need a Loom model under `tdd-protocol.md`, *A type sharing state across threads has a Loom model*,
found by an import sweep of every crate's `src/` on 2026-11-12, and their status. The rule covers atomics, locks and
condition variables shared across threads, and state passed between threads through `std::sync::mpsc` or threads a type
spawns. Simulators, test doubles and benchmark tools take a model once a test drives them from more than one thread.

## How the list was made

Every `.rs` file under a crate's `src/` was searched for the imports that share state between threads: std's atomics,
`Arc`, `Weak`, `Mutex`, `RwLock`, `Condvar`, `Barrier`, `Once`, `OnceLock`, `LazyLock` and `mpsc`; `thread::spawn`,
`thread::scope` and `thread::Builder`; `thread_local!`; and `parking_lot`, `crossbeam`, `futures::channel`,
`async_channel`, `flume`, `async_lock`, `event_listener` and tokio's or compio's sync modules. That gave 161 files in 35
crates. Each file was then tagged with the primitives it uses and sorted into the sections below. Nineteen of the 161
matched only the text `sync` inside compio's `AsyncRead` and `AsyncWrite` and use no shared state; they are listed at
the end so the count closes.

To repeat the sweep, from the repository root:

```sh
grep -rlE 'std::sync|sync::atomic|\bAtomic[A-Z][a-z0-9]*\b|\b(Mutex|RwLock|Condvar|Barrier|OnceLock|LazyLock)\b|thread::(spawn|scope|Builder)|thread_local!|parking_lot|crossbeam|futures::channel|async_channel|flume|async_lock|event_listener|tokio::sync' --include=*.rs */*/src
```

A model is shown to bite before it is trusted: the operation it guards, made non-atomic or reordered by a temporary
edit, fails it.

## Required: production types sharing state across threads

Ordered by the harm a race would do: a lock and a condition variable together first, then several locks in one type,
then a flag or a count.

| # | Crate | Type | File | Shared state | What the model checks | Status |
|---|---|---|---|---|---|---|
| 1 | `ultragraphdb_store_posting` | `Lists<S, C>` | `src/types/lists/mod.rs` (with `lists_commit.rs`, `lists_read.rs`, `lists_rollup.rs`; `CacheMiss` in `src/types/cache_miss/mod.rs` borrows its state) | `state: Mutex<State>`, `changed: Condvar`, `writer`, `rollups`, `rolling` mutexes | A reader waiting on `changed` is woken by the change it waits for, with no lost wakeup; the locks are taken in one order, so no two threads deadlock | To do |
| 2 | `ultragraphdb_store_apply` | `ChangesetCore<S, C, B>` | `src/types/changeset_core/mod.rs` (with `changeset_core_catalog.rs`, `changeset_core_drop.rs`, `changeset_core_rebuild.rs`) | `catalogs`, `apply`, `rebuild`, `vectors`, `ended` mutexes; `sequenced: AtomicBool` | Concurrent applies serialize through `apply`; a rebuild and an apply never interleave inside one changeset; `sequenced` is seen set by every thread after it is set; no lock-order deadlock | To do |
| 3 | `ultragraphdb_store_kv` | `FjallStore` and its `Shared` | `src/types/fjall_store/mod.rs` (with `fjall_store_store.rs`, `store_drops.rs`, `store_meta.rs`, `store_temporaries.rs`) | `writer`, `discard`, `view`, `temporaries` mutexes; `poisoned: AtomicBool`; `applied_ts: AtomicU64` | `applied_ts` never moves backwards and a reader never sees a timestamp ahead of the writes it covers; once `poisoned` is set every operation refuses; temporaries get distinct ids | To do |
| 4 | `ultragraphdb_store_posting` | `StackedLayers` | `src/types/stacked_layers/mod.rs` | `stack: Arc<RwLock<Stack>>` | Readers see a whole stack, never one a writer has half pushed or popped | To do |
| 5 | `ultragraphdb_protocol_transport` | `BufferPool`, `PooledBuffer` | `src/types/buffer_pool/mod.rs`, `src/types/pooled_buffer/mod.rs` | `Arc<Mutex<PoolState>>` | A buffer returned on one thread and taken on another is handed out once; the pool's count of buffers out matches the buffers held | To do |
| 6 | `ultragraphdb_store_kv` | `GcSettings` | `src/types/gc_settings.rs` | `discard_ts`, `undecodable: AtomicU64`; `collected: Mutex<Vec<Slice>>` | Concurrent `fetch_add`s on `undecodable` lose no count; a raised `discard_ts` is seen by the collector; `collected` keeps every slice pushed | To do |
| 7 | `ultragraphdb_store_quads` | `Dictionary<B>` | `src/types/dictionary/mod.rs` | `state: Mutex<DictionaryState<B>>` | Two threads interning one term get one id | To do |
| 8 | `ultragraphdb_store_quads` | `QuadStore<S, C, B>` | `src/types/quad_store/mod.rs` | `referrers: Mutex<ReferrerCache>` | The cache's bytes stay within its bound under concurrent use | To do |
| 9 | `ultragraphdb_store_quads` | `DataDictionary` | `src/types/data_dictionary/mod.rs` | `shapes: Mutex<Option<Arc<Shapes>>>` | A reader sees the old shapes or the new, never neither, while another thread replaces them | To do |
| 10 | `ultragraphdb_cluster_metadata` | `LeasedBlocks` | `src/types/leased_blocks/mod.rs` | `Arc<Mutex<BTreeMap<u16, VecDeque<SequenceBlock>>>>` | No block is leased to two takers; a block returned is leased again once | To do |
| 11 | `ultragraphdb_store_kv` | `TermCache` | `src/types/term_cache/mod.rs` | `generation: AtomicU64` | A lookup never answers from a generation older than one a writer has bumped past | To do |
| 12 | `ultragraphdb_core_tok` | `LANG_BASE_CACHE` | `src/utils/langbase.rs` | `static LazyLock<RwLock<HashMap<String, String>>>` | Concurrent first lookups of one language agree; a reader never sees a half-written entry | To do |
| 13 | `ultragraphdb_client_core` | `ResultStream` and the core's read tracker | `src/types/result_stream.rs`, `src/types/stream_state.rs`, `src/types/client_core/mod.rs` | `read_to_end: Arc<AtomicBool>` shared by the caller's handle and the core | A handle dropped on the caller's thread is seen dropped by the core, which then cancels the read once; a read marked read-to-end is seen so | To do |
| 14 | `ultragraphdb_common_role` | `OpenRequests`, `OpenFrame` | `src/types/open_requests.rs`, `src/types/open_frame.rs` | `open: Arc<AtomicUsize>` | The count returns to zero once every frame is dropped; a stop waiting on zero never sees zero while a frame is held | To do |
| 15 | `ultragraphdb_common_service` | `Stopping` | `src/types/stopping.rs` | `Arc<AtomicBool>` | A stop set on one thread is seen by every holder, and is never unset | To do |
| 16 | `ultragraphdb_common_service` | `Readiness` | `src/types/readiness/mod.rs`, `readiness_default.rs` | `ready`, `listening: Arc<AtomicBool>` | Ready is never reported before listening is | To do |
| 17 | `ultragraphdb_common_service` | `stop_flag`, `stop_with_parent`, `watch_parent` | `src/utils/run_service.rs` (read by `src/utils/fetch_settings.rs` and `ultragraphdb/ultragraphdb_config/src/service.rs`) | `Arc<AtomicBool>` set by a signal or by the parent watcher thread | A stop set by the watcher thread ends the loops that read it | To do |
| 18 | `ultragraphdb_supervisor` | `Signals` | `ultragraphdb/ultragraphdb_supervisor/src/service.rs` | `term`, `int: Arc<AtomicBool>` set from a signal | A signal's flag is seen by the supervising loop | To do |
| 19 | `ultragraphdb_common_role` | `ConnectionBound`, `HeldConnection` | `src/types/connection_bound.rs`, `src/types/held_connection.rs`, through `src/utils/sync.rs` | `held: Arc<AtomicUsize>` | Racing connections admit exactly the bound; a leaving connection frees its place; the count ends at zero | Done: `tests/loom/types/connection_bound_tests.rs` |

| 20 | `ultragraphdb_store_kv` | `Syncer` | `src/types/syncer/mod.rs` | `std::sync::mpsc` to a thread it spawns with `thread::Builder` | Every sync requested is performed and acknowledged once, in order; the thread ends when the store drops its sender | To do |
| 21 | `ultragraphdb_store_kv` | `ThreadExecutor` | `src/types/thread_executor/thread_executor_executor.rs` | `thread::scope`, `thread::spawn` | Every task handed to it runs once, and the executor returns only after every task has ended | To do |
| 22 | `ultragraphdb_cluster_partition` | `PartitionMailbox` | `src/types/partition_mailbox.rs` | `std::sync::mpsc` | Each message reaches the partition once, in the order each sender sent it; a closed mailbox refuses rather than loses | To do |
| 23 | `ultragraphdb_common_storage` | `PartitionCore`, `CoreChannels` | `src/types/partition_core/mod.rs` | `std::sync::mpsc` from the partition's host, beside `futures::channel::mpsc` to its sessions | Frames from the host are taken once and in order; the core ends when the host's sender is dropped | To do |
| 24 | `ultragraphdb_storage` | the storage service's threads | `ultragraphdb/ultragraphdb_storage/src/service.rs` | `std::sync::mpsc`, `futures::channel`, `thread::Builder` | Startup hands each partition thread its work once; a stop reaches every thread and each is joined | To do |
| 25 | `ultragraphdb_compute`, `ultragraphdb_frontend` | the services' core threads | `ultragraphdb/ultragraphdb_compute/src/service.rs`, `ultragraphdb/ultragraphdb_frontend/src/service.rs` | `thread::Builder` | A stop reaches every core thread and each is joined | To do |
| 26 | `ultragraphdb_core_algo` | `merge_sorted`, `merge_sorted_more_mem` | `src/uidlist.rs` | `thread::scope` over disjoint chunks | The merged list equals the sequential merge whichever thread finishes first | To do |

`src/utils/lock.rs` in `ultragraphdb_store_apply`, `ultragraphdb_store_kv`, `ultragraphdb_store_posting`,
`ultragraphdb_store_quads` and `ultragraphdb_store_sim` holds the poison-handling `lock` (and in store_posting `wait`)
helpers the types above call; their models cover them.

## Required once a test shares them: simulators, test doubles and tools

Types that never ship take a model once a test drives them from more than one thread; until then the status says
which. A simulator's `Store` is `Send + Sync`, so whether a test shares it is a fact to check, not one to assume.

| # | Crate | Type | File | Shared state | Status |
|---|---|---|---|---|---|
| 27 | `ultragraphdb_store_sim` | `SimStore` | `src/types/sim_store/mod.rs` (with `sim_store_store.rs`, `temporaries.rs`) | `shared: Arc<Mutex<Inner>>` | Not yet shared by a test |
| 28 | `ultragraphdb_store_sim` | `SimSnapshot` | `src/types/sim_snapshot/mod.rs` | `shared: Arc<Mutex<Inner>>` | Not yet shared by a test |
| 29 | `ultragraphdb_store_sim` | `SteppedExecutor` | `src/types/stepped_executor/mod.rs` | `shared: Arc<Mutex<Shared>>` | Not yet shared by a test |
| 30 | `ultragraphdb_store_sim` | `SimListCache<V>` | `src/types/sim_list_cache/mod.rs` | `shared: Mutex<Shared<V>>` | Not yet shared by a test |
| 31 | `ultragraphdb_store_sim` | `LogCapture` | `src/types/log_capture/mod.rs` | `events: Arc<Mutex<Vec<CapturedEvent>>>` | Not yet shared by a test |
| 32 | `ultragraphdb_store_sim` | `SimClock` | `src/types/sim_clock.rs` | `nanos: Arc<AtomicU64>` | Not yet shared by a test |
| 33 | `ultragraphdb_client_core` | `TestServer`, `TestAuthenticator`, `TestExchange` | `src/utils_test/test_server/mod.rs`, `src/utils_test/test_authenticator/mod.rs`, `src/utils_test/test_exchange/mod.rs` | `clock: Arc<AtomicU64>` | Not yet shared by a test |
| 34 | `ultragraphdb_traits_blocks` | `TestBlockSource` | `src/utils_test/test_block_source.rs` | `state: Arc<Mutex<State>>` | Not yet shared by a test |
| 35 | `ultragraphdb_traits_catalog` | `TestCatalogSource` | `src/utils_test/test_catalog_source.rs` | `state: Arc<Mutex<State>>` | Not yet shared by a test |
| 36 | `ultragraphdb_tool_bench` | `ReadCounts` | `src/types/read_counts.rs` | five `AtomicU64` counters | Not yet shared by a test |
| 37 | `ultragraphdb_tool_bench` | `TimedStore` | `src/types/timed_store/mod.rs` (with `timed_store_store.rs`) | `calls: Mutex<Vec<WriteCall>>`; `other_calls`, `other_nanos: AtomicU64` | Not yet shared by a test |
| 38 | `ultragraphdb_tool_bench` | run state of `apply_run`, `partitions_run`, `pipeline_run` | `src/utils/apply_run.rs`, `src/utils/partitions_run.rs`, `src/utils/pipeline_run.rs` | `applied`, `replied: Arc<AtomicU64>`; `stop: Arc<AtomicBool>`; `Shared { writers: AtomicUsize, last_close: AtomicU64 }` | Not yet shared by a test |
| 39 | `ultragraphdb_tool_bench` | run start and its parties, the bound run | `src/types/run_start/mod.rs`, `src/types/run_start_party/mod.rs`, `src/utils/bound_run.rs` | `std::sync::mpsc` | Not yet shared by a test |
| 40 | `ultragraphdb_tool_test` | `PartitionWorkload` | `src/types/partition_workload/mod.rs` | `std::sync::mpsc` | Not yet shared by a test |

## Required: static counters

A `static` atomic hands out distinct numbers by one `fetch_add`. These three serve tests, which the test harness runs
on several threads of one process, so a test already shares each. Loom's atomics have no `const` constructor, so a model
of these takes the counter through `loom::lazy_static!` or moves it into a value the model owns.

| # | Crate | Static | File | Status |
|---|---|---|---|---|
| 41 | `ultragraphdb_tests` | `DIRECTORIES: AtomicU32` | `ultragraphdb/ultragraphdb_tests/src/utils/harness.rs` | To do |
| 42 | `ultragraphdb_tool_test` | `NEXT: AtomicU64` in `TestDir` | `src/types/test_dir/mod.rs` | To do |
| 43 | `ultragraphdb_transport_tls` | `NEXT_CA: AtomicU64` in `TestPki` | `src/utils_test/test_pki.rs` | To do |

## Message passing through `futures::channel`

These files pass state between threads and tasks through `futures::channel`, which Loom cannot see into: its atomics are
not Loom's. Ordering races between the two ends of a `futures` channel, as between a client and a server, are the
simulators' and the ordering tests' to check.

| Crate | Files |
|---|---|
| `ultragraphdb_common_frontend` | `src/types/asked.rs`, `client_session/mod.rs`, `core_joins.rs`, `join_fan_out.rs`, `login_asked.rs`, `owner/mod.rs`, `owner/owner_answer.rs`, `owner/owner_catalog.rs`, `owner/owner_check.rs`, `owner/owner_syncing.rs`, `owner/owner_turn.rs`, `owner_handle.rs`, `route_sink.rs`; `src/utils/owner_thread.rs` (also `thread::Builder`); test doubles `src/utils_test/double_frame.rs`, `double_session.rs`, `owner_run.rs`, `role_double.rs` |
| `ultragraphdb_common_compute` | `src/types/compute_core/mod.rs`, `compute_session.rs`, `link.rs`, `open_read.rs`, `result_sink.rs`, `scan_stream.rs`; `src/utils/run_read.rs` |
| `ultragraphdb_common_config` | `src/types/config_command.rs`, `config_handle.rs`; `src/utils/store_owner.rs` |
| `ultragraphdb_common_storage` | `src/types/core_ends.rs`, `core_message.rs`, `refill_blocks.rs`, `storage_session.rs` |

## Immutable sharing

These files use `Arc` or a one-time static over data no thread mutates after it is built, or are the implementation
files of a type listed above, whose model covers them.

| Crate | Files |
|---|---|
| `ultragraphdb_store_kv` | `src/types/fjall_scan/mod.rs`, `fjall_snapshot/mod.rs`, `gc_filter/mod.rs`, `gc_filter_factory.rs`; `fjall_store/` impl files (row 3) |
| `ultragraphdb_store_posting` | `src/types/cached_list/mod.rs`, `list/mod.rs`, `list/list_read.rs`, `mutable_layer/mod.rs`; `lists/` impl files (row 1) |
| `ultragraphdb_store_quads` | `src/types/description/mod.rs`, `kernel_scan.rs`, `page.rs`, `page_walk.rs` |
| `ultragraphdb_store_apply` | `src/types/changeset_applier/mod.rs`; `changeset_core/` impl files (row 2) |
| `ultragraphdb_store_sim` | `src/types/store_model/mod.rs`, `model_reads.rs`, `model_recovery.rs`, `model_writes.rs`; `sim_store/` impl files (row 27) |
| `ultragraphdb_common_storage` | `src/types/open_scan/mod.rs` |
| `ultragraphdb_core_text` | `src/types/collator/mod.rs`, `src/types/tag/tag_compact.rs` |
| `ultragraphdb_core_types` | `src/s2/loop/mod.rs` |
| `ultragraphdb_transport_tls` | `src/types/tls_acceptor/mod.rs`, `tls_connector/mod.rs`, `tls_trust/mod.rs` |
| `ultragraphdb_client_core` | `src/utils_test/test_authenticator/test_authenticator_authenticator.rs` |
| `ultragraphdb_tool_bench` | `src/types/counted_snapshot/mod.rs`, `counted_snapshot_store_snapshot.rs`, `counting_cache/mod.rs`, `timed_store/timed_store_store.rs` (row 37), `src/utils/inbound_run.rs`, `rollup_run.rs`, `setup.rs` |
| `ultragraphdb_tool_test` | `src/types/changeset_workload/mod.rs` |

## No shared state

The text `sync` inside compio's `AsyncRead` and `AsyncWrite` matched in these, which share nothing between threads:
`ultragraphdb_client_compio` `src/traits/split_stream.rs`, `src/types/client/mod.rs`; `ultragraphdb_client_core`
`src/utils_test/test_exchange/test_exchange_server_mechanism.rs`; `ultragraphdb_common_role` `src/utils/serve.rs`;
`ultragraphdb_protocol_transport_compio` `carrier/carrier_async_read.rs`, `carrier_async_write.rs`,
`compio_sim_stream/compio_sim_stream_async_read.rs`, `compio_sim_stream_async_write.rs`,
`compio_tls_stream/mod.rs`, `compio_tls_stream_async_read.rs`, `compio_tls_stream_async_write.rs`,
`connection/mod.rs`, `tls_read_half.rs`, `tls_write_half.rs`; `ultragraphdb_store_kv`
`src/types/fjall_store/fjall_store_debug.rs`; `ultragraphdb_transactions_log`
`compio_file/compio_file_durable_file.rs`, `compio_storage/compio_storage_durable_storage.rs`;
`ultragraphdb_transport_http_client` `src/types/http_client.rs`; `ultragraphdb_transport_http_server`
`src/utils/serve_http.rs`.
