The folder and crates were restructured, which broke the Bazel build.
plz fix this first.

 >> The same Go helpers are copied into five crates 

creste a shared
ultragraphdb_common_utils

and move those helpers there except for errors. 
For errors, put dedicated crates containing only shared errors under folder ultragraphdb_error/

This follows the layout of the scrith repo, which puts errors and traits into dedicated crates so they can be easily shared across crates without adding heavy dependencies. 

Likewise, for the Collator trait and other traits used across crates, move them into dedicated crates under

ultragraphdb_traits. 

>> The chunker's error texts differ from Dgraph's in three places:
>> JSON syntax errors carry serde's wording.
>> serde stops nesting at 128 levels where Go allows 10,000.
>> Error columns after an invalid UTF-8 byte are off.

Do a a hand-written JSON scanner to match the GO reference 

>> pre-existing: 22 files in ultragraphdb_client's test utilities have no header.  

Add proper headers to all files. 

>> Two places where the port refuses instead of panicking, which no spec declares. Ledger #19 
 
In general, panicks should not occur; Instead, see if these can be converted to Result types and return a clear error. 


>> NumError 

IN genreal, errors should follow the pattern of a publi struct wrapping an error enum type. 

See
 ../deep_causality/deep_causality_physics/src/error/

for a refemce. 

>> Dgraph bugs found while porting

@openspec/notes/dgraph-bugs-found-in-port.md 

Determine which bug can be made unrepresentable in Rust, and then suggst
a targeted refactoring to ensure it works, 

For those errors that cannot be eliminated by making them unrepresentable, 
then determine the correct behavior and propose a fix.

Last, list remaining errors that can  neither be fixed by changing representaiton nor
by identifying the correct behavior and let me reviee those.

