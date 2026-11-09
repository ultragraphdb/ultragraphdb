# Why UltraGraphDB?

## The problem

The DeepCausality project expanded into multiple knowledge-heavy areas, and tracking the relations between papers,
specifications, and implementations across domains became difficult. The initial plan was a knowledge graph, but
concepts recur across domains, sometimes with similar and sometimes with different meanings. Supporting AI agents across
the entire knowledge corpus became a requirement too, as frontier AI made noticeable progress over the last few years.

While working with the talented people at Carver Automation, I met a similar issue: data from multiple sources must
combine into a coherent knowledge representation. This is non-trivial because, unlike the DeepCausality knowledge,
ServiceRadar, the network intelligence tool developed by Carver Automation, also has real-time data streams at different
update intervals.

The core problem reduced to one question: how do you represent knowledge across different domains, sources, and update
intervals so that data records can be understood in context and actions follow from the representation?

The informed reader recognizes an ontology here, and building one was the obvious next step. The relations between data
then need storage. For DeepCausality, the quantity is moderate, but the complexity is high because of the substantial
cross-domain work. For ServiceRadar, the situation inverts: complexity is lower and confined to networking, whereas the
data volume is enormous and requires strict retention policies to keep storage in check.

The team at Carver Automation faced a different problem, though. For ServiceRadar, the foremost job of an ontology is
literally connecting the dots: find the outlier, ring the alert, and provide actionable intelligence. Stepping back,
their deeper challenge was connecting data across systems and data silos. Networking, like many IT areas, usually
entails different solutions from different vendors. Most enterprises already have a data warehouse and a data
lakehouse, and data "integration" often means you can query multiple data sources and create another management
dashboard. Like the cross-domain knowledge challenge, the team faces the challenge of reasoning across data sources to
determine the impact before changing anything. They report that the problem centers on configuration updates over the
network. In their experience, disruptive incidents often come from pushing an update that either triggered cascading
failures through a configuration mistake or bricked the affected system because a prerequisite was unmet. The problem
isn't that configuration management is unsolved; very good change management systems exist. The real culprit is the
scale and diversity of connected systems. For a single network management system, the problem would have been solved
long ago. In large enterprises, however, a central team manages networking for all remote sites with a multitude of
tools, and, critically, a regular workflow requires access through several tools. Catching a configuration glitch
within any one system is not the problem. The real problem sits between those systems and therefore surfaces only
randomly, usually when someone gets paged at 3 a.m.

Regardless of the specifics, the common theme across all identified use cases was connecting the dots across systems and
identifying impact before something breaks.

So the ontology, if you want to call it that way, means connecting the dots between systems as reality evolved over time.
The second part makes the problem hard because you sometimes must keep legacy systems alive while supporting the modern
workflow. The ontology then needs to discern correctly when the prior relations apply and when the new relations are
valid.

In short, the problem reduces to building a temporally aware ontology that connects the dots across data and systems to
act on cross-domain knowledge. And it needs to be stored somewhere.

## The Situation

Naturally, a graph database fits a central ontology well, but the graph database landscape has changed a lot over the
last few years. Some projects use an unfavorable BSL license (ArangoDB, SurrealDB), others switched to fully commercial
(NebulaGraph), and others were abandoned (Dgraph). Some new graph databases exist (HydraDB), but most don't support
clustering and HA in the OSS version. Very few provide clustering in the OSS version, ArcadeDB among them, but its
enormous performance claims (10 million records per second) rest on weak consistency guarantees that cause phantom
reads and, after a power loss, guaranteed data loss. For strictly analytical workloads, this is generally accepted
because the primary data lives safely in another database. An ontology, though, is generally primary data and needs
strict serialization guarantees to prevent data loss. The other major players are all fully commercial (Neo4j,
NebulaGraph, TigerGraph, etc.). They are good graph database systems, but they carry a steep price tag.
Other GraphDBs that wre ruled out:
Apache AGE:- fails on multihop. Thaks for the reminder. in general, all graph extensions over relations DBs must joind tables and thus only work well for small workloads.
JanusGraph - Tinkerpop is so outdated at this stage. It is true that it scales really well and it is used at large scale, its just hard to justify in 2026
Oxigraph, QLever, Jena: single-node doesnt fly at a large airline. pun intended.
TypeDB - no clustering in OSS version. unfavorable licence, non stanaded query language. I did tried, and it went nowhere.
Managed services - must be in premise.

Put simply, no PostgreSQL equivalent exists among graph databases.

## The context

At the same time, frontier AI systems have made significant progress. A small team ported PostgreSQL to Rust using AI
agents. Another team rebuilt the entire Adobe Creative Suite in Rust with AI. People build new programming languages
with AI. It still takes real domain expertise to undertake any of those projects, but AI's ability to implement complex
systems has improved significantly compared to one year ago.

I gained meaningful expertise in distributed systems by contributing to the Apache Iggy project while they designed
their high-performance clustering architecture. People experienced in distributed systems will quickly point out that no
single task needs optimizing; a multitude of interrelated topics must work well together to meet performance and
reliability goals. There's real wisdom here, because AI agents tend to narrow optimization goals without paying
attention to the implied trade-offs. As a result, performance can be stellar, yet write durability may raise serious
concerns that imply data loss under certain circumstances. "Certain circumstances" is a euphemism for "we don't know
when it will happen, but experience tells us it eventually will." AI agents are not necessarily bad at writing
concurrent or distributed system code; they prioritize certain aspects in ways a seasoned engineer would not.

In this context, the real question is whether AI can implement a very specific design for a distributed graph database,
eventually comply with querying standards such as GQL, and serve data reliably. Complying with standards when a
predefined outcome is well specified is almost the prime task for AI. AI does very well at reaching test coverage
against existing test oracles. It also does well at porting code from one language to another so that test results
replicate bit-identically. The remaining question is therefore: can the resulting database code be robust and reliable,
meaning it neither crashes nor loses data, while maintaining the expected performance?

From the pgRust project we know that an AI can faithfully port a very large C code base to Rust. That fell squarely
into AI's primary strength, though, because PostgreSQL has an enormous body of very high-quality conformance tests.
Whenever you give a frontier AI model an expected outcome, it eventually finds a way to meet it. Practitioners have
repeatedly observed that a hard constraint, such as an external oracle's criteria or existing conformance tests,
reliably combats AI slop and coerces the AI to write code that fulfills its specification. What is unclear is whether
you can still reach a reliable database system that hits its performance targets without such a comprehensive,
high-quality conformance suite.

There is no real answer to that.


## The Options

The options on the table are the stereotypical three:

1) Buy
2) Build
3) Adapt

Buy isn't an option because the budget is not there. Build was not the preferred option because building a database is,
of all systems, a high-effort, high-risk undertaking. Plan A was to adapt Dgraph, one of the few systems that provides
clustering and HA under a liberal Apache license. Initial exploration showed that Dgraph's new owner has basically
abandoned the project and none of the major bugs will get fixed. After inspecting the Dgraph source code, I categorically
rejected maintaining a patched fork. For the same reason, I eventually dropped the idea of porting Dgraph to Rust,
similar in spirit to the pgRust project.

## The Decision

I decided to build UltraGraphDB in an afternoon, after registering the domain and settling one key assumption: that
Claude Opus 5.5 can figure out database transactions. The domain name mattered because I always liked the name
UltraGraph (another graph library I wrote for the DeepCausality project), and it settled one of the two hardest problems
in computer science: naming. Hint: cache invalidation is not solved yet.

In retrospect, the decision process still feels as odd as it was. You don't usually decide based on domain name
availability, and what if Claude was hallucinating and had made up quantum-ether transactions that do not exist?

Domain names, it turned out, are as verifiable as the essence of DB transactions. Instead of asking Claude something I
had no idea about, I tested it on something I was absolutely sure about: the goroutine bottleneck that bound Dgraph to
about 10k tx/sec, often less, regardless of the number of cores. In a world without AI, this would make a nice
take-home assignment for a graduate-level database course. Quizzing Claude was an eye-opening experience. It pinpointed
the issue perfectly and, more importantly, explained why no number of additional CPU cores would fix anything. Claude
proposed two fixes that, once applied, gave a handy 30% performance boost. I would have rejected one patch because it
removed a sensible check, but the other was quite good as far as I can tell. Was that sampling enough? No. However,
while I was working on a modern storage-compute separation, Claude, pointed at the cloned StarRocks repository, aced
that one too. However, 