# UltraGraphDB Storage I/O in Longshore

Nov 17, 2026 · @Marvin Hansen

Stage 6.3 decides the I/O layer under the tile store's file path: the write-ahead log, the grid's extents and the
engine's reads. The candidate is longshore's `io_uring` ring (`../longshore`, co-maintained), extended with file
operations and aligned registered buffers; the alternative is compio, which carries every role's I/O today. Networking
stays on compio either way. The stage ends with a measured decision, experiment E9 on physical Linux, which stage 6.5
takes into its design before it builds the grid and the engine. This note states the decision to be made, what each
candidate offers now, and what an in-depth investigation establishes before the stage's full specifications are written.

**Status of each part.**

| Part | Status |
| --- | --- |
| compio for every role's I/O, networking and files | Built; pinned to compio's master at `d52b87b8` since 2026-11-17 for the fix of compio-rs/compio#1119 |
| longshore as the tile store's object binding (S3, GCS) | Decided; enters stage 6.5's specification at its rebase (task 0.7) |
| longshore's ring for the tile store's file path | This stage: to be investigated, built in longshore and decided by E9 |
| Networking on longshore | Not planned; reconsidered only on a measured gain at the roles' frame sizes (see *Beyond this stage*) |

## Why before stage 6.5

Stage 6.5 builds the tile store as sans-I/O state machines with an I/O shell (design D3: "Sans-I/O cores, compio
shells"). Its first I/O work is group 3, the durable seam: `DurableStorage`, `DurableFile` and `AlignedBuf` move into
`ultragraphdb_traits_durable`, with `CompioStorage` behind them and allocation and hole punching through rustix on
compio's blocking pool (tasks 3.2, 3.3; T10). Its tile-store note defers registered buffers to E9 (T18: "registered
buffers wait for E9 on physical Linux, and only a measured gain sends a patch upstream, since a custom operation in this
workspace would need `unsafe`"), and its task 8.4 runs E9 after the engine exists. A shell chosen after the engine is
built is chosen too late; this stage moves the choice and E9 ahead of group 3.

Stage 6.3 changes no code of this repository but a benchmark harness. Its work lands in longshore, so it runs beside
stage 5.9 and completes before stage 6.5's design is rebased.

## The two candidates for the file path

**compio** (pinned at master `d52b87b8`): owned buffers moved into each operation and returned with its result;
positional reads and writes, `fsync` and `fdatasync`, `ftruncate`, `statx`, `openat` with custom flags (O_DIRECT),
rename, unlink and mkdir on the ring; off `io_uring` (macOS kqueue, a container whose seccomp profile refuses
`io_uring`), file operations run on a thread pool. It has no `fallocate` (the log calls rustix on compio's blocking
pool) and no registered buffers: no `READ_FIXED` or `WRITE_FIXED`, no `register_buffers`. It exposes fixed files
(`Runtime::register_files`) and an `IO_LINK` flag at the driver level.

**longshore** (`crates/longshore_common`, about 3,000 lines, no dependencies): one `Io` per thread, owning every buffer
the kernel touches; callers hold `BufId`, `Sock` and `Token` and receive `Completion { token, result }` from `tick`.
Buffers are one pool, every buffer registered, buffer id equal to its registration index; sockets are registered fixed
files; the ring is set up `SINGLE_ISSUER | DEFER_TASKRUN` (Linux 6.1 and later). Its file support is a whole-file read
from offset 0 of a registered path. It has no file writes, no positional reads, no `fsync` or `fdatasync`, no
`fallocate`, no O_DIRECT, and its pool has byte alignment only. A seeded simulator (`sim`) models sockets, time and
whole-file reads, with no disk writes, syncs, crashes or torn writes; it is the only backend on macOS. longshore holds
33 `unsafe` sites, which this repository's crates cannot hold (`Cargo.toml`'s workspace lints, `AGENTS.md`): a raw
`io_uring` driver for ultragraphdb lives outside this workspace either way.

## Scope of the stage

The work in longshore, each piece useful to any I/O layer:

1. **File operations on the ring:** `openat` with flags (O_DIRECT, O_DSYNC as the investigation decides), positional
   reads and writes from registered buffers (`READ_FIXED`, `WRITE_FIXED`), `fsync` and `fdatasync`, `fallocate` with
   hole punching, `ftruncate`, `statx`, rename, unlink, mkdir, close; linked operations where ordering requires them.
2. **Aligned registered pools:** alignment to the device's logical block size (4 KiB at least), size classes, more than
   one registration range, sized at start-up from the tile store's settings.
3. **A file path off `io_uring`:** synchronous or on a thread pool, so the tile store's shells run real files in the
   Bazel tests on macOS and in a container that refuses `io_uring`.
4. **The simulator's file model**, or the boundary with this repository's `SimStorage`, which stage 6.5 extends with the
   fault classes of `STORAGE.md` section 19.1.

The work in this repository: a benchmark harness for E9, and a spike of the partition core driving both rings.

The stage ends when E9 is recorded and the decision is written into stage 6.5's design (D3, T10, T18) and tasks (3.2,
3.3, 5.3, 8.3, 8.4). On a measured gain, stage 6.5's shells are longshore's and its durable seam implements
`DurableStorage` over longshore; without one, stage 6.5 proceeds on compio as designed and longshore stays the object
binding.

## What the investigation establishes

The full specifications wait for these answers. Each states what to establish and what evidence settles it.

1. **Kernel and ring capabilities.** Which `io_uring` operations the file path needs and the kernel each requires
   (`SINGLE_ISSUER` and `DEFER_TASKRUN`, `FALLOCATE`, `FTRUNCATE`, `READ_FIXED` and `WRITE_FIXED`, linked `FSYNC`), and
   so the minimum kernel the storage role supports. Evidence: the kernel's `io_uring` opcode table and its version
   history, checked on the kernels of the supported distributions.

2. **Registered memory limits.** The most buffers and bytes one ring may register, the `RLIMIT_MEMLOCK` a storage
   process needs at the tile store's pool sizes, and what containers and Kubernetes grant by default (longshore's README
   records the locked-memory limit as the second container constraint). Evidence: the kernel's limits, a pod with the
   chart's security context, and the pool sizes the tile store's settings produce.

3. **O_DIRECT on the filesystems we support.** Alignment of offsets, lengths and buffers per filesystem (ext4, XFS) and
   per device's logical block size; what happens on a filesystem that refuses O_DIRECT (tmpfs, overlay in containers).
   Evidence: `statx`'s `STATX_DIOALIGN` results and runs on each.

4. **Durability semantics.** Whether the log and the grid sync with `fdatasync` after writes, with `RWF_DSYNC` per write,
   or with an O_DSYNC file; whether a linked write-then-sync chain gives the ordering `STORAGE.md` requires under a
   crash; how each choice behaves on a drive without power-loss protection, which no setting may skip a sync for.
   Evidence: `STORAGE.md`'s durability sections, the kernel's documented semantics, and crash tests against the
   simulator's fault classes (torn writes at 512-byte boundaries, lost and misdirected writes).

5. **Two rings on one core.** How the partition's core thread, which runs compio for its role connections, also drives
   longshore's ring for its files, within stage 6.5's turn (task 8.3: one I/O submission per turn). Candidates: compio
   awaiting the ring's eventfd as readable and the turn calling `tick`; longshore's ring polled inside compio's turn; a
   dedicated I/O thread per core. What `DEFER_TASKRUN` requires of the thread that enters the ring, and what each
   candidate adds to a turn's latency. Evidence: a spike in this repository measuring a turn's latency and CPU under
   both rings.

6. **The buffer model at the seam.** How the grid's and the engine's state machines name buffers (a `BufId` into a
   registered pool, or an owned `AlignedBuf` as today), what copies each implies on the write and read paths, and
   whether the log's `PooledBuffer` and `AlignedBuf` move into registered pools sized at start-up. Evidence: the tile
   store's I/O requests in `STORAGE.md` section 2 and the log's present buffer flow.

7. **macOS and the refused-`io_uring` path.** Whether the storage role must run on a container that refuses `io_uring`
   (the role-binaries specification requires the polling fallback for every role today) or may require a seccomp
   profile allowing it, as longshore's README describes; and how the off-ring file path is tested equal to the ring's.
   Evidence: the role-binaries specification, the user's decision on the fallback for storage, and differential tests
   of both paths.

8. **The `unsafe` boundary and its checks.** The safe API longshore exposes to this repository, and how longshore checks
   its `unsafe` (Miri where it applies, loom for cross-thread hand-offs, the simulator for completion orders). Evidence:
   longshore's present checks and the new operations' needs.

9. **The dependency.** How this repository takes longshore: a git dependency pinned by commit, as compio is, through
   rules_rs's crate universe and Cargo; and how a longshore release cadence fits this repository's update script.
   Evidence: the compio pin's mechanics (`MODULE.bazel`, `allow_git_cli = False`) applied to longshore's workspace.

10. **E9, the measurement that decides.** Workloads: the log's appends with their sync at round sizes; the grid's
    extent writes with O_DIRECT; reads of 4 KiB to 64 KiB at several queue depths per core, many in flight. Figures:
    operations and bytes per second, p50 and p99 latency, CPU per operation, at one pinned core and at all cores.
    Machines: physical Linux with NVMe, the same hardware for both candidates, three to five runs each. Baseline:
    compio at the pinned commit. The threshold that counts as a gain, set before the runs (for example, CPU per
    operation down by a quarter, or throughput up by a third at equal CPU, outside both runs' spread). Recorded as a
    dated file under `openspec/benches`.

11. **Effort and ownership.** The size of each piece in longshore and in this repository, and the order: file
    operations and pools first, then the off-ring path, then the shells. Evidence: the answers above.

## Beyond this stage

Networking stays on compio. Moving it to longshore would need, in longshore, a readiness backend (kqueue and epoll) for
macOS and for containers that refuse `io_uring`, Unix-domain sockets, socket options on ring-created sockets, and a
safe local executor with spawn and timers for the roles' async code (about 1,040 production lines). No measurement
shows the runtime limiting the roles today: the benchmark of 2026-11-15 places the limit in the virtual machines'
network handling with partition threads half idle. Networking is reconsidered when a request/response benchmark at
the roles' frame sizes on physical Linux shows a clear gain for the raw ring while role threads are CPU-bound.
