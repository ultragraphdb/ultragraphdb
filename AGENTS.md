# ultragraphdb Codebase Guide for Agents

## Core Directives

Communication Style:

Direct and Unfiltered: Do not use polite fillers or "I'm sorry".
Just state the truth as it is.

Analytical: Use logic, historical precedent, and probability to back up your critiques.

Vigilant: Treat every prompt as a potential trap for cognitive bias or confirmation bias.

Tone: Be respectful regardless of agreement. It's okay to disagree, correct, and call out as long as it's respectful.

## Golden Rules

1) Never ever git commit. Prepare a commit message and ask the user to commit.
2) Never ever delete files or folders. Ask the user for permission.

file mv and git mv (preserves history) is generally authorized.

## Behavioral Guidelines

These guidelines favor correctness, clarity, and minimal diffs over speed. For trivial tasks, use judgment.

### 1. Think Before You Code

Do not silently guess.

Before making changes:

- State your assumptions clearly.
- If anything is ambiguous, ask instead of choosing one interpretation silently.
- If there are multiple valid approaches, briefly present the tradeoff.
- If the request seems mistaken, inefficient, or overcomplicated, say so.
- If a simpler solution exists, recommend it before implementing.
- If you are confused, stop and explain what is unclear.

- Do not act certain when you are uncertain.

### 2. Keep the Solution Simple

Solve the requested problem with the minimum necessary code.

- Do not add features that were not asked for.
- Do not introduce abstractions for one-time use.
- Do not add configurability, extensibility, or generalization unless requested.
- Do not add defensive error handling for unrealistic cases.
- Prefer simple, readable code over clever code.
- If the solution feels too large, step back and simplify it.

### 3. Stay Strictly Within Scope

Only change what the task requires.

- Do not refactor unrelated code.
- Do not rewrite comments, formatting, or naming unless necessary for the task.
- Match the existing style and conventions of the codebase.
- Do not fix neighboring issues unless the user asked.
- If you notice unrelated problems, mention them separately instead of changing them.

### 4. Make Surgical Diffs

- Touch as few files as possible.
- Change as little code as necessary.
- Avoid broad rewrites when a targeted fix is enough.
- Preserve existing structure unless changing it is required.
- Remove only the dead code, imports, or variables created by your own changes.
- Do not delete pre-existing unused code unless asked.

### 5. Work Toward Verifiable Outcomes

Turn requests into clear success criteria whenever possible.

- "Fix the bug" - reproduce it, fix it, then verify the fix
- "Add validation" -> add checks for invalid input and verify behavior
- "Refactor this" - preserve behavior and confirm tests still pass
- "Optimize this" -> improve performance without changing correctness

Prefer tests, existing checks, or concrete validation over verbal confidence.

### 6. Read Before You Write

- Read enough nearby code to understand how the target piece fits in.
- Identify the local conventions before introducing new patterns.
- Identify the levels of abstraction in the affected code, and land new code at the right one.
- If you find inconsistencies of abstraction, or cannot place new code, stop and say so.

### 7. Preserve Intent

- Preserve comments unless they are clearly outdated and directly affected by the task.
- Preserve behavior unless the requested change is meant to alter it.
- Preserve public interfaces unless changing them is necessary.
- Call out any intentional behavior change explicitly.

Do not make hidden product or design decisions on the user's behalf. Instead, ask.

### 8. Ask for Help at the Right Time

Pause and ask if the request is ambiguous in a way that affects implementation, the codebase contains
conflicting patterns, the correct behavior is unclear, or the task requires a product or architectural
decision.

### 9. Final Check Before You Finish

Confirm that the request was addressed, the change is no larger than necessary, unrelated code was not
modified, assumptions were surfaced, affected tests or checks were run, and the result matches the
requested scope. If something could not be verified, say that clearly.

## Project

ultragraphdb is a distributed graph database in Rust, ported from Dgraph (Apache-2.0,
`github.com/dgraph-io/dgraph`). It keeps Dgraph's DQL query language and leaves out its GraphQL layer.
Storage is redb, the cache is moka, and replication and partitioning are part of the database.

### Ported code and attribution

Dgraph is licensed under Apache-2.0 and carries no NOTICE file. A file ported from Dgraph:

- keeps the copyright line of the Dgraph file it comes from, for example
  `SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.`, beside this repository's own;
- states that it was changed and names its Dgraph source path, in its header.

Code written here from scratch carries this repository's header alone:

```rust
/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */
```

### Crates

Crates are grouped in folders named `ugdb_<group>/`, and every crate is a workspace member through the
group's glob in the root `Cargo.toml`.

| Crate               | Path                              | Depends on     | Provides                                                                                                                                                                                                                 |
|---------------------|-----------------------------------|----------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `common_version`    | `ugdb_common/common_version`      | —              | `PRODUCT_NAME`; the workspace's first crate                                                                                                                                                                              |
| `proto_dgraph`      | `ugdb_proto/proto_dgraph`         | —              | Dgraph's gRPC API, `api.proto`, generated unchanged: the server serves it and the client calls it. Built twice, see *Structure exemptions*                                                                               |
| `ultragraph_client` | `ugdb_client/ultragraph_client`   | `proto_dgraph` | `UltraGraphClient`, `UltraGraphError` and the transaction types, over `proto_dgraph`; it speaks to ultragraphdb and Go Dgraph alike. Behind `test-utils`: the `DgraphInstance` fixture, which serves the acceptance test and differential runs against Go Dgraph |

### Structure exemptions

`ugdb_proto/proto_dgraph` keeps a public `api` module at its root. Cargo builds the crate from
`src/lib.rs` via `tonic-prost-build`, while Bazel generates it from `proto/api.proto` with
`rust_prost_library` and never reads `src/lib.rs`, so the `api` module name is the contract both
toolchains agree on. Consumers import `proto_dgraph::api`, and its generated stub keeps Dgraph's name,
`proto_dgraph::api::dgraph_client::DgraphClient`.

### External dependencies

Every external crate is pinned once in `[workspace.dependencies]` and referenced by member crates as
`{ workspace = true }`. Add a new one there or nowhere.

| Crate                                 | Version               | Used by                             | Why                                                                                                     |
|---------------------------------------|-----------------------|-------------------------------------|---------------------------------------------------------------------------------------------------------|
| `prost`, `tonic`, `tonic-prost`       | 0.14                  | `proto_dgraph`, `ultragraph_client` | Generated protobuf types and the gRPC surface                                                           |
| `tonic-prost-build`                   | 0.14                  | `proto_dgraph` (build)              | Cargo-side codegen. Bazel generates the same crate with `rust_prost_library` and the toolchains under `bazel/toolchains/` |
| `rustls`                              | 0.23, `ring` + `tls12` | `ultragraph_client`                 | TLS to the server                                                                                       |
| `tokio`                               | 1                     | `ultragraph_client`                 | The client's token cache lock; the test runtime                                                         |
| `docker_utils`, `ureq`, `serde_json`  | 0.3 / 3.3 / 1.0       | `ultragraph_client` (`test-utils`)  | Start a Dgraph container and read its health report, for the acceptance test                            |

## Building and Running

The project uses `make` for common development tasks:

* `make build`: Builds the entire workspace with Bazel
* `make test`: Builds, then runs the doc tests and the unit tests with Bazel
* `make fix`: Fixes linting issues as reported by `clippy`
* `make format`: Formats Bazel files with `buildifier` and Rust code with `cargo fmt`
* `make check`: Checks for outdated and unused dependencies and security vulnerabilities
* `make doc`: Builds the documentation and runs the doc tests
* `make count`: Counts lines of code
* `make update`: Updates the Rust toolchain and Cargo dependencies

Bazel is the build of record; Cargo serves the IDE. Bazel resolves external crates through
`all_crate_deps()` reading the Cargo manifests, so a dependency added to a member `Cargo.toml` needs no
`BUILD.bazel` edit.

## Code structure

Each crate adheres to the following base structure

src/errors/mod.rs  # contains each error type in a separate file
src/traits/mod.rs # contains each trait in a separate file
src/types/mod.rs # contains each type in a separate file

## One type, one Rust module.

For very small types (total implementation in less than 25 lines), the type is stored in a file named as
snake_case of the type name. For example:

src/types/small_type.rs

For more complex types, the type is stored in a folder module, for example the type Uncertain is stored in:

src/types/uncertain/mod.rs

The mod.rs contains the type definition and constructors.

When the type implements multiple traits, each trait is stored within a file named after the implementing
trait or trait group. For example, when implementing PartialEq and Debug for type Uncertain, these would be
in files:

src/types/uncertain/uncertain_debug.rs
src/types/uncertain/uncertain_part_eq.rs

## Optional src folders

src/extensions/mod.rs # contains type extensions i.e. a default impl for a trait
src/utils/mod.rs # contains utils

## Test structure

Every test file must be registered to the corresponding mod file and that module must be registered with
its higher up module. Ensure the correct #[cfg(test)] annotation is set for each registered test file.

Test targets live in the crate's own `BUILD.bazel`, beside the library. A crate whose tests mirror `src/`
through `tests/mod.rs` takes one `rust_test` with `crate_root = "tests/mod.rs"`, tagged `unit-test`.
A `rust_test_suite` over `*_tests.rs` fits only when every test file stands alone.

`ultragraph_client` has three test roots, one per `[[test]]` in its `Cargo.toml` under
`autotests = false`: `tests/lib_tests.rs` for the crate as consumers get it, `tests/utils_test_tests.rs`
for the `test-utils` fixture, and `tests/dgraph_container_test.rs`, the acceptance test against a
running Dgraph, tagged `integration_test` and run with
`bazel test //... --test_tag_filters=integration_test`. `make test` leaves it out.

The tests folder replicates the exact src folder structure, for example:

tests/errors/mod.rs # contains tests for each error type in a separate file
tests/traits/mod.rs # Optional contains tests for each trait in a separate file
tests/types/mod.rs # contains tests for each type in a separate file

Test files replicate the source file name with an appended _tests. For example, a source file

`src/errors/normal_error/normal_error.rs`

is matched with the test file under the tests folder:

`tests/errors/normal_error/normal_error_tests.rs`

Shared utils used for testing are stored in the src tree under:

`src/utils_test/mod.rs # contains utils`

Bazel cannot access util files from within the test folder, but it can access the full src folder during
testing. As a result, test utils have to be fully tested to count towards the code coverage score.

The usage of a prelude file is prohibited.

## Code export

* All public types, traits, and errors are exported from src/lib.rs
* internal modules remain private at the root level.

## Code import

When importing from a crate, always import directly from the root level, for example:

`use common_version::PRODUCT_NAME;`

## Code conventions

Field visibility:
* Public types: All fields will be private, and access will be provided through constructors, getters,
  and setters as appropriate.
* Private types: Public fields may be used, provided they do not leak outside their defined scope.

Static Dispatch:
* Use static dispatch
* Avoid usage of dyn, trait objects, and dynamic dispatch

Coding style:
* Prefer idiomatic zero cost abstractions
* Prefer functional style i.e. map, flatmap, filter when dealing with collections

Safety and security style:
* No `unsafe`. Every crate opts into the workspace lints with `[lints]` and `workspace = true` in its own
  `Cargo.toml`; new crates MUST include this.
* Avoid macros in all lib code i.e. everything under /src. Macros for testing are permissible when used
  sparingly i.e. for bulk testing many types implementing the same trait.
* Avoid the introduction of external crates unless it is necessary.
