# Provenance

This file links every non-trivial behaviour of `igs-iec104` to its source: a clause of an IEC
standard, an observation test, or third-party code under a permissive licence. The project is a
clean-room implementation: no protocol behaviour without a source recorded here or in an
`// IEC 60870-5-104 §x.y` comment.

The IEC standards are licensed and are not distributed with this repository. The references below
point to clauses; they do not reproduce the text of the standards.

## Reference standards

| Standard | Edition | Use |
| --- | --- | --- |
| IEC 60870-5-104 | 2006 | TCP/IP transport, APCI, ASDU profile |
| IEC 60870-5-101 | 2003 | ASDU definitions, information elements |
| IEC 62351-3 | 2023 | TLS profile |

## Decisions on ambiguities in the standard

Profile of the causes of transmission (COT) accepted for each ASDU type. Two sources in the 104 can
disagree: the "type identification × cause of transmission" matrix in 104 §9.5, and the list of
causes given in the definition of each ASDU (101 §7.3 or 104 §8). They agree for 49 of the 54 types
in the 104 profile; the 5 discrepancies are resolved below.

Confidence: **high** when the sources within the 104 agree; **medium** when the 104 contradicts
itself, to be confirmed by an observation test against other implementations.

### D-001 — No return information for M_PS_NA_1 (TI 20)

- **Decision**: causes 11 and 12 (return information) rejected for TI 20.
- **Sources**: 104 §9.5, matrix (cells marked "not required"); 104 §7.7, the return-information
  service only covers M_SP, M_DP and M_ST. 101 §7.3.1.20 allows them: restriction specific to the
  104 profile.
- **Confidence**: high.
- **Test**: to be written.

### D-002 — Deactivation of C_BO_NA_1 (TI 51)

- **Decision**: causes 8 and 9 (deactivation and its confirmation) accepted for TI 51, as an
  option; a controlled station may choose not to support them.
- **Sources**: 104 §7.7, the command-interruption service includes C_BO; 101 §7.3.2.7 defines these
  causes. The 104 §9.5 matrix shades these two cells, in a shade that differs from the rest of the
  grid; the explicit text is preferred.
- **Out of scope**: does not apply to C_BO_TA_1 (TI 64), for which 104 §8.7 and the matrix both
  exclude these causes.
- **Confidence**: medium (internal contradiction in the 104).
- **Test**: to be written; to be confirmed by observation.

### D-003 — No deactivation for C_CI_NA_1 (TI 101)

- **Decision**: causes 8 and 9 rejected for TI 101.
- **Sources**: 104 §9.5, matrix (cells marked "not required"); 104 §7.8, transmission of integrated
  totals only provides for activation, confirmation and termination. 101 §7.3.4.2 allows them:
  restriction specific to the 104 profile.
- **Confidence**: high.
- **Test**: to be written.

### D-004 — Spontaneous clock synchronization not permitted (TI 103)

- **Decision**: cause 3 (spontaneous) rejected for C_CS_NA_1; only activation and activation
  confirmation are sent.
- **Sources**: 104 §9.5, matrix (cell marked "not permitted"); 104 §7.6. 101 §7.3.4.4 allows it:
  explicit restriction of the 104.
- **Confidence**: high.
- **Test**: to be written.

### D-005 — Causes for F_SC_NB_1 QueryLog (TI 127)

- **Decision**: sent with the causes of 104 §8.9 only (13, 44 to 47). Cause 5 (request) is never
  sent; whether to tolerate it on reception is still to be decided.
- **Sources**: 104 §8.9 (ASDU definition); the 104 §9.5 matrix leaves cause 5 possible; 104 §7.11
  does not mention QueryLog.
- **Confidence**: medium (internal contradiction in the 104).
- **Test**: to be written; to be confirmed by observation.

## Third-party code and sources

| Source | Licence | Use | Provenance |
| --- | --- | --- | --- |
| `iec104` crate (Matheus Zaniolo) | MIT, `LICENSE` file checked | Possible inspiration or code | To be confirmed with the author |
