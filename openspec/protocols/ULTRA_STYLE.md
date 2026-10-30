# Ultra Style

The coding style of UltraGraphDB: how its Rust is written so that defects are prevented while the code is written,
caught by the compiler, the linter, the tests, the simulator and the fuzzers before it ships, and so that the database
runs robustly, reliably and fast at all times.

Status: first draft, 2026-10-30. It applies to the crates of task group 10, the server, and every crate after them;
the existing crates adopt it over time, as they are changed.

Sources: NASA/JPL's *The Power of Ten — Rules for Developing Safety Critical Code* (Holzmann, `P10.pdf`), written for
C, and TigerBeetle's *TigerStyle* (`TIGER_STYLE.md`), written for Zig. Ultra Style keeps their intent and rewrites
their means for Rust 2024: a language where the compiler already proves memory safety and exhaustive matching, where
memory is owned and freed by scope, and where a panic, an allocation or a bounds check can hide inside an ordinary
expression. It adds what this repository already practises: sans-I/O state machines, injected clocks, deterministic
simulation, the five-phase test protocol (`tdd-protocol.md`), fuzzing and mutation testing.

## Design goals

Correctness, performance, developer experience, in that order. All three matter, and the order decides the rare case
where they conflict. In most cases they do not: simple, bounded, explicit code is easier to prove correct, easier for
the CPU to run fast, and easier for the next reader to change. Finding that one design that serves all three is the
work; it is rarely the first draft.

A problem solved in design costs least, one solved in implementation costs more, one solved in production costs most.
UltraGraphDB therefore takes TigerBeetle's position on technical debt: a defect, an unbounded queue or an exponential
algorithm found during implementation is fixed then, before the code is called done.

## The Ultra Ten

Ten rules, each stated positively, each with its reason and the check that enforces it. Holzmann's test applies: a
rule that no tool can check is a wish, so every rule names its check; where no tool exists yet, the check is review,
and the rule says so.

### 1. Control flow is simple, explicit and acyclic

Every function is reached through an acyclic call graph: an algorithm over a recursive structure (a triple term, a
type hierarchy, a query plan) walks it with an explicit stack whose depth has a named bound, and refuses input past
the bound with an error. Control flow is `if`, `match`, `for`, `while` and `loop`, with early returns through `?` and
`let ... else`. Function pointers, `dyn` and callbacks stored for later are replaced by static dispatch (generics,
enums, `impl Trait`), so the call graph is visible to the reader and to tools.

*Why:* recursion makes stack use unbounded and depends on input the server does not control; a nesting attack on a
recursive decoder is a crash in any language. An acyclic call graph makes every execution's depth and stack use
provable.

*Check:* review, and the bound's test at the bound and one past it (as the wire's `MAX_TERM_DEPTH` has). Rust lints
only unconditional recursion (`unconditional_recursion`), so an acyclic call graph is a reviewer's check.

### 2. Every loop, queue, retry and wait has a bound

A loop over data iterates a collection or a range with a known end. A `while` or `loop` that is not an event loop
counts its iterations against a named maximum and returns an error past it. An event loop ends only through its
shutdown state, and says so in a comment. Every queue, table, pool and cache has a capacity, fixed at startup, and a
full one answers with an operating error (`Busy`, `StreamLimit`, `TooLarge`) rather than growing. Every retry has a
count, every wait a deadline, every iterator from `repeat`, `cycle` or `from_fn` a `take`.

```rust
for _ in 0..ROUNDS_PER_SEGMENT_MAX {
    let Some(round) = scan.next_round()? else {
        return Ok(scan.end());
    };
    rounds.check_chain(&round)?;
}
Err(LogError::Invariant("a segment holds at most ROUNDS_PER_SEGMENT_MAX rounds"))
```

*Why:* everything has a limit in reality; naming it turns a hang or a tail-latency spike into a fail-fast error with
a cause, and lets a back-of-the-envelope sketch bound memory and time.

*Check:* `clippy::infinite_iter`, `clippy::maybe_infinite_iter`; review of every `while` and `loop`; a test at each
limit and past it.

### 3. Data-plane memory is allocated at startup

The data plane — framing, the session layer's request handling, the commit step, the log, reads and writes of the
store — runs on memory allocated when the node starts: buffer pools (`BufferPool`, after TigerBeetle's
`MessagePool`), vectors created `with_capacity` their bound and reused with `clear`, fixed arrays, and tables sized
from configuration. A push checks the bound first, so a vector never reallocates on the data plane. The control plane
— startup, configuration, catalog changes, administration — allocates freely, once per event.

```rust
if self.pending.len() < self.pending_max {
    self.pending.push_back(request);
    Ok(())
} else {
    Err(SessionError::Busy())
}
```

*Why:* Rust frees memory correctly, so the C reasons for Holzmann's rule 3 (use after free, leaks) are the compiler's
now; the reasons that remain are predictability and capacity. An allocator call on the hot path has unbounded
latency, fragments the heap under load, and turns overload into an out-of-memory abort, which Rust cannot recover
from. Memory sized at startup makes overload an error the client can retry and makes the node's footprint a number an
operator can plan with. Dependencies that allocate internally (fjall, oxrdf) are called on bounded inputs.

*Check:* review against the plane each function belongs to. A test cannot count allocations without `unsafe` (a
global allocator), so the rule is a reviewer's check.

### 4. A function fits on one screen: at most 70 lines

At most 70 lines as `rustfmt` formats it, comments and blank lines excluded. Control flow is centralised in the
parent: the parent holds the `if`s and `match`es and the state, helpers compute without branching on the caller's
cases and return what should change rather than changing it — "push `if`s up and `for`s down". Leaf functions are
pure. A good function is an inverted hourglass: few parameters, a simple return type, the work in between.

*Why:* a unit that fits on a screen can be understood and verified as a unit; one that scrolls hides its branches.

*Check:* `clippy::too_many_lines` with `too-many-lines-threshold = 70` in `clippy.toml`.

### 5. Assertions: two per function, paired, in both spaces

Every function checks what it relies on: its arguments, its preconditions, its postconditions and the invariants of
the state it changes, at an average of at least two checks per function. A check is one of three kinds, by what a
failure means (see *Invariants and errors*):

- **Compile time**, for relationships of constants and type sizes: `const _: () = assert!(HEADER_SIZE % 8 == 0);`.
- **Release**, for every invariant whose violation would corrupt what is stored or sent: an ordinary check returning
  an invariant error, through `invariant(holds, name)?`.
- **Debug**, for checks too costly for release: `debug_assert!`, run by every test, simulation and fuzz target.

Checks are **paired**: a property is checked on two paths, as a record is checked before it is written to the log and
again when it is read back. Checks cover the **positive and the negative space**: what must hold and what must not,
because bugs live where data crosses between them. A compound check is split, `debug_assert!(a); debug_assert!(b);`,
so a failure names the condition; an implication is `if a { debug_assert!(b); }`. A check is side-effect free, and one
that can never fail, such as `debug_assert!(true)`, does not count.

*Why:* tests find a defect per 10 to 100 lines (Holzmann), and assertions catch defects tests never reach; they turn a
correctness bug into a stopped component, and they multiply what the simulator and the fuzzers find, since a fuzzer
sees only what fails loudly.

*Check:* review, which counts the density; the phase-3 audit (`tdd-protocol.md`) plants defects that the checks must
catch; mutation testing removes checks and expects a test to fail.

### 6. Every value lives in the smallest scope, once

A variable is declared where it is first needed and dies where it is last used; blocks end scopes early. State is held
once: no copy or alias that can drift from the original, no field duplicated in two structs. A value is checked close
to where it is used, so no gap of time or code separates the check from the use (TigerStyle's POCPOU, a cousin of
TOCTOU). Shadowing is used only to refine the same value (`let len = u32::try_from(len)?;`), never to reuse a name for
another meaning.

*Why:* a value out of scope cannot be misused; a value held once cannot be stale.

*Check:* `clippy::shadow_unrelated`; review.

### 7. Every result is handled, and code under `src/` never panics

Every value computed is used: every `Result` is matched or propagated with `?`, and every other return value is read.
A value that is not needed is not computed, so a function returns only what its callers use, and a caller that has no
use for a result calls a function that does not produce it. Production code holds no `let _ =` and no `.ok()` that
drops an error. Operating errors — a malformed frame, a full queue, a lost connection, a failed sync — are values of
the crate's error type and are handled where they arise or returned. Code under `src/` reaches no `panic!`, `unwrap`,
`todo!` or `unimplemented!` on any input, and no implicit panic: indexing and slicing use `get` and
`split_at_checked`, iterators and `chunks_exact`; arithmetic on values from outside uses `checked_*`; division states
its rounding (see *Off-by-one*). An `expect` remains only where a local invariant proves it cannot fire, with the
invariant as its message.

*Why:* 92% of catastrophic failures in distributed data systems come from mishandled non-fatal errors (Yuan et al.,
OSDI 2014), and a discarded result is an error handled by not looking at it. A value computed and thrown away is work
on the hot path that buys nothing, and a sign that the function's shape and its callers' needs have drifted apart. A
panic in a server takes down every connection on the thread for the fault of one request.

*Check:* `unused_must_use` and `unused_results` (deny), `clippy::let_underscore_must_use`,
`clippy::let_underscore_untyped`, `clippy::unwrap_used`, `clippy::expect_used`,
`clippy::panic`, `clippy::todo`, `clippy::unimplemented` (phase 1's surface excepted), `clippy::indexing_slicing`,
`clippy::arithmetic_side_effects` in decoders and parsers.

### 8. Code says what it does: no repository macros, `cfg` only where the platform differs

Library code defines no macros (the repository's rule) and uses those of the standard library and its dependencies,
such as `tracing`'s. Conditional compilation is limited to the target platform (`target_os` for `O_DIRECT`,
`fallocate` and io_uring) and to the `test-utils` feature; each other `cfg` carries a comment justifying it.

*Why:* Holzmann's rule 8 counts 2^10 variants of a program with ten conditional directives, each needing its own test;
a macro hides control flow from the reader and from tools.

*Check:* review of every `cfg`.

### 9. Ownership over indirection

No `unsafe` (`unsafe_code = "forbid"`). Data is owned by one place and lent by reference; state moves between machines
by value, as the sans-I/O cores do. Shared ownership (`Arc`), interior mutability (`Cell`, `RefCell`, `Mutex`,
atomics) and trait objects each carry a comment justifying them, and nest at most one level: an `Arc<Mutex<T>>` is the
deepest shared indirection, never `Arc<Mutex<Vec<Box<dyn T>>>>`. A value over 16 bytes that is not meant to be copied
is passed by reference; a `Copy` type stays at 16 bytes or under.

*Why:* Holzmann's rule 9 restricts pointers because they hide data flow from readers and analysers; in Rust the
borrow checker removes the dangling pointer but not the hidden flow, which shared ownership and interior mutability
reintroduce.

*Check:* `unsafe_code = "forbid"`; `clippy::large_types_passed_by_value`, `clippy::trivially_copy_pass_by_ref`,
`clippy::needless_pass_by_value`, `clippy::rc_buffer`, `clippy::mutex_atomic`; review of every `Arc` and interior
mutability.

### 10. Zero warnings at the strictest setting, from the first line

Every crate builds with zero warnings from `rustc` and from Clippy with the Ultra lint set (*The lint set*). A lint is
silenced only with `#[expect(lint, reason = "...")]` at the narrowest item, never `#[allow]`: `expect` itself warns
once the lint stops firing, so a stale exception surfaces. Where a lint misreads the code, the code is rewritten until
it is trivially right, as Holzmann asks.

*Why:* a warning is a defect report a tool wrote for free; a codebase with warnings stops reading them.

*Check:* `clippy::allow_attributes`, `clippy::allow_attributes_without_reason`; Clippy with `-D warnings` over the Ultra
lint set.

## Invariants and errors

Two kinds of failure, kept apart because they take different responses:

| | Operating error | Invariant violation |
|---|---|---|
| Cause | the world: a client, a disk, a network, a full queue | a bug in UltraGraphDB |
| Expected | yes, and tested | no; tests, simulation and fuzzing exist to make it fire |
| Represented | a variant of the crate's error, with its protocol code | `InvariantError` naming the invariant |
| Response | handled: refused, retried, reported to the client; the node goes on | fail-stop: the component stops, then the node |

An invariant violation in a release build stops the component it was found in, before it writes anything more: the
commit step refuses writes as after a failed sync (code 17, Unavailable), the event is logged at error with the
invariant's name, and the node shuts down through its ordinary shutdown path, so recovery replays the log from a
known state on restart. That is TigerBeetle's crash, reached as Holzmann's rule 5 recovers, by returning an error
rather than by a panic, so the repository's rule holds that
`src/` never panics on any input: only a bug makes an invariant fail, and a bug stops the node rather than its
thread. In a debug build, the same check also panics through its `debug_assert!`, so a test, a simulation seed or a
fuzz input that breaks an invariant fails at once with a stack.

```rust
/// The check of an invariant: in debug builds a violation panics at the caller, for tests, simulation and fuzzing;
/// in release it returns the violation, which stops the component.
#[track_caller]
pub fn invariant(holds: bool, name: &'static str) -> Result<(), InvariantError> {
    debug_assert!(holds, "invariant violated: {name}");
    if holds {
        Ok(())
    } else {
        Err(InvariantError::Violated(name))
    }
}
```

`invariant()` and `InvariantError` live in `ultragraphdb_error_invariant`, an error crate of their own as every error
in the repository has, so a crate checking an invariant depends on that error alone. Each crate's error converts from
`InvariantError` into its own `Invariant` variant, so the caller's `?` carries it up.

Compile-time checks state what the design depends on, before the program runs:

```rust
const _: () = assert!(HEADER_SIZE == 256);
const _: () = assert!(MAX_WRITE_SET <= MAX_FRAME_SIZE - HEADER_SIZE);
const _: () = assert!(Superblock::SIZE == 72);
```

A blatantly true check may replace a comment where the property is critical and surprising: the check is a comment
the compiler or the tests read too.

## Types

Rust's types prove what C and Zig must assert. Use them first; an invariant the type system holds needs no check.

- **Make invalid states unrepresentable.** A connection's phase is an enum whose variants carry only the state that
  phase has; a builder's typestate refuses a frame without its header at compile time; `NonZero<u32>` holds a count
  of at least one.
- **Distinct meanings, distinct types.** An index, a count and a size are different quantities in the same integer,
  and confusing them is the classic off-by-one. Where they cross a module boundary, wrap them: `Position`, `Version`,
  `SessionId`, `StreamId`, `ByteCount`. Inside a function, name them with their unit (*Naming*).
- **Explicit widths.** Everything stored or sent has an explicit width (`u16`, `u32`, `u64`, `u128`); `usize` holds
  in-memory lengths and indices only, and crosses into a fixed width through `u32::try_from` or a checked bound.
  `as` converts only where the conversion is lossless by construction, and says so; `clippy::cast_*` lints flag the
  rest.
- **Exhaustive matches.** A `match` over the crate's own enum names every variant, no `_`, so a new variant fails to
  compile wherever it is not yet handled (`clippy::wildcard_enum_match_arm`).
- **`#[must_use]`** on every function whose result matters and is not a `Result`: a computed value ignored is a bug.
- **Simple signatures.** The return type a caller handles with the fewest branches wins: `()` over `bool`, `bool` over
  `u64`, `u64` over `Option<u64>`, `Option<u64>` over `Result<u64, E>`, because the extra dimensions propagate up the
  call chain.
- **No mixed-up arguments.** Rust has no named arguments: a function taking two arguments of one type takes newtypes
  or an options struct, so a swap does not compile. Dependencies that are singletons of their type (a clock, an
  executor, a pool) are passed positionally, most general first.

## Memory and ownership

- **Planes.** Each module states whether it is data plane or control plane (rule 3). Data-plane functions take their
  buffers from the caller and return them, as the framer reads into a `PooledBuffer` and hands it back.
- **Reuse, then clear.** A buffer is cleared (`Vec::clear` keeps its capacity) and reused, not dropped and rebuilt.
- **No buffer bleed.** Every byte written to the wire or the disk is written on purpose: padding and reserved bytes
  are zeroed, and a reused buffer's length is set to what was written, so no stale byte of an earlier frame or
  another client leaves the node. Receivers refuse nonzero padding (`PROTOCOL.md`, section 1).
- **Large values stay put.** A struct of more than a cache line is built once and changed in place through `&mut`
  (`reset`, `clear`), not rebuilt and moved: a Rust move is a `memcpy`, and a large one on the hot path is latency and
  stack. Its size is asserted at compile time where it matters.
- **Resource pairs read together.** The acquisition of a resource and the guard or `Drop` that releases it sit
  together, with a blank line before and after, so a leak shows as a broken pair.
- **No aliasing of state.** Two owners of copies of one fact drift; derive the second from the first when it is
  needed.

## Concurrency and asynchrony

- **The program runs at its own pace.** External events are queued and taken in batches at the program's pace, not
  acted on in the callback that announced them: the sans-I/O machines take a frame, return what to send and when they
  next need the time, and their shell does the I/O. The control flow stays in the program, work per period stays
  bounded, and batching amortises every cost.
- **Injected time, randomness and threads.** Storage and server code read the clock, draw randomness and spawn work
  through what they are given, so a simulation run is a function of its seed; `clippy.toml`'s `disallowed-methods`
  enforces it (`Instant::now`, `SystemTime::now`, `thread::spawn`, `RandomState::new`).
- **Share nothing on the data plane.** Each worker core owns its connections, buffers and caches; cores exchange
  messages over bounded channels; one writer owns each piece of shared state, as the commit step owns the order of
  commits. A lock on the data plane carries a comment on its bound: what it guards and for how long.
- **Atomics say why.** Every atomic operation names its `Ordering` and the comment says why that ordering suffices;
  `SeqCst` is chosen deliberately, not by default. A contended atomic sits alone on its cache line.
- **No precondition survives an `.await`.** A check made before a suspension point may be false after it, so a
  function's preconditions hold between awaits only, and are checked again after one. The logic lives in synchronous
  cores that run to completion; the `async` shells around them are thin.
- **Cancellation safety is stated.** Every `async fn` says what a caller dropping its future at each `.await` leaves
  behind (a partial frame read, a half-written buffer), and the type's state stays valid either way.

## Performance

- **Design for it.** The large wins come in design, when nothing can be measured yet. Every design sketches, on the
  back of an envelope, its use of the four resources — network, disk, memory, CPU — in their two dimensions, bandwidth
  and latency, and aims to land within 90% of the best.
- **Slowest resource first,** weighted by frequency: network, disk, memory, CPU, unless a faster resource is used so
  much more often that it dominates (a cache miss per key can cost what a sync per round costs).
- **Batch.** Amortise every fixed cost: frames per write, transactions per round, rounds per sync, keys per lookup.
- **Control plane and data plane apart.** The control plane checks generously; the data plane checks per batch, not
  per item, which keeps checks dense without making them costly.
- **Let the CPU sprint.** Give it long, predictable runs of the same work. A hot loop lives in a function of its own
  taking slices and primitives, not `&self`, so the compiler need not prove that fields stay in registers and the
  reader sees every input.
- **Bounds checks earned, not paid.** Iterators, `zip`, `chunks_exact` and `as_chunks` iterate without a bounds check
  and without a panic; a slice taken once at its checked length lets the compiler drop the checks inside the loop.
- **Be explicit.** Pass every option of a library call at the call site, never relying on its defaults
  (`..Default::default()` hides a choice that a dependency upgrade can change): a compio runtime's `single_issuer` and
  `defer_taskrun`, a fjall keyspace's caches, an atomic's ordering.
- **Measure what the design cannot settle.** A close choice gets a short benchmark (criterion) or a differential test,
  recorded with its result and the drive or core it ran on.

## Naming

- **Nouns and verbs exactly right.** A name says what a thing is or does in the domain's words; `pipeline` reads in a
  sentence where `preparing` does not.
- **Rust's casing.** `snake_case` for functions, variables and modules, `UpperCamelCase` for types with acronyms as
  words (`Xxh3`, `Uid`), as `clippy::upper_case_acronyms` expects.
- **No abbreviations,** except an integer index in a sort or matrix routine; long flags in scripts (`--force`).
- **Units and qualifiers last,** most significant first: `latency_ms_max`, not `max_latency_ms`, so `latency_ms_min`
  sits beside it. Limits follow it: `streams_max`, `credit_initial`.
- **Related names of equal length** line up: `source` and `target`, `source_offset` and `target_offset`.
- **A helper names its caller:** `read_frame` and `read_frame_body`.
- **One meaning per name** across the codebase: a word used in the protocol is not reused for another idea.
- **Order for the first read:** the important first. The repository's module layout (one type per module, trait
  implementations in files of their own) decides the rest.

## Comments and documentation

- **Say why.** Code says what; the comment says why this way and not the obvious other, and how a test reaches its
  goal. Every public item has documentation (`missing_docs`), with `# Errors` for each error it returns.
- **Sentences:** a capital letter and a full stop; a comment after code on the same line may be a phrase.
- **Commit messages** inform the reader of `git log` and `git blame`: what changed and why.

## Off-by-one and division

- **Index, count, size.** An index is 0-based, a count 1-based (`count = index + 1`), a size a count times its unit
  (`size = count * unit`). Each conversion is written out where it happens, and the types or names say which is which.
- **Ranges are half-open,** `start..end`, with `end` exclusive, everywhere in the codebase.
- **Division states its rounding:** `div_ceil` for a count of blocks, `checked_div` where the divisor comes from
  outside, `is_multiple_of` asserted where the division must be exact.

## Verification

Each layer catches what the one before it misses; none replaces understanding. Build a precise model of the code
first, write it down as types and checks, write the code and comments to explain the model to a reviewer, and let the
tools find the bugs in both.

| Layer | Catches | In this repository |
|---|---|---|
| Types | invalid states, unhandled variants, memory errors | rule 9 and the *Types* section |
| Compile-time checks | broken relationships of constants and sizes | `const _: () = assert!(...)` |
| Lints | the patterns of the Ultra Ten | *The lint set*, `make check` |
| Runtime checks | invariants in release | `invariant(...)`, fail-stop |
| Debug checks | invariants too costly for release | `debug_assert!` in every test build |
| Tests, test first | behaviour against the spec | the five phases of `tdd-protocol.md`, BDD scenarios |
| Deterministic simulation | interleavings, faults, crashes, from a seed | `ultragraphdb_store_sim`, `ultragraphdb_transactions_sim`, the simulated network |
| Fuzzing | inputs no test names | `fuzz/`, `scripts/fuzz.sh`, seeds from the golden values |
| Mutation testing | checks and tests that catch nothing | `scripts/mutants.sh` |
| Proof | the protocol's properties | `lean/RoundModel` |

A fuzzer or a simulator proves the presence of bugs, never their absence; they are the last line of defence, after
the reader's and the reviewer's understanding.

## The lint set

The Ultra lint set, in the crate's `Cargo.toml` `[lints]` or the workspace's, with `too-many-lines-threshold = 70`
in `clippy.toml`, run by `make check` with `-D warnings` in every crate that has adopted it:

| Lint | Level | Rule |
|---|---|---|
| `unsafe_code` | forbid | 9 |
| `unused_must_use`, `unused_results`, `missing_docs` | deny | 7, comments |
| `clippy::all` | deny | 10 |
| `clippy::allow_attributes`, `clippy::allow_attributes_without_reason` | deny | 10 |
| `clippy::too_many_lines` | deny | 4 |
| `clippy::unwrap_used`, `clippy::expect_used`, `clippy::panic`, `clippy::todo`, `clippy::unimplemented` | deny | 7 |
| `clippy::indexing_slicing` | deny | 7 |
| `clippy::arithmetic_side_effects` | deny in decoders and parsers: of wire, log and store bytes, of query text | 7 |
| `clippy::let_underscore_must_use`, `clippy::let_underscore_untyped` | deny | 7 |
| `clippy::cast_possible_truncation`, `clippy::cast_sign_loss`, `clippy::cast_possible_wrap`, `clippy::cast_lossless` | deny | types |
| `clippy::wildcard_enum_match_arm` | deny | types |
| `clippy::infinite_iter`, `clippy::maybe_infinite_iter` | deny | 2 |
| `clippy::shadow_unrelated` | deny | 6 |
| `clippy::large_types_passed_by_value`, `clippy::trivially_copy_pass_by_ref`, `clippy::needless_pass_by_value` | deny | 9 |
| `clippy::rc_buffer`, `clippy::mutex_atomic`, `clippy::await_holding_lock`, `clippy::await_holding_refcell_ref` | deny | 9, concurrency |
| `clippy::disallowed_methods` | deny | concurrency |

Phase 1 of the test protocol lands a surface of `unimplemented!()` bodies; those carry
`#[expect(clippy::unimplemented, reason = "phase 1 surface")]` until phase 4 replaces them.

The workspace's `[workspace.lints]` carries rule 7's lints (`unused_must_use`, `unused_results`,
`clippy::let_underscore_must_use`, `clippy::let_underscore_untyped`) at `warn` for every crate, so the existing crates'
warnings list what their adoption has left to change. A crate of group 10 onward builds with none of them.

## Formatting

`rustfmt` with `style_edition = "2024"`, its 100 columns and 4 spaces, unchanged. Comments wrap at 120 columns, as
the repository writes them today. Every `if` has braces, which Rust requires.

## Dependencies and tools

Every external crate is a cost in supply-chain risk, build time and behaviour the project does not control, so each
is pinned once in the workspace and justified in `AGENTS.md`'s table, pure Rust where one exists (no C toolchain per
target). A tool earns its place the same way: the toolbox stays small — Rust, Bazel, Cargo for what Bazel cannot
build (`fuzz/`), Python for golden-vector generators, Lean for proofs.

## Exceptions

A rule is broken only where following it would make the code less correct, and then visibly: an
`#[expect(lint, reason = "...")]` on the narrowest item, or a comment beginning `Exception (Ultra rule N):` with the
reason, so a search finds every exception and a review can revisit it.
