# Group 0: workspace

## 0.1 Dependencies

- `compio` 0.19.2, no default features; `runtime`, `io-uring`, `fs`, `net`, `time`. The stable release Apache Iggy
  builds on; 0.20.0 is in beta. `cargo tree -e normal,build` on `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-gnu`
  and `aarch64-apple-darwin` finds no `cc`, `cmake` or `bindgen`. Under `--target all`, `cc` enters through
  `generator` ← `loom` ← `compio-executor`, which `compio-executor` 0.1.4 declares under `cfg(loom)` alone, a test
  configuration. `windows-sys` and `linux-raw-sys` are Rust declarations, not C.
- `xxhash-rust` 0.8.19, `xxh3`; pure Rust.
- `arbitrary` 1.5.0, `derive`, for the fuzz packages.
- `futures-channel` through the pinned `futures` 0.3.

## 0.2 Crates

Twelve crates scaffolded from `ultragraphdb_error_store`'s layout (manifest with `autotests = false` and the `unit`
test at `tests/mod.rs`, `[lints] workspace = true`, `BUILD.bazel` with library, doc, doc test and test target). The
server node holds the `ultragraphdb` binary. `cargo build --workspace` and `bazel build` of the 49 new targets pass.

## 0.3 Lean

`lean/` is a Lake project pinned to `leanprover/lean4:v4.14.0`, MGQL's release, with the library `RoundModel`;
`lake build` passes. `lean/.lake/` is ignored.

## 0.4 Fuzzing

- `cargo-fuzz` 0.13.2 installed; `scripts/fuzz.sh` uses `nightly` unless `FUZZ_TOOLCHAIN` names another.
- Exercised: no fuzz package anywhere (reports and exits 0), an unknown crate (names it, exits 1), a crate without a
  fuzz package (names it, exits 1), a non-numeric `FUZZ_SECONDS` (exits 1). A first run of the unknown-crate case
  exited 1 silently: `grep` matching nothing under `pipefail` ended the script inside `crate_dir`; fixed with an
  explicit `|| true`. `scripts/mutants.sh`'s `crate_dir` has the same pattern, reached only with a file argument; it is
  left as it is and reported.
- The fuzzing run itself is exercised first by task 1.8's target.
- `ultragraphdb_tool_bdd::fuzz_regressions` and `fuzz_regressions_in`, through the five phases:
  1. Surface with `unimplemented!()` bodies, compiling.
  2. Six tests against committed sample data (`testdata/fuzz_regressions/sample_target/`: `a_case` = 0xFF, `b_case` =
     0x01 0x02, `c_empty` = no bytes, `nested/` a directory). Run against the surface: 6 failed, each at the
     `unimplemented!()` panic (lines 17 and 28), the `should_panic` test failing because the message did not name the
     file.
  3. Audit: a missing sort cannot be told apart on APFS, which lists a directory in name order; on Linux's ext4 the
     order test catches it. Recorded, not hidden.
  4. Implemented; the first run failed 2 tests because a slip in writing the fixture (an `xxd` call given two files
     wrote a dump into `b_case`) had changed the data, which the tests caught; the fixture was restored. Cargo: 19
     passed. Bazel `//ultragraphdb_tool/ultragraphdb_tool_bdd:ultragraphdb_test_bdd_tests`: 19 passed. Clippy clean.
  5. `scripts/mutants.sh ultragraphdb_tool_bdd src/utils/fuzz_regressions.rs`: 15 mutants, 15 caught.
