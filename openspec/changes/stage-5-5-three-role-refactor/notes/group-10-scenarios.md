# Group 10: the client-visible scenarios and their bindings
Task 10.4's list. Each client-visible scenario of groups 9, 9.5 and 10, from the change's specs, and where a real client meets it through real frontend, compute and storage: its feature under `ultragraphdb/ultragraphdb_tests/features/` and the runs it takes, or why it stays elsewhere. Reasons read: *double*, it needs a role misbehaving in a way a signal cannot make a real process do; *client*, the real client cannot send it; *stage*, this stage does not hold what it names; *time*, the wait outweighs what it adds beside the suite binding it.

## client-endpoint

| Scenario | Binding |
|---|---|
| Connections past the bound are refused | `client-endpoint/processes.feature`, compio over TCP |
| An eviction reaches every core at once | `client-endpoint/processes.feature`, compio over TCP |
| A blocked commit does not block other connections | double: an owner check made to wait has no switch on the real owner |
| A TLS client reaches the endpoint | `client-endpoint/endpoint.feature`, both shells over TLS |
| A clear-text client is refused by a TLS endpoint | `client-endpoint/endpoint.feature`, both shells over TLS |
| A wrong password is refused without detail | `client-endpoint/endpoint.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A data request before authentication is refused | client: it queues requests until authenticated; raw frames at the frontend bind it |
| An unknown name answers as a known one does | `client-endpoint/processes.feature`, compio over TCP |
| A write reaches its partition | `client-endpoint/routing.feature`, compio over TCP and Unix |
| A stale placement map is corrected | double: no command moves a bucket in this stage, so a real frontend's map is never stale |
| A home's code 26 reaches the client | double: a home refusing a registration with `detail` 0 needs a played partition |
| Later requests wait for the first | double: as the stale map above |
| A first read moved by a stale map is resent | double: as the stale map above |
| A first read the map does not move is refused | `client-endpoint/spread.feature`, compio over TCP |
| A read's code 23 answers the transaction's other reads | `client-endpoint/processes.feature`, compio over TCP |
| A first request spreading over partitions is refused before any Bind | `client-endpoint/spread.feature`, compio over TCP |
| A first read holds a later read only until its first frame | `client-endpoint/shared.feature`, compio over TCP |
| Reads alternate between two compute processes | `client-endpoint/processes.feature`, compio over TCP |
| A closed compute process is skipped | `client-endpoint/processes.feature`, compio over TCP |
| No compute process can be reached | `client-endpoint/processes.feature`, compio over TCP |
| A read is never resent after a frame | `client-endpoint/processes.feature`, compio over TCP |
| A hung compute process answers code 17 | `client-endpoint/processes.feature`, compio over TCP |
| A compute process waiting within the deadline is not hung | `client-endpoint/processes.feature`, compio over TCP |
| A frame restarts a hung compute's timer | double: compute sending frames with chosen gaps past a deadline |
| A stream without credit holds off a hung compute's timer | `client-endpoint/processes.feature`, compio over TCP |
| A partition's invariant violation answers Unavailable | double: no input makes a real partition break an invariant |
| A role connection closing under a request answers code 17 | `client-endpoint/processes.feature`, compio over TCP |
| A role connection's dial past its bound closes it | double: a peer that accepts and says nothing on a dial the frontend opens; a real role is dialled at its Join, before a signal can stop it |
| A vector index is refused | `client-endpoint/endpoint.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A command of a later stage is refused on its stream | client: it offers no Install or Query; raw frames at the frontend bind them, and "A Prepare of a later stage's language is refused" binds the Prepare here |
| The gate allows an authenticated principal in this stage | `client-endpoint/endpoint.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| An unknown tenant is refused after the gate | `client-endpoint/endpoint.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A principal the gate denies learns nothing of a number | double: this stage's authorizer allows every authenticated principal, so denial exists only under a test authorizer |
| The gate fails closed | double: as above |
| A denied request reaches no role | double: as above |
| A write past `held_bytes` is refused | `client-endpoint/processes.feature`, compio over TCP |
| Credit still drains a read under held writes | `client-endpoint/shared.feature`, compio over TCP |
| An unbound transaction passes its limit | `client-endpoint/limits.feature`, compio over TCP |
| A bound transaction passes its limit | `client-endpoint/limits.feature`, compio over TCP |
| A read holding a write back is named at the limit | `client-endpoint/limits.feature`, compio over TCP |
| A relayed Commit is not timed | time: 12 s beside a process stop, against the frontend's suite |
| An empty transaction commits at the frontend | `client-endpoint/endpoint.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A resent empty Commit answers not committed | `client-endpoint/lost_replies.feature`, compio over TCP |

## metadata-group

| Scenario | Binding |
|---|---|
| A catalog version past its cap is refused at its Commit | `metadata-group/catalog.feature`, compio over TCP |
| An unknown name keeps one salt across a restart | bound here as "An unknown name answers as a known one does", which asks the same across a restart |
| A registered session survives a restart | time: a whole cluster stopped and started; the frontend's suite binds it through the metadata group |
| The first start creates tenant 0 | every scenario of this list begins in the tenant it creates |

## partitions

| Scenario | Binding |
|---|---|
| A transaction reaching a second partition aborts | `partitions/spread.feature`, compio over TCP |
| Write skew across partitions is refused, not committed | `partitions/spread.feature`, compio over TCP |
| A reverse read over a spread tenant aborts a read-write transaction | `partitions/spread.feature`, compio over TCP |
| A reference to another partition's entity stays in one partition | `partitions/spread.feature`, compio over TCP |
| A Drop over several partitions is refused | `partitions/spread.feature`, compio over TCP |
| A delete by pattern of another partition's subject is refused | `partitions/spread.feature`, compio over TCP |
| A delete by pattern leaves other partitions' edges | `partitions/spread.feature`, compio over TCP |

## quad-reads

| Scenario | Binding |
|---|---|
| A bound subject asks one partition | `quad-reads/spread.feature`, compio over TCP |
| A bound literal object fans out | `quad-reads/spread.feature`, compio over TCP |
| An unbound subject merges every partition | `quad-reads/spread.feature`, compio over TCP |
| A vector object reads the forward lists | `quad-reads/spread.feature`, compio over TCP |
| A reifier's skolem IRI reads its statement's properties | `quad-reads/access_paths.feature`, compio over TCP |
| A read sees its transaction's own write | `quad-reads/transaction_reads.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A read sees its transaction's new term and its emptied list | `quad-reads/client_order.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A term nobody holds conflicts with its later statement through compute | `quad-reads/transaction_reads.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A write waits for a read sent before it | `quad-reads/client_order.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A read waits for a write sent before it | `quad-reads/client_order.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A scanned range conflicts with an insert into it through compute | `quad-reads/transaction_reads.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A reverse range conflicts with an edge whose reverse entry lands in it | `quad-reads/transaction_reads.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A range stopped at its limit conflicts only with what it returned | `quad-reads/transaction_reads.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A limit stopping before a graph leaves that graph unread | `quad-reads/client_order.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| Resuming records the rest | `quad-reads/transaction_reads.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A read past the read set's room ends its transaction | time: a megabyte of spans takes tens of thousands of reads; compute's suite binds it with a smaller room |
| A Commit takes the writes held behind a read | `quad-reads/client_order.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| An Abort drops what a read held back | `quad-reads/client_order.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A read the client cancels releases the write behind it | `quad-reads/client_order.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A read cancelled while it is held is dropped unsent | `quad-reads/client_order.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A span recorded after a Cancel only widens the commit's check | double: a span recorded between a Cancel and its arrival is a race only a played frontend can order |
| A statement's record read conflicts with a property written | `quad-reads/transaction_reads.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A read under way when its transaction ended takes the stream's Error | `quad-reads/limits.feature`, compio over TCP |
| A read already answered takes nothing further | `quad-reads/client_order.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A reverse read gathers every referrer | `quad-reads/spread.feature`, compio over TCP |
| A lagging referrer delays the read, never truncates it | double: one partition lagging behind the others of one storage process |
| A bound object reads the reverse keyspace | `quad-reads/access_paths.feature`, compio over TCP |
| An unbound graph reads every graph | `quad-reads/access_paths.feature`, compio over TCP |
| A statement without a statement ID is named by its quad | `quad-reads/access_paths.feature`, compio over TCP |
| Parallel edges are told apart by their reifiers | `quad-reads/access_paths.feature`, compio over TCP |
| A reverse read names each parallel edge's reifier | `quad-reads/access_paths.feature`, compio over TCP |
| An unbound graph is walked by Scans of limit 1 | `quad-reads/access_paths.feature`, compio over TCP |
| A graph appearing in a gap conflicts | `quad-reads/client_order.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A read resumes where it stopped | `quad-reads/resume_keys.feature`, compio over TCP |
| A resumed read may find nothing | `quad-reads/resume_keys.feature`, compio over TCP |
| A resume key with another pattern is refused | `quad-reads/resume_keys.feature`, compio over TCP |
| A resume key holds terms, not keys | `quad-reads/access_paths.feature`, compio over TCP |
| The empty result | `quad-reads/resume_keys.feature`, compio over TCP |
| A resume key of another transaction is refused | `quad-reads/resume_keys.feature`, compio over TCP |
| A reader at a named version resumes after a reconnect | `quad-reads/resume_keys.feature`, compio over TCP |
| A resumed version no longer retained is refused | `quad-reads/retention.feature`, compio over TCP |
| A limit stopping inside one object's statements resumes there | `quad-reads/resume_keys.feature`, compio over TCP |
| A limit stopping after an object with no statement ID skips it whole | `quad-reads/resume_keys.feature`, compio over TCP |
| A limit stopping inside a statement's record resumes there | `quad-reads/resume_keys.feature`, compio over TCP |
| Only the last batch holds a key | `quad-reads/resume_keys.feature`, compio over TCP |
| An unlimited read that fails restarts | double: as above |
| A read after a commit sees it through its token | `quad-reads/levels.feature`, compio over TCP |
| Two reads of one transaction share its snapshot | `quad-reads/levels.feature`, compio over TCP |
| A monotonic-fast read does not wait for a lagging partition | double: as above |
| Monotonic-fast reads never go back | `quad-reads/levels.feature`, compio over TCP |
| A monotonic-fast read never answers below the token | `quad-reads/levels.feature`, compio over TCP |
| A latest-state read across partitions is refused | `quad-reads/spread.feature`, compio over TCP |
| A later latest-state read waits for the first read's partition | `quad-reads/spread.feature`, compio over TCP |
| A first read refused leaves the next read first | `quad-reads/spread.feature`, compio over TCP |
| Concurrent monotonic-fast reads take the highest epoch answered | `quad-reads/levels.feature`, compio over TCP |
| A batch stops at the client's frame | client: its Hello always names 1,048,576, so 524,288 cannot be asked; compute's suite binds it with a played frontend |
| A row too large for the client's frame is refused | client: as above; at 1,048,576 the row is past what one write holds |
| A resume key that does not fit goes alone | client: its Hello's frame leaves no row sizes a test can choose short of a frame; compute's suite binds it |
| An Execute too large for compute's frame is refused | client: a parameter large enough passes the Hello's frame first; the frontend's suite binds it |
| A reader coming back within the retention keeps its version | `quad-reads/retention.feature`, compio over TCP |
| A reader coming back past the retention is refused | `quad-reads/retention.feature`, compio over TCP |
| An open Scan holds its version past the retention | `quad-reads/retention.feature`, compio over TCP |

## role-binaries

| Scenario | Binding |
|---|---|
| The role secret opens no client session | `role-binaries/client_session.feature`, compio over TCP |

## schema-changes

| Scenario | Binding |
|---|---|
| A resent Drop builds the version a stopped frontend left | double: the frontend must stop between a partition's reply and its own step |
| A Drop through no session runs again | `schema-changes/drops.feature`, compio over TCP |
| A default entry outlives a drop | `schema-changes/drops.feature`, compio over TCP |

## system-identity

| Scenario | Binding |
|---|---|
| A result names an IRI by its text | `system-identity/results.feature`, compio over TCP |
| A statement with a statement ID is named by its reifier | `system-identity/results.feature`, compio over TCP |
| A resume key carries terms, not keys | `system-identity/results.feature`, compio over TCP |
| A tampered resume key is refused | `system-identity/results.feature`, compio over TCP |

## term-dictionary

| Scenario | Binding |
|---|---|
| A dangling ID read through compute answers code 17 | double: needs a store corrupted under a running partition |
| Transactions naming only new blank nodes bind in turn | `term-dictionary/spread.feature`, compio over TCP |
| A forged skolem IRI in a write is refused at its home | `term-dictionary/spread.feature`, compio over TCP |
| A home's failed store never reads as absent | double: needs a store failing under a running partition |

## transactions

| Scenario | Binding |
|---|---|
| A read above the watermark is refused | `transactions/requests.feature`, compio over TCP |
| A transaction sees every commit acknowledged before it began | `transactions/requests.feature`, compio over TCP |
| A snapshot read waits for its epoch | double: as above; "A read with a future token waits" binds the wait itself |
| A read-only transaction does not abort | `transactions/requests.feature`, compio over TCP |
| Write skew is refused | `transactions/requests.feature`, compio over TCP |
| Inserts of different objects into one list both commit | `transactions/requests.feature`, compio over TCP |
| Inserts of the same new statement conflict | `transactions/spread.feature`, compio over TCP |
| A term found absent conflicts with a statement of it | `transactions/requests.feature`, compio over TCP |
| Creating a dictionary entry conflicts with nothing | `transactions/spread.feature`, compio over TCP |
| An absent term is named before the range that read it | `transactions/requests.feature`, compio over TCP |
| An aborted transaction writes nothing | `transactions/requests.feature`, compio over TCP |
| A retried commit commits once | `transactions/lost_replies.feature`, both shells over TCP |
| A session commits through two partitions | `transactions/lost_replies.feature`, compio over TCP |
| An evicted session is told so | `transactions/processes.feature`, compio over TCP |
| A session with a commit in flight is not evicted | `transactions/processes.feature`, compio over TCP |
| A full registry of commits in flight refuses a Register as busy | `transactions/processes.feature`, compio over TCP |
| A Cancel after a commit reaches its partition holds its session from eviction | client: it offers no cancel of a commit; raw frames at the frontend bind it |
| A retry of a request never committed is told so | `transactions/lost_replies.feature`, compio over TCP |
| A session belongs to its principal | stage: the bootstrap principal is the only one, until stage 7's administration commands |
| A reused number on an open transaction is refused | `transactions/requests.feature`, compio over TCP |
| A commit answered code 17 is resolved by its resend | double: "committed, and the role connection closed before the reply" cannot be ordered on real processes; the lost-reply scenarios bind the stored reply, "A Recall reaching no answer" code 17 |
| A Recall reaching no answer stays in doubt | `transactions/processes.feature`, compio over TCP |
| Any other refusal of a Recall stays in doubt | double: a partition's address refusing a Recall as another's |
| A stored reply outranks every other Recall answer | double: three partitions answering three chosen ways |
| A retry of a session evicted meanwhile is unknown | `transactions/processes.feature`, compio over TCP |
| A one-frame write's resend is decided by the table | `transactions/processes.feature`, compio over TCP |
| A full commit queue answers busy | double: filling the queue is a simulator's work |
| A write set over the Welcome's bound is refused | `transactions/requests.feature`, compio over TCP |
| A transaction reaching a second partition is not retryable | `transactions/spread.feature`, compio over TCP |
| A statement named by its reifier is checked at its subject | `transactions/spread.feature`, compio over TCP |
| A Commit reply past the floor of `max_frame` is refused | `transactions/processes.feature`, compio over TCP |
| Concurrent insert-if-absent yields one node | `transactions/operations.feature`, compio over TCP |
| Concurrent upsert-and-link deduplicates and links atomically | `transactions/operations.feature`, compio over TCP |
| Properties for creation are written once | `transactions/operations.feature`, compio over TCP |
| An upsert-and-link in an aborted transaction writes nothing | `transactions/operations.feature`, compio over TCP |
| A failed compare-and-set writes nothing | `transactions/requests.feature`, compio over TCP |
| Counter adds commute | `transactions/operations.feature`, compio over TCP |
| A counter add returns the counter's new value | `transactions/requests.feature`, compio over TCP |
| A claim owned by another partition is cross-partition | `transactions/spread.feature`, compio over TCP |
| A created node is named by its skolem IRI | `transactions/requests.feature`, compio over TCP |
| An insert commits in one request | `transactions/requests.feature`, compio over TCP |
| A DirectWrite on a unique predicate is refused | `transactions/requests.feature`, compio over TCP |
| Concurrent identical inserts both succeed | `transactions/requests.feature`, compio over TCP |
| An upsert decides in commit order | `transactions/requests.feature`, compio over TCP |
| A failed condition is an answer | `transactions/requests.feature`, compio over TCP |
| A retried write commits once | `transactions/lost_replies.feature`, both shells over TCP |
| A write reaching two partitions is refused | `transactions/spread.feature`, compio over TCP |
| A declaration in a write is refused | `transactions/requests.feature`, compio over TCP |
| A ConditionalWrite claiming a value goes to its claim | `transactions/spread.feature`, compio over TCP |
| A resent blank-node write reaches one session table | `transactions/lost_replies.feature`, compio over TCP |
| A token names its commit | `transactions/tokens.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A read with a token sees the commit | `transactions/tokens.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A causally later commit is not stamped earlier | `transactions/spread.feature`, compio over TCP |
| A reply through no session carries no token | `transactions/tokens.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A named read version takes nothing from a token | `transactions/tokens.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A token above the open epoch is refused | `transactions/tokens.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A one-frame write carries the session's token | `transactions/tokens.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A zero token replaces nothing | `transactions/tokens.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A vanished client's transaction is aborted at its limit | `transactions/limits.feature`, compio over TCP |
| A closed connection aborts its transaction at once | `transactions/requests.feature`, compio over TCP |

## wire-protocol

| Scenario | Binding |
|---|---|
| A client authenticates with SCRAM-SHA-256 | `wire-protocol/connections.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A wrong password is refused without saying why | `wire-protocol/connections.feature`, all but Unix sockets: compio over TCP and TLS, tokio over TCP and TLS |
| A request before authentication is refused on its stream | client: as above |
| An unfinished authentication times out | time: 60 s by a limit no setting moves; the transport binds it |
| A retired number is refused | client: it sends no such command; the transport binds it |
| A replica command number is refused in this stage | client: as above |
| A skolem IRI nobody can hold binds nothing | `wire-protocol/terms.feature`, compio over TCP |
| A skolem IRI nobody can hold is refused in a change | `wire-protocol/terms.feature`, compio over TCP |
| The same path under another base is an ordinary IRI | `wire-protocol/terms.feature`, compio over TCP |
| A result names IRIs by their text | `wire-protocol/terms.feature`, compio over TCP |
| A result names a blank node by its opaque skolem IRI | `wire-protocol/terms.feature`, compio over TCP |
| A literal at a pattern's subject is refused | `wire-protocol/terms.feature`, compio over TCP |
| A label or a triple term at a pattern's object is refused | `wire-protocol/terms.feature`, compio over TCP |
| A triple term in a change is refused | `wire-protocol/terms.feature`, compio over TCP |
| A forged skolem IRI in a pattern matches nothing | `wire-protocol/terms.feature`, compio over TCP |
| A stale statement is refused | `wire-protocol/requests.feature`, compio over TCP |
| Describe answers from the metadata group | `wire-protocol/requests.feature`, compio over TCP |
| A literal at a node slot is refused | `wire-protocol/requests.feature`, compio over TCP |
| A commit-time operation's node is named by its IRI | `wire-protocol/requests.feature`, compio over TCP |
| A Describe past the client's frame is refused | client: as above |
| A statement prepared in another tenant is refused | stage: tenant 0 is the only one, until stage 7's `operate` |
| One connection's transactions act in two tenants | stage: as above |
| A repeated term is sent once per batch | `wire-protocol/requests.feature`, compio over TCP |
| A decimal the payload cannot hold is kept by its lexical form | `wire-protocol/terms.feature`, compio over TCP |
| A statement with properties is named by its reifier | `wire-protocol/terms.feature`, compio over TCP |
| A graph drop removes the graph | `wire-protocol/requests.feature`, compio over TCP |
| A tenant's data drop keeps its catalog | `wire-protocol/process_faults.feature`, compio over TCP |
| A drop retried through its session applies once | `wire-protocol/lost_replies.feature`, compio over TCP |
| A transaction writing into a dropped graph aborts | `wire-protocol/requests.feature`, compio over TCP |
| A DirectWrite is relayed with its principal | client-invisible: what the partition's frame holds; the frontend's suite reads the frame |
| A malformed write is refused | `wire-protocol/requests.feature`, compio over TCP |
| A write in one frame holds what it uses | client: it numbers requests itself and sends no names; `protocol_wire` binds the refusal |
| A client naming a principal is refused | `wire-protocol/requests.feature`, compio over TCP |
| A read with a future token waits | `wire-protocol/process_faults.feature`, compio over TCP |
| A read past its transaction's deadline is refused | `wire-protocol/process_faults.feature`, compio over TCP |
| A long read flows without a manual grant | `wire-protocol/credit.feature`, compio over TCP |
| A Result frame past its credit closes the client's connection | double: a server breaking the rule |
| A Result frame past its credit closes the frontend's role connection | double: a compute process breaking the rule |
| An Entries frame past its credit closes compute's role connection | double: a partition breaking the rule |
| A Register answered busy is sent again | `wire-protocol/process_faults.feature`, compio over TCP |
| A Register busy past its attempts returns code 14 | `wire-protocol/process_faults.feature`, compio over TCP |
| A connection lost under a Register's wait ends it | double: the loss must fall inside the retry delay; the client's test server orders it |
| An unsent commit is lost, a sent one in doubt | `wire-protocol/lost_replies.feature`, both shells over TCP |
| Code 17 on a commit is in doubt | `wire-protocol/process_faults.feature`, compio over TCP |
| A resent one-frame write refused for another reason did not commit | double: code 7 on a resend needs a server changing its answer |
| A commit through no session has an unknown outcome | `wire-protocol/lost_replies.feature`, compio over TCP |
| A Write on a new stream is refused | client: its types name a transaction for every Write; raw frames at the frontend bind it |
| A Commit in a read-only transaction is refused | client: `ReadOnly` before anything is sent; raw frames at the frontend bind the server's side |
| A Commit crossing its stream's Error is late | `wire-protocol/requests.feature`, compio over TCP |
| A request behind its transaction's Commit is late | client: `StreamFinished` before anything is sent; raw frames at the frontend bind it |
| A read crossing its transaction's Error takes that Error | `wire-protocol/requests.feature`, compio over TCP |
| A late frame reaches the client | `wire-protocol/requests.feature`, compio over TCP |
| A frame on a stream not yet opened is refused | double: a server breaking the rule |
| A refusal after rows fails the read | double: code 24 after rows needs a partition lagging mid-read; "A read is never resent after a frame" binds a refusal after rows with code 17 |
| A code's retry names what runs again | `wire-protocol/requests.feature`, compio over TCP |
| Code 17 runs a read again and leaves a commit in doubt | `wire-protocol/process_faults.feature`, compio over TCP |
| A long deadline survives the client's timeout | `wire-protocol/process_faults.feature`, compio over TCP |
| A stream without credit runs no timeout | `wire-protocol/process_faults.feature`, compio over TCP |
