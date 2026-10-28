# Native clients: one neutral schema, generated bindings

Status: a later stage, when early adopters ask for clients in languages other than Rust (decided 2026-10-28). Nothing
here is built in stage 5. Stage 5's part is to keep the protocol free of anything that would block it: every byte
layout stated in the specification in its own words, none left to a Rust crate's internals.

## The idea

A native client in Python, JavaScript, C or another language implements the binary protocol's bytes directly. It never
wraps the Rust client: no FFI, and no WebAssembly build of the Rust client core.

The protocol's messages are described once, in a language-neutral schema kept in the repository: the header, every
command's body, the section tables, the records, the request term table, result batches and error codes, field by field,
with each field's type, width, byte order and rules. From that schema a generator per language produces the language's
protocol bindings: the message types and their encoders and decoders. A client in that language is the generated
bindings plus a small hand-written layer.

| Layer | Source | In Rust (stage 5) |
|---|---|---|
| Message layouts, encoders and decoders | generated from the schema | `ultragraphdb_protocol_wire` |
| Field rules: bounds, one encoding per value | generated from the schema; checksums call the language's XXH3-64 library | the wire's validation |
| Connection state: framing, the handshake, streams, credit, cancel | hand-written per language, small, specified | `ultragraphdb_protocol_transport` (sans-I/O) |
| Client logic: sessions, transactions, retries through the session | hand-written per language, specified | `ultragraphdb_client_core` (sans-I/O) |
| The language's convenience API | hand-written | `ultragraphdb_client_tokio`, `ultragraphdb_client_compio` |

The generated layer is the large, error-prone part: hundreds of fields and offsets. The hand-written part is a few
hundred lines of state machine per language, and the specification describes it.

## The schema is the source of truth

- The specification's layout tables are rendered from the schema, so the documents cannot drift from the code.
- The Rust codecs are either generated from the schema or checked against it field by field.
- The golden frames of the conformance suite verify every generator: each language's generated bindings read every golden
  frame and rebuild it byte for byte, and refuse every malformed frame by the rule the suite names. A new language's
  codec is checked against the same bytes as Rust's.
- The wire primitives are chosen so every target language has them: fixed-width little-endian integers, UTF-8 text
  delimited by offsets with no escape grammar, and XXH3-64 as `doc/xxhash_spec.md` of the xxHash repository specifies
  it.

## Precedent

Apache Kafka defines its protocol in JSON message specifications under `clients/src/main/resources/common/message/`,
translated into Java code that reads and writes the messages when Kafka is compiled; the Kafka Protocol Guide describes
how each field is serialized. The approach here follows it: a schema of the messages, generated codecs, hand-written
connection and client logic.

Kaitai Struct generates parsers for many languages, but its serialization support is limited and the chunk checksums
would need custom processing. FlatBuffers and Cap'n Proto generate code for many languages but would replace the wire
format rather than describe it. A small schema of the repository's own, in JSON or TOML, with a generator per language,
fits a custom format.

## When the stage comes

1. Write the schema from the specification, and check the Rust codecs against it with the golden frames as the oracle.
2. Render the specification's layout tables from it.
3. Generate the first language's bindings, write its connection and client layer from the specification, and run it
   against the conformance suite and the server.

Retrofitting is cheap because the golden frames pin the bytes.

## Sources

- [Apache Kafka message definitions, README](https://docs.airlock.com/iam/8.6/licenses/kafka-clients-4.2.0.jar/common/message/README.md)
- [Apache Kafka, `clients/src/main/resources/common`](https://apache.googlesource.com/kafka/+/HEAD/clients/src/main/resources/common)
- [The sans-I/O pattern](https://sans-io.readthedocs.io/how-to-sans-io.html)
