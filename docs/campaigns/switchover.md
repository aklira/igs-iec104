- Machine: Linux 6.18.33.2-microsoft-standard-WSL2, 16 CPUs, AMD RYZEN AI MAX+ PRO 395 w/ Radeon 8060S
- Commit: b8aafd3, with uncommitted changes
- Command: `igs-iec104-campaign switchover`

# Switchover campaign

Two stations of one redundancy group on loopback (common addresses 1 and 2), 1000 float points each, changing in turn at 2000 changes a second in all. The connection that carries the data is cut 10 s into the load, through a relay on loopback; the controlling station reconnects, starts the data transfer again, and receives the changes for 10 s more. Then the changes stop, and the connection receives what is left.

| measure | value |
| --- | --- |
| changes made by the stations | 40060 |
| connections the controlling station had | 2 |
| connections that ended (the cut) | 1 |
| deliveries on the first connection | 20040 |
| deliveries on the later connections | 20022 |
| resent changes (delivered a second time) | 2 |
| most deliveries of one change | 2 |
| gaps (a change skipped, then a later one arrived) | 0 |
| reorders (a first delivery after a later change) | 0 |
| changes not received at the end | 0 |

- Verdict: PASS: no change lost, in order, at most one resend per change
