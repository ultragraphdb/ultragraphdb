## REMOVED Requirements

### Requirement: RDF N-Quads parse as Dgraph parses them
**Reason**: Dgraph's N-Quad and JSON mutation formats belong to DQL.
**Migration**: Writes are changesets (`changesets`); SPARQL Update and GQL writes produce them in stages 6 and 7, and RDF files load through stage 8's loaders.

### Requirement: JSON mutations parse into N-Quads
**Reason**: Dgraph's N-Quad and JSON mutation formats belong to DQL.
**Migration**: Writes are changesets (`changesets`); SPARQL Update and GQL writes produce them in stages 6 and 7, and RDF files load through stage 8's loaders.

### Requirement: Malformed values are refused
**Reason**: Dgraph's N-Quad and JSON mutation formats belong to DQL.
**Migration**: Writes are changesets (`changesets`); SPARQL Update and GQL writes produce them in stages 6 and 7, and RDF files load through stage 8's loaders.

### Requirement: Every JSON value carries its key's language
**Reason**: Dgraph's N-Quad and JSON mutation formats belong to DQL.
**Migration**: Writes are changesets (`changesets`); SPARQL Update and GQL writes produce them in stages 6 and 7, and RDF files load through stage 8's loaders.

### Requirement: JSON numbers decode as encoding/json decodes them
**Reason**: Dgraph's N-Quad and JSON mutation formats belong to DQL.
**Migration**: Writes are changesets (`changesets`); SPARQL Update and GQL writes produce them in stages 6 and 7, and RDF files load through stage 8's loaders.

### Requirement: The format option decides the input format
**Reason**: Dgraph's N-Quad and JSON mutation formats belong to DQL.
**Migration**: Writes are changesets (`changesets`); SPARQL Update and GQL writes produce them in stages 6 and 7, and RDF files load through stage 8's loaders.
