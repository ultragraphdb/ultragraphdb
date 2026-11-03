# unified-math-tdd-protocol Specification

## Purpose
TBD - created by archiving change unified-math-next. Update Purpose after archive.
## Requirements
### Requirement: Every stage runs the same five-phase cycle in order

Each of the four stages SHALL pass through phases 1 to 5 in order, and SHALL NOT begin a phase before the previous phase's exit condition is met.

The five phases are: (1) a compiling API with unimplemented bodies; (2) the full test suite, written
against that API and observed failing; (3) an audit of the suite against its own input variety;
(4) implementation, judged finished only by the audited suite; (5) hardening: mutation testing with
`scripts/mutants.sh` over the code the group added or edited, and fuzzing of every decoder of bytes
from a client, a peer or the disk.

The order is the point. A test written after the code it checks tends to encode what the code does
rather than what it should do, and a suite that has never been shown to reject a wrong implementation
has not been shown to do anything. This generalises
[`linear-test-first-development`](../../../specs/linear-test-first-development/spec.md), which applied
the same cycle to one crate, to a programme of four stages — including the stages that port or move
existing code, where the temptation to skip straight to phase 4 is strongest.

#### Scenario: A stage's phase order is auditable after the fact
- **WHEN** the commit history for any stage is read
- **THEN** the API-only commit precedes the suite commit, and the suite commit precedes the first implementing commit
- **AND** no test for a behaviour appears in a commit later than the one implementing that behaviour

#### Scenario: A ported stage does not skip phase 1
- **WHEN** a stage moves or ports existing code (C1's Meek closure, C2's absorbed statistics, C4's replacements)
- **THEN** the destination API is first declared with unimplemented bodies
- **AND** the existing implementation is not pasted into the destination before its suite exists and fails

### Requirement: Phase 1 produces a compiling API whose every body is unimplemented

Each stage SHALL first land its complete public surface — every type, trait, function, error variant and signature — with `unimplemented!()` bodies, and SHALL NOT contain a working algorithm at that point.

Designing the surface before the implementation forces it to answer to its callers rather than to
whatever the implementation found convenient. It also makes the suite compilable before anything
works, which phase 2 depends on.

For a stage that ports existing behaviour, the surface is derived from the call sites it must serve,
not transcribed from the source being replaced. C1's destination signature is an inherent method on
`MixedGraph<T>`, not the free function it comes from; C2's entropy signature carries the base and
zero-policy parameters the three sources disagree on, which none of the three has.

#### Scenario: The API-only stage compiles
- **WHEN** the stage's API-only commit is built under both `cargo build` and `bazel build`
- **THEN** compilation succeeds with no error
- **AND** every declared item is reachable from its crate root

#### Scenario: Nothing works yet
- **WHEN** any public function of the API-only surface is called
- **THEN** it panics as unimplemented rather than returning a value

#### Scenario: The surface is complete before the suite is written
- **WHEN** the suite is authored in phase 2
- **THEN** it compiles against the phase-1 surface with no addition to that surface

### Requirement: Phase 2 writes the full suite against the unimplemented API and observes it fail

The complete test suite SHALL be written before any algorithm, SHALL cover every scenario of the stage's capabilities, SHALL be run against the unimplemented API, and SHALL be observed failing for the intended reason.

Observing the failure is what separates a test that will catch a defect from one that passes
vacuously. A test that fails with a compile error, a missing import, or a panic raised somewhere
other than the function under test has not been shown to exercise anything.

The scenarios are the source the suite is written from, not a list to be reconciled against it
afterwards. A scenario with no test means phase 2 is unfinished — that is a property of the suite,
carried by the suite, and it needs no second artifact tracking it.

#### Scenario: Every test fails for the intended reason
- **WHEN** the suite runs against the unimplemented API
- **THEN** every test fails
- **AND** each failure is the unimplemented panic, not a compile error, a missing import, or a panic from elsewhere

#### Scenario: The failure run is recorded
- **WHEN** the phase-2 exit is claimed
- **THEN** the failing run's output is recorded in the stage's task notes, with the test count

#### Scenario: Every scenario of the stage has a test
- **WHEN** the phase-2 exit is claimed
- **THEN** each scenario of the stage's capabilities is exercised by at least one test in the suite

### Requirement: Expected values come from a source independent of the implementation

Every assertion SHALL compare against a value obtained independently of the code under test, and SHALL NOT compute its expectation through the same code path, the same formula, or a helper that shares either.

This is the anti-circularity rule, and it is the one that most often fails silently. A test that
checks `entropy(p)` by recomputing `-Σ p log p` in the test body asserts only that the expression was
typed the same way twice. A test that checks a ported function against the function it was ported
from asserts only that the port was faithful, including to its defects.

An acceptable independent source is one of: a closed form evaluated by hand and written as a literal;
a value published in a cited reference; a value computed by a demonstrably different algorithm (a
naïve O(n³) oracle against an optimised path); an algebraic invariant the result must satisfy
(idempotence, a conservation law, an inverse round trip); or a property that holds for all inputs in
a generated family.

#### Scenario: A tautological assertion is rejected in review
- **WHEN** a test's expected value is produced by calling the function under test, by re-implementing its formula inline, or by a helper that either calls it or shares its expression
- **THEN** the test is rejected and rewritten against an independent source

#### Scenario: A ported behaviour is pinned to the mathematics, not to its origin
- **WHEN** a stage ports existing behaviour
- **THEN** at least one test per ported function asserts against a closed form, a published value, or an invariant
- **AND** agreement with the origin implementation is recorded as a separate, additional test, never as the only one

#### Scenario: The independent source is named
- **WHEN** any test asserts a numeric literal
- **THEN** a comment names where the literal came from — the closed form, the citation, or the oracle

### Requirement: The suite covers corner cases enumerated in advance

Each stage SHALL enumerate its degenerate and boundary inputs before its suite is judged complete, and SHALL cover every enumerated case.

Enumerating in advance is what distinguishes a corner case that was considered from one that was
discovered through a defect. `../../AGENTS.md`'s own worked example is the topological-charge
normalisation that was eight times too small: the line ran 513 times, every test used the identity
gauge field where the field strength is zero, and `q == 0` held for any constant whatsoever.

The per-stage minimum is stated in that stage's own capability. Across all stages the following
classes SHALL be enumerated wherever the stage's inputs admit them: the empty input; the
single-element input; the input where two distinct quantities coincide (the identity, a diagonal, a
symmetric matrix, a tie); the input at which an index expression degenerates (a 2×2 where
`a[i*n+j]` and `a[i*n-j]` coincide at `j = 0`); a value at or across each documented threshold; a
value that is zero, negative, or exactly at a domain boundary; a non-finite input where the type
admits one; and, for every precision-generic path, the same case at `f32`, `f64` and `Float106`.

#### Scenario: The enumeration precedes the suite
- **WHEN** a stage's phase-2 exit is claimed
- **THEN** its corner-case enumeration is committed
- **AND** each enumerated case names the test that covers it

#### Scenario: A quantity is never pinned only where it vanishes
- **WHEN** a test asserts that a computed quantity is zero
- **THEN** the suite also asserts that quantity at an input where it is non-zero and known
- **AND** the non-zero expectation comes from an independent source

#### Scenario: A degenerate input never panics unexpectedly
- **WHEN** any enumerated degenerate input is passed to any public function
- **THEN** the call returns a value or a typed error, and does not panic, overflow, or return a silently substituted default

### Requirement: Every error variant is constructed by at least one test

Each stage SHALL exercise every variant of every error type it introduces or extends, through the public API, and SHALL assert the variant rather than only that an error occurred.

An error variant no test constructs is a claim about failure that nobody has checked. Asserting only
`is_err()` is the same omission one level down: it passes when the wrong error is returned, which is
exactly the case a caller matching on the variant will get wrong.

#### Scenario: Each variant has a constructing test
- **WHEN** an error enum introduced or extended by a stage is enumerated
- **THEN** each variant names a test that provokes it through the public API
- **AND** that test asserts the specific variant, not merely that the result is an error

#### Scenario: An unreachable variant is justified
- **WHEN** a variant cannot be provoked through the public API
- **THEN** either it is removed, or its unreachability is recorded with the argument that makes it unreachable

### Requirement: Phase 3 audits the suite's input variety before the suite is trusted

Each stage SHALL, before implementing, review its suite's inputs against the corner-case enumeration, so that no
behaviour is tested only where several distinct quantities happen to coincide.

Whether the suite rejects incorrect code is settled by mutation testing in phase 5: `scripts/mutants.sh` changes one
expression at a time in the real implementation and re-runs the affected crate's suite, which does what deliberate
defects planted by hand would do, mechanically and over every mutable expression rather than a chosen few.

#### Scenario: Input variety is audited
- **WHEN** the phase-3 exit is claimed
- **THEN** the suite's inputs are reviewed against the corner-case enumeration for coincidence — cases where several distinct quantities take the same value — and any such case is supplemented by one where they differ

### Requirement: Phase 4 implements against the audited suite and never edits it to pass

Implementation SHALL be judged complete by the audited suite turning green at the repository's coverage requirement, and the suite SHALL NOT be weakened to accommodate an implementation.

The repository's standing rules apply unchanged: full coverage of added or edited files, and a
failing test means the implementation or the API is wrong, never that the test is. A test's assertion
is weakened only when the API itself is agreed to have been specified wrongly, and that agreement is
recorded.

#### Scenario: Coverage meets the standing requirement
- **WHEN** coverage runs over a stage's added or edited files
- **THEN** every added file reports full line coverage, except lines recorded as demonstrably unreachable

#### Scenario: A failing test changes the code
- **WHEN** a test fails during implementation
- **THEN** the implementation or the API changes
- **AND** an assertion is weakened only alongside a recorded decision that the API was wrong

#### Scenario: Both build systems run the suite
- **WHEN** `cargo test` and `bazel test //...` are run for the stage
- **THEN** both execute the full suite, both pass, and the executed test counts agree

### Requirement: Phase 5 hardens the implementation with mutation testing

Each group SHALL run `scripts/mutants.sh <crate> --diff <base>` once per crate over every line it added or edited there, once its implementation passes, with `<base>` the commit the group started from, and SHALL run it at most three times per crate: one run over the diff, one fix pass, and one re-run of only the mutants that pass killed (`--re`). A survivor is resolved as a suite gap, killed by a test, or as a recorded equivalence; a survivor still open after the third run is recorded in the group's notes for the stage's final verification, and the group closes.

Mutation testing answers the question coverage cannot: whether a test would notice if a line were
wrong. It is scoped to the lines a group changed, through cargo-mutants' `--in-diff`, and bounded, because every
mutant costs a build plus a test run: the script refuses a fourth run of a file for one base, holds one lock so only
one run uses the machine at a time, and runs four jobs. Agents call the script, never `cargo mutants` directly, so the
bound holds.

Equivalent mutants belong in `.cargo/mutants.toml`, each carrying the measurement that settles it
rather than an assertion that it is fine. Those entries are regular expressions matched against text
full of regex metacharacters, and a wrong one fails silently in both directions — an unescaped `+`
or `*` matches nothing, an unescaped `||` matches everything. Each entry SHALL be escaped and
confirmed with the `comm` check the file carries, and SHALL match no more than it argues for.

#### Scenario: Mutation runs over the stage's own code
- **WHEN** a group's phase-5 exit is claimed
- **THEN** `scripts/mutants.sh --diff` has run over the lines that group added or edited, once per crate and at most three times per crate
- **AND** its report, with every survivor left open, is recorded in the group's task notes

#### Scenario: A survivor is a gap until argued otherwise
- **WHEN** a mutant survives
- **THEN** either a test is added that kills it, or an entry is added to `.cargo/mutants.toml` carrying the measurement that establishes the mutation cannot change behaviour

#### Scenario: An equivalence entry is verified
- **WHEN** an entry is added to `.cargo/mutants.toml`
- **THEN** its metacharacters are escaped, the `comm` check confirms it matches what it claims, and it is confirmed to match no additional mutants

### Requirement: Phase 5 fuzzes every decoder of bytes from a client, a peer or the disk

Each stage SHALL write a fuzz target for every decoder it added or edited that reads bytes from a client, a peer or the disk, SHALL seed it, SHALL run it with `scripts/fuzz.sh`, and SHALL commit every failing input as a regression case that the hermetic suite replays.

Fuzzing finds what mutation testing cannot. Mutation testing asks whether the suite notices a wrong
line; a decoder of untrusted or stored bytes fails on the input no test names: a length that overruns,
an offset past the end, a checksum over the wrong range. `cargo-fuzz` runs a target under libFuzzer,
steered by coverage, and stops at the first input that panics or fails one of the target's
assertions; `arbitrary` builds structure-aware inputs, a valid value from arbitrary fields that is
then mutated, which reach the checks behind a frame's outer validation that random bytes rarely pass.

Each fuzzed crate has a cargo-fuzz package at `fuzz/<crate>/`, its targets calling the crate's public
decoders. The packages form one workspace in `fuzz/`, built with the nightly toolchain
`scripts/fuzz.sh` names; the root workspace excludes it and Bazel never builds it, because
`libfuzzer-sys` compiles C++ and a fuzz run is neither hermetic nor finite. Its seeds and regression
cases live in the crate's own `testdata/`, which Bazel already declares, so every finding reaches the
build of record as a test. `scripts/fuzz.sh [crate [target]]` is shaped as `scripts/mutants.sh`: it
checks that `cargo-fuzz` is installed, runs each target for `FUZZ_SECONDS` (60 by default), and
reports executions, coverage and failing inputs.

A group that decodes bytes writes its targets and seeds in its phase 5 and builds them; the runs of
`scripts/fuzz.sh` happen once, in the stage's final verification, while `scripts/mutants.sh` runs in each group.

#### Scenario: A decoder of untrusted or stored bytes has a fuzz target
- **WHEN** a stage adds or edits code that decodes bytes from a client, a peer or the disk
- **THEN** a target in `fuzz/<crate>/` calls that decoder through the crate's public API
- **AND** its seeds are under the crate's `testdata/fuzz_seeds/<target>/`, taken from the golden values the suite already holds

#### Scenario: A target checks the decoder's properties
- **WHEN** a target runs an input
- **THEN** the decoder does not panic, and either accepts the input or refuses it with its typed error
- **AND** an accepted value re-encodes to the same bytes where the format has one encoding, and a checksummed format never accepts an input whose checksums fail

#### Scenario: The fuzz run is recorded
- **WHEN** a stage's phase-5 exit is claimed
- **THEN** `scripts/fuzz.sh` has run every target of the decoders the stage added or edited for its time budget
- **AND** its report of executions, coverage and failing inputs is recorded in the stage's task notes

#### Scenario: A failing input becomes a regression case
- **WHEN** a target finds an input that fails
- **THEN** the input is minimized with `cargo fuzz tmin`, the decoder is fixed, and the input is committed under the crate's `testdata/fuzz_regressions/<target>/`
- **AND** a unit test replays every case there through `ultragraphdb_tool_bdd::fuzz_regressions` under Cargo and Bazel

#### Scenario: Fuzzing stays outside the hermetic build
- **WHEN** the fuzz packages are built
- **THEN** they build in the `fuzz/` workspace alone, which the root workspace excludes and no Bazel target reads
- **AND** no shipped crate depends on `libfuzzer-sys` or `arbitrary`

### Requirement: Test layout and helpers follow the repository's structure

Every stage's suite SHALL mirror its source tree file for file, SHALL register each test module upward through its `mod.rs` chain, SHALL declare every test directory to Bazel, and SHALL place shared helpers under `src/utils_tests/` rather than inside `tests/`.

A test file absent from its `mod.rs` compiles and never runs; a directory absent from Bazel is
invisible to the primary gate. Both failures are silent — the suite appears to pass. A helper placed
inside `tests/` builds under Cargo and fails under Bazel, which cannot reach it; placed under `src`
it is library code, so the coverage requirement applies and it needs its own tests.

**An in-src `#[cfg(test)]` module is the third way to be invisible, and it is not a directory**, so
declaring test directories does not cover it. Such a module needs a `rust_test(crate = ...)` target
of its own; without one it runs under `cargo test` and not under `bazel test //...`. This was found
false in this programme rather than predicted: **no crate had such a target, and 75 tests across
eight crates had never been executed by the authoritative command** — among them the five that had
been added *because* mutation testing found a function unpinned. These modules cannot simply move to
`tests/`: each exercises a private or `pub(crate)` item, and relocating them would mean widening the
public API in order to test it.

#### Scenario: Structure mirrors the source
- **WHEN** a source file `src/a/b.rs` is added by a stage
- **THEN** its tests are at `tests/a/b_tests.rs`

#### Scenario: The executed count matches the authored count
- **WHEN** the suite runs
- **THEN** the executed test count matches the number of test functions in the tree under both build systems

#### Scenario: In-src test modules are declared to Bazel
- **WHEN** a crate carries an in-src `#[cfg(test)]` module
- **THEN** its `../../BUILD.bazel` declares a `rust_test` target on the library crate, and `bazel test //...` executes those tests

#### Scenario: Helpers are reachable and tested
- **WHEN** a stage adds a shared helper
- **THEN** it is under `src/utils_tests/`, `bazel test` resolves it from a clean output base, and it carries its own tests

