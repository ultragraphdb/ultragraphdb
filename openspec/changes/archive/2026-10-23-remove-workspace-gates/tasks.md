## 1. Remove the gates

- [x] 1.1 Remove the trace, the declarations check, the golden-file scan, `workspace_root()`, their types, unit tests and BDD scenarios
- [x] 1.2 `ultragraphdb_tool_bdd`'s Bazel targets drop `//:MODULE.bazel` data and the `local` and `external` tags; the root package stops exporting `MODULE.bazel`
- [x] 1.3 `AGENTS.md` and the stage changes' close tasks no longer name the gates
- [x] 1.4 `make test` green with every target sandboxed; `scripts/bdd_counts.sh` reports equal counts
