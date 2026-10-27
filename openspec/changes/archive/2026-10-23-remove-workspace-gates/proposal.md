## Why

Three gates in `ultragraphdb_tool_bdd` read the repository from inside `make test`: the trace of spec scenarios to
feature scenarios, the check that each crate declares its feature suite for Cargo and Bazel, and the check that each
golden file names its Dgraph commit. To do so their Bazel targets followed `//:MODULE.bazel`'s runfiles link into the
working tree and carried the `local` and `external` tags, so they ran unsandboxed and uncached. Every Bazel target must
be hermetic and sandboxed, or the cache is wrong.

## What Changes

- The three gates, their library functions (`trace`, `spec_scenarios`, `feature_scenarios`, `check_declarations`,
  `golden_files`), their types and their tests are removed, with `workspace_root()`.
- `ultragraphdb_tool_bdd`'s targets read only declared inputs; the root package no longer exports `MODULE.bazel`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `port-testing`: the traceability and declaration requirements are removed; the golden requirement no longer scans
  the workspace for commits.

## Impact

`../../../../ultragraphdb_tool/ultragraphdb_tool_bdd`, the root `BUILD.bazel`, `AGENTS.md`, the stage changes' close tasks.
