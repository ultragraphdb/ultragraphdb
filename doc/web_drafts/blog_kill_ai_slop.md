# Ai Slop Mitigation

When the decision was madce to write UltraGraphDB, I knew from day one that I need an effective Ai slop mitigation process.

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

## TDD with enforced lints

Did you know that the Rust linter actually has a lint that checks the usage of unused return values?
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
that violate the style guide. Trust is good, verification is better never gets old. 
The verification of test suite before the implementation eliminates
a large amount of bugs that would have made it into the implementation because the test suite did not rejected those cases.
Then, mutation testing kills whats left. In many cases, its not much left, but when it finds something, it is meaningful.


## Tigerstyle with enforced lints

Anyone who is long enough in distributed or database systems has heard of TigerbeetleDB, the financial transaction
database writen in zig. Zig was a very delibrate choice because a simpler language with no hidden control flow and explicit 
memory allocation, by definition, is simpller to review and audit. The evidence supports the thesis becasue when
the Jespen test results came back, the only major finding can only be described as a corner case. This is remarkable
since the Jespen test routinly finds major data losses in other databases with some of them in production for years.
In fairness to the other DB vendors, the kind of bugs the Jespen tests finds are generally hard to reproduce 
and in some cases even hard to produce in the first place; thus some latent bug lingering in an obscure corner of the code
for years may not be as much risk in practice. Howevever, the Jespen test was designed from the ground up to find
exactly this kinf of bug regular fuzzy testing would not find. 

The intersting part is that the team behind Tigerbeetle wrote up its coding style to document what works
to keep code robust and reliable as the repo grows large over time. In their [TigerStyle](https://tigerstyle.dev) , they 
document the importance of three core principles. From the document: 

**Safe**

To be safe, a program must not only run correctly. It must apply defense-in-depth and verify itself while running, to run correctly or else shut down if it detects that it has violated expectations.

**Speed**

Think about performance from the outset. The time to solve performance, and get the 1000x wins, is in the design phase, when you can't profile. It's hard to fix a system after implementation, and the gains are less. Have mechanical sympathy.

**Scale**

A day of design is worth weeks or months in production. Therefore, go slow to go fast. Optimize the total cost of software ownership, not for those who write it once, but for those who read and run it many times. Trade linear deadlines for exponential quality.

UltraGraphDB adopted the TigerStile with minor modifications of the coding style to account for the difference
in memory management that applies to Rust. The rules are now tracked in the repo as UltraStyle and may evolve over time
as practical experience may shape deeper insights. Ai agents are explictily asked to adhere to the UltraStyle, check
writen code against the UltraStyle and, because agents are only selective in what they apply, use the Rust linter to
enforce as much as possible throught the Rust linter.

## Anti-Slop-Lints

The rust linter has a suprisngly large number of lints that are perfectly made for combating Ai Slop. Among them,
- unwrap_used (Unrwrap is a bad idea in prod code)
- panic (even less wanted in prod code)
- let_underscore_must_use (the classic)
- unused_results (yes, there is a lint for it)
- large_types_passed_by_value (should be by reference unless for a justified exception)
- unsafe_code (If you don't use FFI, this makes sense to check)
- too_many_lines (Line level in UltraGraph is set to 70)

There are several more of them, see the UltraGraphDB config for details. Some of them are genuily useful to combat
Ai slop (unused_results) and others are style decisions that apply to specific requirements. Unsafe, for example,
is enormously useful when writing drivers, working with FFI, or simnilar use cases. For UltraGraphDB, that is not the case,
at lest for now, hence unsafe is denied. In another project, a different config may work better.

Enabling the anti-slop-lints, as I call them, enforces a strict barrier by the Rust compiler. When designated
as "Deny", there is no way out the AI must fix the lint. This process has been proven effective in killing the
obvious slop before it gets commited to git.

Practically, I graudally enabled one lint after the other since running them all caused to many reported issues.
After the majority was enables, over 15 thousand issues were fixed, and a small number of justified exceptions
were documented. Some lints still needs to be enabled, but they will need meanginful effort to fix, hence its not done yet.
For those into statistcs, that works out to about 5% of obvious slop killed by the linter. For those
into mathamatics, feel free to calculate the total lines of Rust code from which the percentage was calcuated. I will say
though that adding the anti-slop lints, regarless of which one you pick, as early as possible to a project does make
a difference. 