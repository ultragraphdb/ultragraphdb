read @AGENTS.md

plz review PR: GH link ...

Ignore everything under openspec and only review Rustcode, dosctring, and crate configutrations and crate level README.md files. 

Ignore everything MIRI related. 

For larger reviews (> 50 files) or multiple crates, you may start multiple review agents that
use these instructions plus your guidance. 

Look out for
- Oblivious bugs
- Correctness bugs 
- Logic bugs 
- Tests that falsey assert correct results by tautlogy
- Tests that would silently fail or fail to report a failure 
- Poor performance choices in the implementation
- Abstraction leaks in the implementation
- Serious code quality issues. 
- Any major or minor security issues.

if you find any issues, diagnose,identify  the root cause,  and derive a proper solution.

Then report all consolidated findings and let me decide which to resolve.

Note, if there are no major issues found, say so.