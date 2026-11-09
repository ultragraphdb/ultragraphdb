# UltraGraphDB

UltraGraphDB is a next generation distributed graph database written in Rust.
UltraGraphDB aims for
- Ultra Speed
- Ultra Scale
- Ultra Robust and Reliable

## Build

Bazel builds and tests the workspace.

```sh
make build   # bazel build //...
make test    # build, then the doc tests and unit tests
```

`make help` lists every target.

## Requirements

UltraGraphDB runs on Linux with a modern file system: ext4, XFS or Btrfs. The write-ahead log reserves its segments
with `fallocate`, writes aligned blocks and makes each round durable with `fdatasync`, and the server's I/O runs on
io_uring.

The kernel is Linux 6.6 or later that supports:
- `IORING_SETUP_SINGLE_ISSUER` (6.0)
- `IORING_SETUP_DEFER_TASKRUN` (6.1)

Newer io_uring operations are probed at startup and fall
back to a blocking call where the kernel lacks them, so a newer kernel runs more of the I/O on the ring. io_uring must
be enabled: `kernel.io_uring_disabled` at 0, and in a container a seccomp profile that allows `io_uring_setup`,
`io_uring_enter` and `io_uring_register`, which Docker's default profile blocks. 

| Platform | Support |
|---|---|
| Linux 6.6 or later on ext4, XFS or Btrfs | Supported, for development and production |
| macOS | Works for development; not suited to production, for safety or performance: `fsync` does not flush the drive's cache, segments are not preallocated, and I/O runs on kqueue instead of io_uring |
| Windows | Not supported |

## License

Apache-2.0; see `LICENSE`. 
