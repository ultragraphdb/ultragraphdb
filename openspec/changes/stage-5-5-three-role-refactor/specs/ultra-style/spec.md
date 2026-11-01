## ADDED Requirements

### Requirement: The Ultra lint set is declared once and adopted per crate
The Ultra lint set of `openspec/protocols/ULTRA_STYLE.md` SHALL be declared in each adopting crate's manifest, with
`too-many-lines-threshold = 70` in `clippy.toml`, and Clippy SHALL run over every adopting crate with `-D warnings`
under Cargo (`make check`) and under Bazel's lint configuration alike. A lint SHALL be silenced only with
`#[expect(lint, reason = "...")]` on the narrowest item.

#### Scenario: An adopting crate fails on a new warning
- **WHEN** a function of an adopting crate grows to 71 lines, or an `unwrap` is added under its `src/`
- **THEN** `make check` and the Bazel build both fail naming the lint

#### Scenario: A stale exception surfaces
- **WHEN** an `#[expect]` names a lint that no longer fires on its item
- **THEN** the build warns that the expectation is unfulfilled, and `make check` fails

### Requirement: Every crate the change touches conforms before its behaviour changes
A crate this change modifies SHALL first adopt the Ultra lint set and build with zero warnings under it, with its
behaviour unchanged and its suite green, before any of its behaviour changes; the lift SHALL be its own commit. Every
crate this change creates SHALL adopt the lint set from its first commit. A rule a crate cannot follow SHALL be broken
visibly, with `#[expect]` or a comment beginning `Exception (Ultra rule N):`, and listed in the crate's notes.

#### Scenario: A lift changes no behaviour
- **WHEN** a crate is lifted into the Ultra lint set
- **THEN** its suite passes before and after the lift with the same test count, under Cargo and Bazel

#### Scenario: A new crate starts conformant
- **WHEN** a crate created by this change is first built
- **THEN** its manifest declares the Ultra lint set and Clippy reports no warning

### Requirement: Conformance covers the rules no lint checks
A lifted or created crate SHALL also meet, by review recorded in its group's notes, the Ultra rules no lint enforces: an
acyclic call graph with bounded explicit stacks for recursive structures (rule 1), a named bound on every loop, queue,
retry and wait (rule 2), data-plane memory allocated at startup (rule 3), at least two checks per function on average,
paired and in both spaces (rule 5), and a justification for every `Arc`, interior mutability and `cfg` (rules 8, 9).

#### Scenario: An unbounded loop is found in review
- **WHEN** the review of a lifted crate finds a `loop` with no counted bound that is not an event loop
- **THEN** the loop gains a named maximum and an error past it, with a test at the bound and one past it, before the
  crate's behaviour changes
