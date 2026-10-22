# ultragraphdb

A next generation distributed graph database in Rust.  

## Build

Bazel builds and tests the workspace; Cargo serves the IDE.

```sh
make build   # bazel build //...
make test    # build, then the doc tests and unit tests
```

`make help` lists every target.

## License

Apache-2.0; see `LICENSE`. 
