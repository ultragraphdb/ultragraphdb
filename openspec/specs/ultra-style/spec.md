# ultra-style Specification

## Purpose
Ultra Style as the workspace holds it: the one lint table of `ULTRA_STYLE.md` in `[workspace.lints]`, every touched
crate conforming before its behaviour changes, the rules no lint checks reviewed and recorded in the group's notes,
Bazel as the test gate and line coverage measured per added crate, so every crate meets one standard of safety.

## Requirements
### Requirement: The Ultra lint set is the workspace's one lint table
The Ultra lint set of `openspec/protocols/ULTRA_STYLE.md` SHALL be the workspace's one lint table,
`[workspace.lints]` in the root `Cargo.toml`, taken by every crate with `[lints] workspace = true`, with
`too-many-lines-threshold = 70` in `clippy.toml`. Clippy SHALL run over the workspace through `make fix`, and
formatting through `make format`; a crate conforms when both leave it unchanged and `make fix` reports no warning
and no error for it. A lint SHALL be silenced only with `#[expect(lint, reason = "...")]` on the narrowest item, and
only with the user's consent. `clippy::unimplemented` SHALL warn, not deny, so a phase-1 surface of bare
`unimplemented!()` bodies compiles; once phase 4 is done, no such warning SHALL remain.

#### Scenario: A crate fails on a new violation
- **WHEN** a function of a crate grows to 71 lines, or an `unwrap` is added under its `src/`
- **THEN** `make fix` fails naming the lint

#### Scenario: A stale exception surfaces
- **WHEN** an `#[expect]` names a lint that no longer fires on its item
- **THEN** `make fix` warns that the expectation is unfulfilled

### Requirement: Every crate the change touches conforms before its behaviour changes
A crate this change modifies SHALL first conform to the lint table and to the rules no lint checks, with its behaviour
unchanged and its suite green under Bazel, before any of its behaviour changes; the lift SHALL be its own commit. Every
crate this change creates SHALL conform from its first commit. A rule a crate cannot follow SHALL be broken visibly,
with `#[expect]` or a comment beginning `Exception (Ultra rule N):`, and listed in the crate's notes.

#### Scenario: A lift changes no behaviour
- **WHEN** a crate is lifted into Ultra Style
- **THEN** its suite passes under Bazel before and after the lift with the same test count

#### Scenario: A new crate starts conformant
- **WHEN** a crate created by this change is first built
- **THEN** its manifest takes the workspace's lint table, and `make fix` reports nothing for it

### Requirement: Bazel is the test gate and coverage is measured per added crate
Bazel SHALL be the one gate for tests: a group SHALL be done when `make test` passes. Every crate to which this change
adds source files SHALL be measured with `cargo llvm-cov -p <crate> --lcov`, and every line of the files it adds SHALL be
covered, as `openspec/protocols/tdd-protocol.md`'s phase 4 asks; a line left uncovered SHALL be covered by a test or
listed with its reason in the group's notes.

#### Scenario: An uncovered branch is found
- **WHEN** the coverage report of a crate shows an error branch of an added file that no test reaches
- **THEN** a test reaching it is added before the group closes, or the line is listed with its reason

### Requirement: Conformance covers the rules no lint checks
A lifted or created crate SHALL also meet, by review recorded in its group's notes, the Ultra rules no lint enforces: an
acyclic call graph with bounded explicit stacks for recursive structures (rule 1), a named bound on every loop, queue,
retry and wait (rule 2), data-plane memory allocated at startup (rule 3), every invariant whose violation would corrupt
what is stored or sent checked as a returned invariant error (rule 5), and a justification for every `Arc`, interior
mutability and `cfg` (rules 8, 9).

#### Scenario: An unbounded loop is found in review
- **WHEN** the review of a lifted crate finds a `loop` with no counted bound that is not an event loop
- **THEN** the loop gains a named maximum and an error past it, with a test at the bound and one past it, before the
  crate's behaviour changes

