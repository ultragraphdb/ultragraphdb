# Group 4: the bench and its image

## 4.1 `client_bench` in a Job

- The password: `--password-file` already reads a file, and a Kubernetes Secret mounts as one, so the Job names the bootstrap
  Secret's file; nothing changed.
- The address: `--address` took a socket address, an IP and a port, which a Job dialling the frontend's client Service by its name
  cannot give, and the glibc base holds no shell to resolve it first. `ClientArgs` now resolves a host's name and a port once, at
  start (`resolved`, `std::net::ToSocketAddrs`, the first address), an IP and a port parsing as before; a name that resolves to
  nothing, or no port, is refused naming the value ("--address takes a host and a port, not ..."). Tests: the refusal of
  `localhost` without a port (its message now "a host and a port") and `test_an_address_by_name_is_resolved`, `localhost:7080` to a
  loopback address and port 7080, derived from the required arguments.
- Syncs per commit: not computed by the bench. The partition's `commit phases` log carries `commits` and `log_syncs` per window,
  which the measurement reads beside the bench's JSON line.

## 4.2 The bench's image

`client_bench_multiarch` (`ultragraphdb_tool_bench/BUILD.bazel`): `glibc_multiarch_img` of `:client_bench`, tagged `bench` as its
binary is, on the glibc base the roles ship on. Built with `-c opt`, its `oci_tarball` (11,583,488 bytes) loaded into `container`'s
store, its arm64 manifest tagged and run: user 65532, entrypoint `/client_bench`; with no arguments it prints its usage, and with
`--address frontend.invalid:7080` it refuses the name.

Gate: `cargo clippy -p ultragraphdb_tool_bench --all-targets -- -D warnings` and `cargo fmt` clean;
`bazel test //ultragraphdb_tool/ultragraphdb_tool_bench:all` passes its three targets.
