# Open questions

Rules (IMPLEMENTATION.md section 1, rule 4): when a standard is silent,
ambiguous, or a figure needed by a task is missing from the prepared text
(`[FIGURE ... MISSING]`), the agent does not guess. It adds a row here with
the task ID, the question and the clauses it read, and moves on to another
task. A resolved question gets an entry in `PROVENANCE.md` and a permanent
test; the row below then references it.

The PDF of each standard is the authority when its prepared text and the data
files disagree.

| ID | Task | Question | Clauses read | Status |
| --- | --- | --- | --- | --- |
| Q-001 | C4 | Are the file transfer types (F_FR_NA_1 to F_DR_TA_1) sent in both directions? 104 §9.5 marks them "station-specific" and does not say. The codec accepts both directions; the profile check does not restrict them. | 104 §9.5 (file transfer table) | open: both directions accepted |
| Q-002 | C4 | Should a received F_SC_NB_1 with cause 5 (request) be tolerated? D-005 leaves this open. The codec rejects it with `CauseNotAllowed`. | PROVENANCE.md D-005; 104 §8.9 | open: rejected |
