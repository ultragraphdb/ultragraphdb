## MODIFIED Requirements

### Requirement: Each role runs one runtime per core
Each role SHALL run one compio runtime per core it uses, a storage partition on its own, sharing no mutable state with
another; what crosses between them SHALL go by message. `[runtime] pin_cores` SHALL ask each runtime to bind to one CPU,
chosen from those the process is permitted, as a container's cpuset restricts them. Pinning SHALL be a performance choice
and never a condition of correctness: where the platform cannot bind a thread, as macOS cannot, offering only a
cache-locality hint, the role SHALL run unpinned and say so. The frontend SHALL accept and serve clients on each of its runtimes,
one per CPU it may use, on one client address bound once: its role address, its connections back and its readiness on the first,
each role that joins made known to every one, each dialling its own connections to the other roles, and its throttle of failed
authentications one for all of them, so a principal is refused from every source after the same failures whatever cores they
reached (`SECURITY.md`).
At start each role SHALL log, per runtime, the CPU it asked for and what it has, read back from the operating system where
it can be.

#### Scenario: A pinned role reports what it holds
- **WHEN** a role starts with `pin_cores = true` on a platform that allows binding
- **THEN** it logs each runtime's CPU, read back from the operating system, each within the permitted set

#### Scenario: A platform refusing to pin still serves
- **WHEN** a role starts with `pin_cores = true` where binding is unavailable
- **THEN** it logs that it runs unpinned, and serves requests

#### Scenario: The frontend serves clients on every core
- **WHEN** 32 clients each commit a write over a connection of their own to a frontend of two partitions
- **THEN** every write is committed, the frontend logs a runtime per CPU, and more than one of its cores relays the writes
