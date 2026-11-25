- Machine: Linux 6.18.33.2-microsoft-standard-WSL2, 16 CPUs, AMD RYZEN AI MAX+ PRO 395 w/ Radeon 8060S
- Commit: b8aafd3, with uncommitted changes
- Command: `igs-iec104-campaign load --seconds 60`

# Load campaign

One connection on loopback, 60 s. The station has 10000 float points in group 1; the first 1000 change in turn, 1000 changes a second in all (one change per point each 1.0 s). A general interrogation starts 250 ms after the previous one ended.

| samples | n | p50 (ms) | p95 (ms) | p99 (ms) | p99.9 (ms) | max (ms) |
| --- | --- | --- | --- | --- | --- | --- |
| general interrogation | 78 | 520.73 | 531.95 | 532.44 | 532.44 | 532.44 |
| spontaneous change, from the change to its reception | 59970 | 152.11 | 480.29 | 513.26 | 525.48 | 530.31 |

- Interrogations: 78 completed. Information objects per interrogation: min 10000, max 10000 (expected 10000 each).
- Changes: 60010 made, 59970 received before the end of the run (the others were in flight).
- Verdict: PASS: every interrogation answered every point
