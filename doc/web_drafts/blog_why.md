# Why UltraGraphDB?

## The problem

The DeepCausality project expanded into multiple knowledge heavy areas and over time, it became dificult to 
track the relations between papers, specifications, and implementations across multiple domains. Initially,
the idea was to develop a knowledge graph, but then it became apparent that multiple concepts occur across domains,
sometimes with similar and sometimes with different meaning. Support for Ai agent across the entire knowledge 
corpus became a requirement too as frontier Ai made noticable progress over the last few years. 

When working with the talented people at Carver Automation, a similar issue occured that data from mulitple  
different sources needs to be combind into a coherent knowledge represenation. This is a non-trivial challange because
unlike the knowledge of the DeepCausality project, Service Radar, the network inteligence tool developed by
Carver Automation, also had real-time data streams at different update intervals. 

The core problem reduced to the question of how to represent knowledge across different domains, sourced, and
at different update intervals so that date recordcs can be understood in context and actions can be derived
from the knowledge representation?

The informed reader immediatly recognizes that this describes an ontology and, indeed, the next obvious choice 
was to build an ontology. Then, the relation between data need to be stored somewhere. For the DeepCausality project,
the quanitity is fairly moderate, but the complexity is high due to the substantial cross domain work in that project.
For ServiceRadar, the situatiuon is inverted: Complexity is lower and confined to networking wheras the data
volume is enormous and requires strict retention policies to keep storage in check.

The team at Carver Automation faced a different problem though. For Service Radar, the formost job of an ontology
is all about literally connecting the dots, find the outlier, ring the alert, and provide actionable inteligence.
When stepping back, the deep challange faced by Carver Automation came down to connecting data across systems
and data silos. Networking, like so many other IT areas, usually ential different solutions from different vendors.
Obviosly, most enterprises already have a data warehouse,  a data lake house and data "integration" oftentimes
means you can query multiple data sources and crate another managmeent dashboard. Similar to the cross domain
knoledge challange, the team at Carver Automation faces a challane with reasoning across different data sources to determine
the impact before changing anything. They report this problem centers aroun configuration updates over network.
In their experience, distruptive incidents oftentimes are caused by pushing an update over network that either
triggered cacading failures due to an configuraiton mistake, or that bricked the affected system because
some prerequsit of the update was not met. The problem isn't
that configurationmangermetn hasn't been solved.There are, in fact, very good change management systems that try to solve the problem.
Instead, the real culprit is scale and diversity of connected systems. For a single networking mangement system, 
the problem would not exists because it would have been solved a long time ago. The reality in large enterprises
is that networking is managed centrally for all remote sites, a multitude of tools are used, and, critically,
a regular workflow requires access through multiple tools. The problem is not catching a configuration glitch within any
one system. The real problems sits between all those systems and, as a result, surfaces only randomly and that is
when someone gets paged at 3am. 

Regardless of the specifics, the common theme across all identified
use cases was connecting the dots across systems and identify impact before something breaks.

So the ontology, if you want to call it that way, really means to connect the dots betwen systems as the reality evoloved
over time. The second part makes the problem hard because, at times, you do need to keep legacy systems alive while 
at the same time support the modern workflow. Then the ontoligy needs to correctly discern when the prior relations apply
and when the new relations are valid. 

In short, the problem reduces to building a temporal aware ontolgoy to connect the dotrs across data and systems
to act upon cross domain knowledge. And this needs to be stores somewere. 

## The Situation

Naturally, a graph database would be a great fit for a central ontology, but the graph database landscape has changed a lot over the last few years. Some projects  use an unfavorable BSL license (ArangoDB, SurrealDB), or simply switched to all commercial (NebulaGraph), and others were abandoned (DGraph). Some new graph databases exist (HydraDB), but most don't support clustering and HA in the OSS version. Only very few do provide clustering in the OSS version, ArcadeDB among them, but their enormous performance
claims (10 million records per second), rests on weak consistency gurantees that causes phantom reads and, in case of a
power loss, a guranteed data loss. For strictly analytical workloads, this is generally accpeted since the primary data
are stored safely in another database. An ontology, though, is generally considered primary data and thus needs strict 
serilaization gurantees to prevant data loss. The other major players are all full commercial (Neo4J, NebulaGraph, TigerGraph etc).  These are all good graph database systems, but they do come at a steep price tag.  

In simple words, there is just no PostgreSQL equivalent graph database.

## The context

At the same time, Frontier AI systems have made significant progress. A small team ported PostgreSQL to Rust using AI agents.
Another team rebuild the entire Adobe Creative Suite in Rust with Ai. And people build new programming languages with Ai.
It still takes real domain expertise to undertake any of those engineering projects, but the capabilities of AI for implementing
complex systems has significantly improved compared to just one year ago.

I gained some meaningful expertise in distributed systems while I was contributing to the Apache Iggy project 
at the time when they designed their high performance clustering architecture. People with experience in distributed systems will quickly point out that it's not one single task to optimize, but rather a multitude of interrelated topics that need to work well
together to meet performacne and reliability goals. And there's real wisdom here because AI agents tend to narrow down optimization goals without necessarily paying attention to the implied trade-off. As a result, performance can be stellar, but when you look at the write durability, you will find serious concerns that may imply data loss under certain circumstances.
And the caveat "certain circumstances" is just a euphemism for saying we don't know when it will happen, but by experience we all 
know it will happen eventually. AI agents are not necessarily bad at writing concurrency or distributed system code. It's just that the way they're prioritizing certain aspects in a way that does not align with what a seasoned engineer would do. 

In this context, the real question is if AI can implement a very specific design for a distributed graph database and will it eventually comply with the appropriate querying standards such as a GQL and will serve data reliably. Complying with standards whenever a predefined outcome is well and detail specified is almost the prime task for AI. Reaching test coverage against pre-existing test oracles is something AI is doing very well. Porting code from one language into another reliably so that the test results replicate bit identical is also something AI is doing well. Therefore, the remaining area that really needs to be settled is: can the resulting database code be robust and reliable in the sense that it does not crash and does not lose data while simultaneously maintaining the expected level of performance?

 From the pgRust project we know that yes, an AI can facefully port a very large C-based code base to Rust. However, this was falling exactly into the primary strength of AI because the PostgreSQL project has an absolutely enormous body of conformance test of very high quality. And whenever you give a frontier AI model an expected outcome, eventually it finds its way to meet the expected outcome. In fact, practitioners have repeatedly observed that a hard constraint such as meeting the criteria set by an external oracle or passing already pre-existing conformance test is a reliable ways to combat AI slop and coerce the AI to write code that actually fulfills its specification. What is not so clear is if you don't have such a comprehensive high quality conformance test, can you still reach a reliable database system that still hits its performance targets?

And there's no real answer to that. 


## The Options

The options on the table are the stereotypical three options:

1) Buy
2) Build
3) Adapt

Buy isn't an option because the budget is simply not there. Build wasn't necessarily the preferred option because out of all systems, building a database is a very meaningful high effort and high risk undertaking. Initially plan A was to adapt
DGraph because it is one of those few systems that provides clustering and HA under a liberal Apache license. 
As initial exploration progressed, 
it became clear that the new owner of DGraph has basically abandoned the project and none of the major bugs will get fixed.
Maintaining a patched fork was categorically rejected after the inspecition of the DGraph source code. The idea of 
porting DGraph to Rust, similar in spirit to the pgRust project, was eventually dropped for the same reason.

## The Decision

The Decision to build UltraGraphDB was made in an afternoon after the domain was registered, and one key assumption that
claude opus 5.5 can figure out data base transactions was settled. The domain name was important because somehow I always 
liked the name UltraGraph (its another graph library I wrote for the DeepCausality project) and at least it setlled one 
of the two hardest problems in computer science: Naming. Hint, caching is not solved. Not yet. 

In retrospect, the decision process still feels as weirdo as it was because, for once, you don't ususally decide
based on domain name availability and even more so, what if Claude was hallicinating and just made up quantum-ether 
transactions that do not exists?

Turned out, domain names are as verifiable as the essence of DB transactions. Instead of asking claude something
I had no idea about, I tested it on somethnign I was absolutely sure about: The Goroutine bottleneck that bound Dgraph
to about 10k tx/sec, often less and regarless of the number of cores. This would be a nice take home assignment for
a graduate level course in databases in a world without Ai. Quizing Claude though was an eye opening experience.
For once, it perfectly pin pointed the issue, but more importantly it also explained well why no amount of more CPU cores would fix anythingt. In fact, claude proposed to fixes, which indeed, once applied gave a handy 30% perfoamcne boost. One
of the patches I would have rejected because it removed a sensible  check, but the other one was actually quite good 
as far as I can tell. Was that sampling enough? No. However, while working on a modern storage compute seperation,
once pointed at the cloned repo of StarRocksDB, Claude could ace this one too. It is obvious that frontier AI
indeed made a leap forward, presumably driven by the arrival of the GB200/300 hardware that brings way larger and way
faster memory to the table. However, it is not so obvious how to get the most of those frontier models and, equally important,
 how to cut slop to an insignificant level. 


## The Slop Mitigation 

I wrote multiple internal memos about reducing slop as early as possible. One of the rather frustrating insights
was that specs, no specs, or bad specs, slop code always occured. The entire premise of spec drivenh development seems
to more driven by building what is said to be done instead of some random one-shot app with little to no concern to 
code maintainability. After having removed an ungodly amount of Ai slop from a larger enterprise mono-repo, I had a
long and deep thought about stopping slop at the generative stage. Then, what exactly do I consider slop:

- Function calls with ignored return value. The typical let _ = .. in rust.
- Tests that tests noting e.g. assert(true) or only test against cherry picked values that hide real bugs
- solutions to made up problem nobody has defined
- sloppy code style e.g. wrong formatting style
- working around an existing constrained when the constrained can be easily be modified 

The last one is a bit harder to spot because the Ai will try to explain it away with "norhinf dowstream breakq" 
or no "migration needed" when, in fact, it re-writes half the code instead of just adding a new parameter
to an existing function that is maybe called twice in the entire system.

It was clear to me that, from day one, I need to have an effective slop mitigation mechnism otherwise
I will drown in Ai slope code and eventually discover the aformentioned  quantum-ether
transactions protocol in a less obvious corner of the code.

Then, what works to eliminate slop early on?

TDD with lint checks.

Did you know that the Rust compile actually has a lint check to ban the usage of unused return values?
If not, that one is your friend against Ai slop. The TDD idea is simple, you write a mock API that only defines
the public signature, then add the full test suite against the public signatures, let all tests fail, and then implement
the public signatures one by one until all tests are green. In larger organizations that take TDD seriously, testing 
and test writing is done by someone else who is not the code author. Applied to Ai, we do need two more steps:

1) Define the public signatures marked with unimplemented
2) Write the test suite according to the TDD style guide e.g. range testing etc
3) Verify the test suite to eliminate ineffective, selective, or tautoligy test
4) Implement the public signatures until tests pass
5) Run mutation testing

It turned out, even if the Ai writes according to a TDD style guide, it still sneaks in tests
that violate the style guide. The verification of test suite before the implementation eliminates
an absurd amount of bugs that would have made it into the implementation becaue the test suite did not rejected those cases.
Then, mutation testing kills whats left. In many cases, its not much left, but when it finds something, it is meaningful.

Enabling the anti-slop-lints, as I call them know, enforces a strict barrier by the Rust compiler. When designated
as "Deny", there is no way out the AI must fix the lint. This process has been proven very effective in killin slop
before it gets commited to git. 