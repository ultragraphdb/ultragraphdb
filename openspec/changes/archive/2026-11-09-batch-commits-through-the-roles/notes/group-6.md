# Group 6: measurements

Run on 2026-11-09, 06:55 to 07:58 UTC, on this Mac (macOS 27.2, Apple silicon, 16 CPUs, the internal SSD), the host's 1-minute
load 3.5 to 13.8 through the series. The scripts and every run's JSON line and phase logs are in the session's scratchpad, `m6/`
(6.1, 6.4) and `k6/` (6.2).

## The machines

- **6.1, a `container` Linux machine:** Wolfi (`cgr.dev/chainguard/wolfi-base`), 12 vCPUs and 12 GiB, Linux 6.18.56 aarch64, every
  data directory on a named volume, ext4 on the virtual disk `vdc`. `container` 1.7.1 gives a machine one CPU more than `--cpus`
  asks for (`--cpus 4` shows 5, `--cpus 12` shows 13), so the machine ran with `--cpus 11`; `make cluster-local` now asks for one
  fewer than `CPUS`.
- **6.2, the `container` cluster:** `make cluster-local CPUS=12 MEMORY=16g PARTITIONS=<p>`, one node of 12 CPUs, Kubernetes
  v1.35.5, every role on io_uring under the shipped seccomp profile, the bench as a Job; a fresh cluster per partition count.
- **Builds:** `-c opt` for `aarch64-unknown-linux-gnu` from `68a4f5d5` (sha256 prefixes `05e3fee53bc0 client_bench`, `2652226c6e07 ultragraphdb_compute`, `b3a90fb0b58f ultragraphdb_config`, `7e234e7b0ce0 ultragraphdb_frontend`, `1a73898d8cf0 ultragraphdb_storage`, `38fb46b37276 ultragraphdb_supervisor`, `3f61257aba84 wal_bench`); the cluster's images from the
  same tree.
- **Layout (6.1):** 12.5b's, as processes: the supervisor over the `dev` profile, the roles over Unix sockets, the client endpoint
  on `tcp://127.0.0.1:7080`, epochs 2 to 10 ms, unpinned, `client_bench` on the loopback; a fresh data directory per cluster, one
  cluster per partition count; three interleaved 20 s repetitions, 3 s apart so each run is a phase window of its own.
- **The machine's own in-process rate:** `wal_bench` as 12.5's series (30 s, an offered 50,000 commits a second per partition, no
  crossing edges, a fresh directory per run), three repetitions interleaved, relations a second:

| Partitions | 1 relation a commit | 10 relations a commit | Commits a sync |
|---|---|---|---|
| 1 | 14,506 (14,205–16,052) | 22,497 (14,316–22,878) | 128 / 64 |
| 4 | 73,478 (43,901–77,924) | 102,302 (85,205–103,494) | 128 / 64 |
| 12 | 127,695 (98,762–133,213) | 152,269 (138,185–173,701) | 128 / 64 |

  At 12 partitions the note records 124,057 to 129,360 and 179,162 to 203,082 on the OrbStack VM: one relation a commit within it,
  ten a little below.

## 6.1 Through the binaries on the `container` machine

Medians of three, the range in parentheses; latencies the client's; commits a sync from the partitions' `commit phases` (commits
over log syncs, summed over partitions); the log's sync, the core's stay and the frontend's checks commit-weighted means of the
windows; the share is relations a second over the in-process rate at the same partitions and relations a commit.

| Partitions | Request | Relations | Connections | Commits/s | Relations/s | p50 / p99 ms | Commits a sync | Log sync p50 µs | Core stay µs | Frontend checks µs | Share of in-process (this machine) | Share of the note |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | direct | 1 | 1 | 2,330 (1,684–2,669) | 2,330 (1,684–2,669) | 0.39 / 1.14 | 1.0 (1.0–1.0) | 115 (111–135) | 294 (249–446) | 48 (43–50) | 16% | 20% (20%–20%) |
| 1 | direct | 1 | 16 | 6,969 (3,908–9,190) | 6,969 (3,908–9,190) | 1.90 / 5.69 | 6.4 (6.2–6.6) | 143 (135–151) | 920 (729–1,628) | 62 (57–70) | 48% | 59% (59%–59%) |
| 1 | direct | 1 | 64 | 5,509 (4,366–9,431) | 5,509 (4,366–9,431) | 11.42 / 19.85 | 23.2 (22.4–23.4) | 175 (159–199) | 3,688 (2,465–4,988) | 99 (70–125) | 38% | 46% (46%–46%) |
| 1 | transaction | 1 | 16 | 4,067 (3,953–7,173) | 4,067 (3,953–7,173) | 3.72 / 5.91 | 3.9 (3.8–3.9) | 151 (143–151) | 722 (553–722) | — | 28% | 34% (34%–34%) |
| 4 | direct | 1 | 1 | 2,498 (2,281–2,559) | 2,498 (2,281–2,559) | 0.36 / 1.06 | 1.0 (1.0–1.0) | 114 (114–114) | 269 (268–310) | 47 (44–48) | 3% | — |
| 4 | direct | 1 | 16 | 9,239 (7,832–11,387) | 9,239 (7,832–11,387) | 1.28 / 6.08 | 1.9 (1.9–2.0) | 267 (259–287) | 787 (628–964) | 129 (128–144) | 13% | — |
| 4 | direct | 1 | 64 | 14,768 (14,060–19,138) | 14,768 (14,060–19,138) | 3.02 / 18.35 | 5.9 (5.6–5.9) | 319 (271–327) | 1,737 (1,245–1,834) | 334 (248–358) | 20% | — |
| 4 | transaction | 1 | 16 | 6,552 (6,549–6,606) | 6,552 (6,549–6,606) | 1.97 / 8.78 | 1.3 (1.3–1.3) | 253 (247–253) | 681 (671–681) | — | 9% | — |
| 12 | direct | 1 | 64 | 12,894 (11,887–13,128) | 12,894 (11,887–13,128) | 4.29 / 15.29 | 2.5 (2.4–2.5) | 882 (858–972) | 2,259 (2,111–2,417) | 612 (608–613) | 10% | 10% (10%–10%) |
| 12 | direct | 1 | 256 | 24,655 (23,138–26,566) | 24,655 (23,138–26,566) | 9.16 / 28.71 | 6.8 (6.6–6.8) | 1,375 (1,364–1,498) | 3,582 (3,278–3,754) | 1,637 (1,416–1,820) | 19% | 20% (19%–20%) |
| 12 | direct | 10 | 64 | 6,098 (5,615–6,417) | 60,983 (56,152–64,166) | 8.63 / 60.19 | 2.3 (2.3–2.4) | 1,428 (1,407–1,498) | 4,381 (4,378–4,748) | 1,570 (1,134–1,749) | 40% | 33% (30%–34%) |
| 12 | direct | 10 | 256 | 12,056 (8,473–12,445) | 120,557 (84,728–124,452) | 17.99 / 106.50 | 6.4 (6.4–6.4) | 1,162 (1,124–1,162) | 6,667 (6,553–6,667) | 3,822 (3,416–3,822) | 79% | 65% (59%–67%) |

**Acceptance (design D9): met.** One partition at 16 connections commits 6,969 a second (3,908 to 9,190) at 6.4 commits a sync,
against the note's 287 (205 to 1,123) at one sync a commit; every run at 16 connections and above syncs fewer times than it commits.
Four partitions at 64 connections take 14,768 a second (3,281 in the note), twelve at 256 connections and ten relations 120,557
relations a second. No run refused a commit or left one in doubt.

## 6.2 Through the bench Job on the `container` cluster

| Partitions | Request | Relations | Connections | Commits/s | Relations/s | p50 / p99 ms | Commits a sync | Log sync p50 µs | Core stay µs | Frontend checks µs | Share of in-process (this machine) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | direct | 1 | 1 | 1,517 (1,444–1,528) | 1,517 (1,444–1,528) | 0.50 / 1.13 | 1.0 (1.0–1.0) | 231 (223–235) | 458 (447–524) | 61 (61–61) | 10% |
| 1 | direct | 1 | 16 | 8,505 (3,310–8,698) | 8,505 (3,310–8,698) | 1.70 / 3.27 | 7.2 (7.2–7.4) | 271 (239–287) | 811 (757–1,841) | 86 (85–97) | 59% |
| 1 | direct | 1 | 64 | 5,953 (5,536–11,591) | 5,953 (5,536–11,591) | 11.16 / 21.49 | 24.1 (23.9–24.3) | 367 (335–415) | 3,293 (1,947–3,589) | 109 (107–123) | 41% |
| 1 | transaction | 1 | 16 | 3,651 (3,634–4,438) | 3,651 (3,634–4,438) | 4.14 / 7.27 | 3.8 (3.8–3.8) | 271 (255–271) | 825 (823–869) | — | 25% |
| 4 | direct | 1 | 1 | 1,746 (975–1,788) | 1,746 (975–1,788) | 0.52 / 1.16 | 1.0 (1.0–1.0) | 229 (216–231) | 405 (396–863) | 63 (60–63) | 2% |
| 4 | direct | 1 | 16 | 3,641 (2,932–3,756) | 3,641 (2,932–3,756) | 1.34 / 23.56 | 2.0 (2.0–2.0) | 367 (367–383) | 2,099 (2,033–2,829) | — | 5% |
| 4 | direct | 1 | 64 | 7,537 (7,421–8,681) | 7,537 (7,421–8,681) | 2.79 / 41.31 | 5.9 (5.9–6.0) | 451 (447–463) | 3,331 (2,847–3,431) | 669 (669–669) | 10% |
| 4 | transaction | 1 | 16 | 2,046 (1,729–2,236) | 2,046 (1,729–2,236) | 2.22 / 41.30 | 1.3 (1.3–1.3) | 367 (351–383) | 2,327 (2,143–2,888) | — | 3% |
| 12 | direct | 1 | 64 | 12,941 (7,261–13,091) | 12,941 (7,261–13,091) | 3.30 / 15.79 | 2.3 (2.3–2.4) | 868 (863–959) | 2,083 (1,834–3,977) | 734 (558–934) | 10% |
| 12 | direct | 1 | 256 | 20,013 (13,600–20,893) | 20,013 (13,600–20,893) | 9.52 / 69.37 | 4.1 (3.9–5.2) | 1,124 (1,015–1,156) | 2,826 (2,236–4,374) | 1,876 (1,419–3,702) | 16% |
| 12 | direct | 10 | 64 | 7,097 (5,405–8,967) | 70,968 (54,048–89,675) | 7.86 / 39.38 | 2.3 (2.3–2.3) | 1,514 (1,151–1,722) | 3,845 (2,921–4,743) | 856 (594–1,868) | 47% |
| 12 | direct | 10 | 256 | 9,157 (8,686–12,106) | 91,566 (86,856–121,058) | 22.50 / 118.94 | 6.2 (5.7–6.5) | 2,346 (2,015–2,421) | 8,476 (6,555–9,477) | 3,640 (2,195–5,298) | 60% |

The cluster follows the machine at one partition and at twelve, within the spread. At four partitions it ran two to three times
slower than the machine (3,641 a second at 16 connections, p99 23.6 ms against 6.1 ms), its frontend's and storage's windows
splitting over their cores so no window matches a run (the `—` and the residuals left out below). Not chased further: the host's load
was 10.7 as that cluster's series began, and the figures that matter are 6.4's.

## 6.3 The reply path (design D10)

The residual is the frontend's relay-to-answer less the partition's read-to-reply: the two transits and the time a reply waits to be
read at the frontend. Its p50 and p99 are bucket bounds of the logs' histograms, powers of two.

| Partitions | Request | Relations | Connections | Commits a sync | Relay µs | Partition read-to-answer µs | Residual p50 / p99 µs |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | direct | 1 | 1 | 1.0 (1.0–1.0) | 335 (287–490) | 298 (253–451) | 32 (32–48) / 64 (32–64) |
| 1 | direct | 1 | 16 | 6.4 (6.2–6.6) | 2,152 (1,618–3,962) | 1,849 (1,397–3,566) | 256 (192–384) / 1,024 (256–1,536) |
| 1 | direct | 1 | 64 | 23.2 (22.4–23.4) | 11,477 (6,621–14,453) | 9,099 (5,304–11,814) | 2,048 (1,152–2,560) / 5,120 (3,072–5,120) |
| 1 | transaction | 1 | 16 | 3.9 (3.8–3.9) | — | 1,071 (550–1,071) | — / — |
| 4 | direct | 1 | 1 | 1.0 (1.0–1.0) | 309 (306–348) | 273 (272–314) | 32 (28–32) / 49 (40–111) |
| 4 | direct | 1 | 16 | 1.9 (1.9–2.0) | 1,524 (1,196–1,823) | 1,394 (1,083–1,704) | 112 (88–128) / 192 (64–256) |
| 4 | direct | 1 | 64 | 5.9 (5.6–5.9) | 3,869 (2,985–4,110) | 3,352 (2,478–3,533) | 512 (448–544) / 1,665 (1,024–1,793) |
| 4 | transaction | 1 | 16 | 1.3 (1.3–1.3) | — | 631 (625–631) | — / — |
| 12 | direct | 1 | 64 | 2.5 (2.4–2.5) | 4,162 (4,059–4,570) | 3,755 (3,579–4,106) | 459 (459–491) / 725 (427–853) |
| 12 | direct | 1 | 256 | 6.8 (6.6–6.8) | 8,248 (7,713–8,724) | 6,265 (5,704–6,533) | 1,899 (1,878–2,069) / 2,563 (1,921–4,611) |
| 12 | direct | 10 | 64 | 2.3 (2.3–2.4) | 8,550 (8,480–9,271) | 7,868 (7,778–8,313) | 533 (468–533) / 9,810 (1,195–21,927) |
| 12 | direct | 10 | 256 | 6.4 (6.4–6.4) | 15,997 (15,761–15,997) | 13,229 (13,035–13,229) | 2,261 (2,134–2,261) / 29,023 (27,635–29,023) |

On the cluster:

| Partitions | Request | Relations | Connections | Commits a sync | Relay µs | Partition read-to-answer µs | Residual p50 / p99 µs |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | direct | 1 | 1 | 1.0 (1.0–1.0) | 575 (575–575) | 461 (450–528) | 64 (64–64) / 64 (64–64) |
| 1 | direct | 1 | 16 | 7.2 (7.2–7.4) | 1,721 (1,686–4,659) | 1,567 (1,512–4,260) | 128 (128–384) / 384 (384–1,536) |
| 1 | direct | 1 | 64 | 24.1 (23.9–24.3) | 10,574 (5,327–11,384) | 8,574 (4,308–9,333) | 2,048 (1,152–2,048) / 3,072 (1,536–4,608) |
| 1 | transaction | 1 | 16 | 3.8 (3.8–3.8) | — | 1,189 (957–1,195) | — / — |
| 4 | direct | 1 | 1 | 1.0 (1.0–1.0) | 456 (445–456) | 409 (399–866) | 48 (32–48) / 72 (56–72) |
| 4 | direct | 1 | 16 | 2.0 (2.0–2.0) | — | 4,815 (4,035–5,400) | — / — |
| 4 | direct | 1 | 64 | 5.9 (5.9–6.0) | 7,701 (7,701–7,701) | 6,795 (6,181–7,160) | 560 (560–560) / -22 (-22–-22) |
| 4 | transaction | 1 | 16 | 1.3 (1.3–1.3) | — | 2,514 (2,118–2,811) | — / — |
| 12 | direct | 1 | 64 | 2.3 (2.3–2.4) | 4,143 (3,855–7,661) | 3,500 (3,133–7,153) | 512 (512–619) / 1,153 (-333–1,859) |
| 12 | direct | 1 | 256 | 4.1 (3.9–5.2) | 10,092 (9,966–14,483) | 5,628 (4,344–11,201) | 4,245 (4,096–4,608) / 21,548 (2,398–22,687) |
| 12 | direct | 10 | 64 | 2.3 (2.3–2.3) | 8,020 (5,922–9,369) | 7,032 (5,251–8,179) | 683 (661–810) / 5,875 (1,748–15,954) |
| 12 | direct | 10 | 256 | 6.2 (5.7–6.5) | 22,317 (16,265–24,226) | 17,306 (12,997–20,106) | 2,817 (2,560–4,438) / 32,592 (30,344–43,849) |


**The reply path does not keep up.** At one commit a round the residual is 32 to 64 µs at the median; it grows with the round and
with the frontend's load: 256 µs at 6.4 commits a sync, 2 ms at 23 (one partition), 1.9 ms at 6.8 commits a sync and 12 partitions,
and its p99 reaches 29 ms. The burst test of 3.4 read 1,000 answers at 28 µs each through one connection. The cause is in 6.4: the
frontend's one endpoint runtime, which reads the replies, is the busiest thread of the cluster. Per design D10 this goes to the
owner before any change to the frontend.

## 6.4 The shares far below the in-process rate, chased

Three causes, each measured.

**1. The data a cluster keeps.** 6.1 kept one cluster per partition count through all its runs, so a run started over everything the
runs before it wrote, while `wal_bench` starts every run on a fresh directory. At one partition the rate falls with what is stored:
64 connections commit 9,431 a second over 237,202 stored relations, 4,366 over 755,416 and 5,509 over 1,033,736 (the core's stay
2.5, 5.0 and 3.7 ms). That is 12.5's finding, `referrer_candidates` reading each hub's growing referrer set (group 7, 7.2). At 12
partitions, where each holds a twelfth, the rate holds over 10 million stored relations.

**2. A fresh cluster per point** (one 20 s run each, the same machine):

| Partitions | Relations | Connections | Commits/s | Relations/s | p50 ms | Commits a sync | Core stay µs | Log sync µs | Frontend checks µs | Relay µs | Share of in-process (this machine) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | 1 | 16 | 9,915 | 9,915 | 1.47 | 6.6 | 679 | 174 | 74 | 1,479 | 68% |
| 1 | 1 | 64 | 14,005 | 14,005 | 4.27 | 23.0 | 1,555 | 215 | 77 | 4,429 | 97% |
| 1 | 1 | 256 | 15,371 | 15,371 | 14.86 | 64.3 | 3,621 | 271 | 125 | 16,458 | 106% |
| 12 | 1 | 256 | 25,166 | 25,166 | 9.17 | 6.3 | 3,295 | 1,833 | 1,494 | 8,118 | 20% |
| 12 | 1 | 1,024 | 36,140 | 36,140 | 27.39 | 8.7 | 3,345 | 1,682 | 2,995 | 20,140 | 28% |
| 12 | 10 | 256 | 14,143 | 141,430 | 16.23 | 6.4 | 5,763 | 1,778 | 2,723 | 14,004 | 93% |
| 12 | 10 | 1,024 | 18,570 | 185,696 | 48.42 | 12.7 | 10,477 | 2,083 | 8,684 | 34,813 | 122% |

One partition through the binaries reaches the in-process rate at 64 connections and passes it at 256; twelve partitions at ten
relations a commit reach 185,696 relations a second, the note's 12-partition figure (185,630). The roles cost the commit path
nothing measurable there. Twelve partitions at one relation a commit stop at 36,140 commits a second, 28%, at 8.7 commits a sync
however many connections offer them, while the core stays 3.3 ms and the frontend's relay takes 20 ms.

**3. The frontend's one endpoint runtime.** Not the client: 1,024 connections split over 1, 2, 4 and 8 `client_bench` processes commit
37,381, 38,858, 36,448 and 36,759 a second against one fresh 12-partition cluster. Each thread's time on a CPU over 2 s of the timed
part (the scheduler's `schedstat`, 12 partitions, 1 relation, 1,024 connections, 33,649 commits a second):

| Thread | On a CPU |
|---|---|
| frontend, its main runtime (the endpoint: every client's frames, the relays, the replies) | 98.8% |
| each of the 12 partition cores | 22.3 to 23.1% |
| frontend-owner | 10.2% |
| each io_uring worker of storage | 1.3% |

The frontend serves every client on one compio runtime: `frontend_role` builds one `EndpointCore` and accepts clients on the
runtime it starts on, beside the owner's thread, so about 27 µs of that thread per one-frame write caps the cluster near 37,000
single-relation commits a second whatever the partitions or the clients. `role-binaries` states that each role runs one runtime
per core it uses; the frontend uses one for its whole client path. The reply path's residual (6.3) and the checks' growth with load
(74 µs at 16 connections to 3 ms at 1,024, the owner's asks meeting the epoch close's sync, 7.5) are measured on that thread.
**Brought to the owner, not fixed here:** spreading the frontend's endpoint over its cores is a change of the frontend's design (an
endpoint core per runtime on one shared listener, as compute's `CoreListener` does, each with its role links, so each partition takes
one role connection per frontend core), and design D10 asks for the owner's word before the frontend changes.

Not a cause: the client's logins. `client_bench` logs every connection in with SCRAM at 4,096 iterations on its one thread before
the timed part, about 10 s for 1,024 connections, during which the roles idle; a measurement of the threads must start after them.

## 6.6 and 6.7 The frontend on every core

Design D14, built: `CoreListener` moved to `ultragraphdb_common_service`; the frontend starts an endpoint core per CPU, core 0 keeping
the role listener, the connections back and readiness; a `Join` reaches every core through `JoinFanOut` and `CoreJoins`; each core
holds its own map, links, gate and phases, logged with its core; every core stops accepting on a stop; the authentication throttle
is one `Arc<Mutex>` for every core. Tests: the fan-out reaching every core's links, the throttle shared across clones; the scenario
*The frontend serves clients on every core* (`ultragraphdb_tests`): 32 clients over connections of their own, every write committed,
a runtime per CPU logged, the writes relayed by more than one core. `make test`: doc 21, unit 79, BDD 37, integration 7, all pass.
The shared throttle departs from `role-binaries`' "sharing no mutable state" and from the preference for lock-free state; the
owner has not yet decided between it and asking the owner thread per login.

## 6.8 Like for like (design D14)

2026-11-09, 08:42 to 09:10 UTC, host load 5.3 to 10.0. Three `container` machines over the network (192.168.64.0/24): storage alone
on 6 CPUs and 8 GiB, its data on an ext4 volume; the config service, the frontend and compute on 6 CPUs; the clients on 2 CPUs, the
connections split over 2 or 4 `client_bench` processes. Every role started from its five bootstrap variables, over TCP, epochs 2 to
10 ms, 6 partitions, a fresh cluster per run; the frontend before (6.1's binary, one endpoint core) and after (every core) interleaved,
three 20 s runs each. The target is `wal_bench` at 6 partitions on the storage machine itself: 86,906 (85,463–89,067) relations a
second at one relation a commit, 114,993 (107,576–120,315) at ten.

| Frontend | Relations | Connections | Commits/s | Relations/s | p50 ms | Share of in-process | Commits a sync | Log sync p50 µs | Core stay µs | Read-to-answer µs | Frontend checks µs | Relay µs | Cores relaying |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| before (one core) | 1 | 256 | 32,694 (26,726–33,245) | 32,694 (26,726–33,245) | 7.1 | 38% | 13.5 (7.3–13.8) | 665 (532–750) | 2,645 (2,138–2,703) | 5,553 (4,534–5,606) | 163 (158–323) | 7,207 (7,058–8,752) | 1 |
| after (every core) | 1 | 256 | 32,658 (32,362–33,670) | 32,658 (32,362–33,670) | 6.7 | 38% | 13.7 (13.0–13.8) | 745 (692–745) | 2,742 (2,657–2,759) | 5,683 (5,534–5,698) | 264 (263–275) | 6,846 (6,718–6,927) | 5 |
| before (one core) | 1 | 1,024 | 38,977 (34,107–39,925) | 38,977 (34,107–39,925) | 22.9 | 45% | 17.8 (15.9–22.5) | 521 (487–553) | 5,067 (4,639–5,159) | 11,092 (10,504–11,870) | 733 (701–793) | 21,779 (21,201–25,276) | 1 |
| after (every core) | 1 | 1,024 | 41,515 (40,306–41,865) | 41,515 (40,306–41,865) | 14.9 | 48% | 28.8 (24.0–29.0) | 572 (559–618) | 6,367 (6,022–6,368) | 14,582 (13,863–14,614) | 611 (569–626) | 20,282 (20,168–20,893) | 5 |
| before (one core) | 10 | 256 | 10,176 (8,675–10,248) | 101,760 (86,750–102,480) | 17.1 | 88% | 8.6 (7.1–9.3) | 353 (321–364) | 7,836 (7,676–8,758) | 19,293 (19,112–21,944) | 212 (211–212) | 23,530 (23,326–27,984) | 1 |
| after (every core) | 10 | 256 | 9,939 (9,863–10,027) | 99,390 (98,630–100,270) | 18.0 | 86% | 8.8 (8.6–9.2) | 364 (356–385) | 8,073 (8,043–8,091) | 19,784 (19,731–19,787) | 339 (272–346) | 23,637 (23,599–23,868) | 5 |
| before (one core) | 10 | 1,024 | 10,635 (10,081–12,393) | 106,350 (100,810–123,930) | 68.8 | 92% | 19.8 (16.7–20.6) | 359 (342–361) | 24,623 (20,945–25,134) | 64,101 (54,349–67,073) | 507 (447–866) | 92,107 (74,391–97,513) | 1 |
| after (every core) | 10 | 1,024 | 10,679 (10,567–10,882) | 106,790 (105,670–108,820) | 62.8 | 93% | 22.8 (20.7–23.9) | 377 (372–385) | 26,186 (25,572–27,027) | 67,626 (67,242–69,925) | 662 (647–671) | 90,195 (89,060–91,389) | 5 |

(The client lines are each run's own: a first pass of the analysis also summed lines an earlier run left on the client machine,
caught because storage's own count of commits did not match, and the runner now clears them.)

- **Ten relations a commit reach 86 to 93% of the in-process rate**, with either frontend.
- **One relation a commit reaches 38 to 48%.** The frontend on every core lifts 1,024 connections from 38,977 (34,107–39,925) to
  41,515 (40,306–41,865) commits a second, outside the spread, and their median latency from 22.9 to 14.9 ms; at 256 connections
  nothing changes. The old frontend was near its one thread's cap here too; the next bound was right behind it.
- **No role is busy.** Each thread's time on a CPU over 4 s of a 1,024-connection run at 38,600 commits a second: storage's partition
  threads 363% and fjall's workers 34% of its 600%; the frontend's endpoint cores 155%, its main runtime 33% and its owner 28% of 600%.
- **Where a commit waits.** At 1,024 connections a request spends 14.6 ms in storage from its read to its answer, of which its core
  holds it 6.4 ms; about 340 requests wait ahead of the six cores. Rounds hold 29 commits. Over the run's windows the six cores spent
  15.3 s writing the log and 27.1 s syncing it of about 120 s: a round's write about 455 µs and its sync about 805 µs, some 1.3 ms of
  a 3.6 ms round, beside about 2.3 ms of the core's own work. The core's work and its log's I/O take turns.

### The profiles

`ultragraphdb_storage_profile` and `ultragraphdb_frontend_profile` (new, a symbol build as storage's) on Debian machines of the same
sizes with `perf` 6.12, `cpu-clock:u` at 999 Hz with user call chains over 10 s of a 1,024-connection run; `perf_event_paranoid` is 2
and `/proc/sys` read-only in a `container` machine, so `perf` ran as the roles' user, user space only.

- **Storage: 24.5% of its samples are xxhash's `hash_long_internal_loop`**, 31% of each partition thread's: `step_one_frame` →
  `changeset_write_triples` → `write_terms::iri` → `dictionary::held_id` → `FjallStore::get_versionless` → `Keyspace::get` →
  `Table::get` → `load_block` → `Block::from_file`, a block read from its table and checksummed whole. Each new term of a write is
  looked up in the terms keyspace, and the bench's terms are new. fjall's defaults give each store one 32 MiB block cache and pin
  filter blocks at level 0 alone, index blocks at levels 0 and 1, so a deeper table's filter or index block is loaded and checksummed
  once evicted. Which kind of block is loaded is not yet counted. `wal_bench` runs the same store, so the in-process rate pays it too.
  fjall's workers, flushing and compacting, take 25%; `memcmp` in the skip list and key comparisons about 9%.
- **Frontend:** the five endpoint runtimes take 17 to 18% of its samples each and its owner 12%; no symbol stands out. Its timers,
  `TimerRuntime::poll_timer` and the B-tree under it, take about 4.5%, as a timer is armed per request; std's SipHash in
  `ultragraphdb_protocol_wire` 2.5%; `session_principal` on the owner 2.3%; allocation about 3%.

## 6.10 Failed logins: throttled, then banned, in the owner (design D15)

The owner's decision replaced the throttle split per core in 6.6 and the lockout per principal. `AuthThrottle` is now the owner's
plain table by source, an IPv4 address or an IPv6 /64; `LoginLimits` holds `[endpoint] auth_failures_before_throttle` (50),
`auth_throttled_failures_before_ban` (20) and `auth_ban_ms` (24 hours), each refused at 0 by the settings' check;
`refusal_interval_ms` and `EndpointLimits`' refusal interval are gone. The login messages are one `Asked::Login(LoginAsked)`: the
verifier's ask carries the source and answers the delay, each outcome is told without an answer, each accepted connection asks
whether its source is admitted, on its own task, and a banned one is closed before TLS or any frame. A ban is logged to the audit
target. `PROTOCOL.md` §11.9's table, the authentication step and `SECURITY.md`'s *Failures* state it.

Tests: the throttle's six (no wait within the threshold, a doubling one past it, the ban and its end, a success forgetting, an IPv6
/64 as one source, a full table keeping its bans, the defaults); a session test where two failures with limits of 1 and 1 ban the
source, its next connection closes before any frame, and it authenticates once the ban has ended; the settings' check refusing 0
for each of the three. `make test`: doc 21, unit 79, BDD 37, integration 7, all pass.

## fjall's block cache and filter pinning: dropped

The owner's rule: a quick test, dropped under 2%. `wal_bench` at 6 partitions, one relation a commit, 30 s, on the storage machine
(Debian, 6 CPUs), three runs each interleaved: fjall's defaults 72,497 (72,271–77,306) relations a second; every keyspace's filter
and index blocks pinned at every level and a block cache of 256 MiB in place of 32 MiB, 73,157 (72,303–78,732): +0.9%, within the
spread. The change was built for the test alone and is not kept; the storage engine is replaced at roadmap stage 6.5.

## 6.11 The next round during the sync: measured and not kept (design D16)

Built as D16 states: `OrderedLog::detach_sync` and `attach_sync`, the WAL's `WalSynced`, the pipeline's and the host's, and
`PartitionCore::run` waiting on its sync and its inbox together; the tests passed, among them a read answered while a round's sync
was held. Measured as 6.8, the 6/6/2 layout, fresh cluster a run, three runs a point interleaved, medians with their ranges; the
storage binary the only difference.

The first version (the core attaching a finished sync between turns), against inline syncs:

| Relations | Connections | Inline: commits/s | Overlap: commits/s | Change | Commits a sync | Log sync p50 µs | p50 ms |
|---|---|---|---|---|---|---|---|
| 1 | 256 | 29,211 (28,002–32,058) | 26,862 (24,637–27,534) | −8% | 14.3 → 9.8 | 745 → 1,832 | 7.9 → 5.9 |
| 1 | 1,024 | 39,106 (38,644–39,510) | 32,683 (32,312–33,650) | −16% | 36.3 → 15.3 | 681 → 1,875 | 17.5 → 11.5 |
| 10 | 256 | 8,607 (8,373–8,938) | 7,918 (7,412–8,203) | −8% | 9.9 → 5.7 | 465 → 2,759 | 20.5 → 10.2 |
| 10 | 1,024 | 9,368 (8,809–10,434) | 8,555 (6,711–8,832) | −9% | 25.3 → 9.3 | 479 → 3,295 | 70.5 → 60.2 |

compio reaps a completion only when its task yields, so a refinement yielded every four requests the core took while a sync was in
flight and attached a finished one at once. All three, interleaved:

| Relations | Connections | Inline | First version | Refined | Commits a sync (inline, first, refined) | Log sync p50 µs |
|---|---|---|---|---|---|---|
| 1 | 1,024 | 40,199 (37,171–40,538) | 34,370 (34,038–34,634), −15% | 33,445 (32,188–33,591), −17% | 33.1, 14.0, 13.6 | 697, 2,055, 1,800 |
| 10 | 256 | 9,132 (8,993–9,562) | 7,750 (7,712–8,002), −15% | 8,509 (8,435–8,627), −7% | 9.5, 4.6, 4.9 | 481, 2,530, 2,608 |

Every overlap figure lies below the inline range. The overlap halves the commits a sync carries and so doubles the syncs: at one
relation and 1,024 connections a partition syncs about 200 times a second inline (6,700 commits a second a partition at 33.1) and
about 410 times with the overlap (5,570 at 13.6), and each sync then takes 1.8 ms where it took 0.7. The inline core's wait is what
gathers a sync's commits: the inbox fills while the core sits in `sync`, and the next intake closes them as one round (D12). Run
during the sync, the core closes each round with what arrived in one shorter sync's time, and the log's device spends about 0.74 s a
second a partition syncing where it spent 0.14. The median latency falls (17 to 10 ms at 1,024 connections) as commits wait less in
the inbox, at a lower rate. The change is reverted: the code, its tests and the commit-rounds scenario; the patch is kept outside the
repository.

## Latency at few clients (the owner, 2026-11-09)

The 6/6/2 layout, the tree with 6.11 reverted, the frontend on every core, 6 partitions, one `client_bench` process, a fresh cluster a
run, three 20 s runs a point interleaved; `client_bench` now reports p90 beside p50 and p99. Latencies are the client's, a commit
sent and its reply read, one commit in flight per connection.

| Relations | Clients | Commits/s | p50 µs | p90 µs | p99 µs | Max µs | Commits a sync |
|---|---|---|---|---|---|---|---|
| 1 | 1 | 411 (396–448) | 2,235 (2,039–2,302) | 3,424 (3,193–3,580) | 4,958 (4,841–5,163) | 27,341 (15,560–30,424) | 1.0 |
| 1 | 16 | 8,638 (7,367–8,649) | 1,679 (1,655–1,781) | 2,403 (2,311–3,291) | 5,876 (5,579–8,198) | 157,331 (85,221–230,837) | 1.5 |
| 1 | 64 | 16,522 (15,664–17,833) | 3,293 (3,247–3,316) | 5,016 (4,819–5,420) | 9,910 (9,047–11,375) | 321,013 (103,933–693,620) | 4.0 |
| 10 | 1 | 353 (349–363) | 2,637 (2,627–2,709) | 3,885 (3,851–4,014) | 5,298 (5,129–6,063) | 20,331 (19,858–171,084) | 1.0 |

One client's 2.2 ms by the roles' phase logs (the second run): the frontend's `one-frame write phases` total p50 1,727 µs, of which
its checks 115 and the partition's answer 1,599; storage's read-to-answer p50 about 1,000 µs, of which the commit's log write 207 and
sync 351 (`commit phases`). So about 1.0 ms in storage, 0.6 ms between the frontend and storage, 0.5 ms between the client and the
frontend, 0.1 ms in the frontend's checks. 6.1's single client, 2,330 commits a second at 0.39 ms, ran every role on one machine with
a log sync of 115 µs; here each request crosses two machines' virtual network twice and the storage machine's sync takes three times
as long. Sixteen clients see a lower median than one; not chased.

## 6.9 The frontend's cost a commit: profiled, no cut (the owner, 2026-11-09)

The frontend bounds neither figure 6.9 is kept on. At 1,024 connections its endpoint cores take 155% of its 600% while about 340
requests wait ahead of storage's six cores (6.8), so a cheaper frontend leaves the commits a second where they are. Its whole checks
phase is 115 µs of one client's 2.2 ms (5%) and 611 µs of the 14.9 ms median at 1,024 connections (4%); one client's median varies
from 2,039 to 2,302 µs between runs, so even checks costing nothing stay within the spread. The profile (6.8) shows no symbol above
4.5%: the timer armed per request 4.5%, SipHash in the wire crate 2.5%, `session_principal` on the owner 2.3%, allocation about 3%.

Each DirectWrite with a session asks the owner twice, one after the other: the gate's `tenant_facts`, then `begin_commit`, with
`commit_ended` sent once it is answered. One ask holding the tenant, the token and the session would save a cross-thread round trip
a write, tens of microseconds; it belongs with 7.5, which takes the owner's asks off the commit path. The owner closed 6.9 without a
cut.

## 6.4 Closed

The shares far below the in-process rate have their causes. The data a cluster keeps (1) goes to 7.2. The frontend's one endpoint
runtime (3) was fixed by D14. Like for like (6.8) ten relations a commit reach 86 to 93% of the in-process rate and one relation 38
to 48%, every role below its CPUs. What remains at one relation is storage's: a partition's core alternates its own work, about
2.3 ms a round, with its log's write and sync, about 1.3 ms, and 24.5% of storage's samples load and checksum a store block for a
term's lookup. Executing the next round during the sync (6.11) halved a sync's commits and was reverted; pinning fjall's
filter and index blocks moved nothing (dropped, above). The core's own work is group 7's (7.1 to 7.4), the store's block reads roadmap stage 6.5's.

## 6.5 Recorded

These notes; the Shard-per-Core note's *Commits batched through the binaries*, beside X1; backlog item 18's *Batching* bullet closed
by it.
