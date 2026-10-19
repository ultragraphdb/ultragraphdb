# ultragraphdb

A distributed graph database in Rust, ported from [Dgraph](https://github.com/dgraph-io/dgraph). 

## Build

Bazel builds and tests the workspace; Cargo serves the IDE.

```sh
make build   # bazel build //...
make test    # build, then the doc tests and unit tests
```

`make help` lists every target.

## License

Apache-2.0; see `LICENSE`. Code ported from Dgraph keeps its original copyright notices.
