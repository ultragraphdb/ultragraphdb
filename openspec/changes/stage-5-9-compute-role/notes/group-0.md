# Group 0: specifications and blockers first

Nov 18, 2026. Base `64887405`.

| Task | Record |
| --- | --- |
| 0.1, the deltas on stage 5.8's archived text | `group-0-deltas.md` |
| 0.2 and 0.3, `PROTOCOL.md` and `SECURITY.md` | `group-0-protocol.md` |
| 0.4, the laws | `laws.md` |
| 0.5, the corner cases | `corner-cases.md` |
| 0.6, the registry's counterexample | `group-0-registry.md` |
| 0.7, one reader rule | `group-0-reader.md` |

## Decided in this group

By the user:

- Reads wait behind a stopped role connection; design D2 stands.
- The client's read timeout floor rises to cover compute's worst case: 62,000 ms.
- A draining node refuses new work with a new error code, 33, Draining.

By the principles, each written into `PROTOCOL.md` (`group-0-protocol.md`, sections 9 to 11): the fifteen points the
laws' enumeration found open, the term table's order for an Execute's resume key, and the client's delay and backoff
bounds. A role's settings file is refused when it holds a table the role reads no setting of.

`AGENTS.md`'s role descriptions stay at three roles until the switch (task 6.5), since the file describes the code.

## The gate

`make build` and the Clippy aspect over `//...` passed. `make test` failed its first run and passed its second:

- **The failure.** `ultragraphdb_supervisor_tests:image_bdd` timed out in "The same frames cross TCP". The storage
  container logged `ready` within a second and the compute container was never started, so the test hung in its wait
  for storage's `/ready` at the published port until Bazel cut it about five minutes later.
- **What was run after it.** The target alone, uncached: 9 of 9 passed, about 17 s each. The full `make test` again:
  every phase green.
- **Not root-caused.** Nothing ties it to this group's change: the 21 other integration targets passed in the failed
  run, the 103 scenarios of `storage_tests` among them.
- **Why the cause is hidden.** `ultragraphdb_tests`' `until_answering` counts its 50 ms sleeps toward `START_SECONDS`
  and not the 2 s a probe waits for an answer that does not come, so its limit is about 14 minutes where it states 20 s,
  and Bazel's timeout cuts the test before it reports the container's log. A limit by the clock needs `Instant::now`,
  which the workspace disallows without an exception the user grants. Asked of the user on 2026-11-18.
