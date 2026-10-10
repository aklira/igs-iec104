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
| Q-003 | L2 | An I or S frame whose N(R) acknowledges frames never sent (N(R) outside the sent-but-unacknowledged range). §5.1 defines a valid acknowledgement but says nothing about an invalid one. The session closes the connection (`CloseReason::InvalidReceiveSequence`). | 104 §5.1 | open: closes |
| Q-004 | L2 | Figure 18 (controlling station). Resolved: the figure was read from the PDF page 51 (rendered image, 2026-10-10). The controlling station's STARTDT and STOPDT procedure follows it. | 104 §5.3, figures 17 and 18 | resolved: implemented as figure 18 |
| Q-005 | L2 | An I frame whose N(S) is not the next number expected. The text gives no rule; the session closes the connection (`CloseReason::SequenceError`). Figure 11 (disturbed sequence) was not checked. | 104 §5.1, figures 9 to 12 | open: closes |
| Q-006 | L2 | A frame whose ASDU cannot be decoded (for example a type outside the profile, or an invalid element). The session closes the connection, whereas 101 says such ASDUs are discarded by the controlling station. Discarding while counting N(S) would keep the connection. | 101 §7.2.6 (ASDUs with undefined values are discarded); 104 §5.1 | open: closes |
| Q-007 | L2 | STOPDT act received while stopped: figure 17 has no transition for it (only the U-frame loop), so the session sends nothing. The peer then waits for a STOPDT con until its t1 expires. Confirm the figure. | 104 §5.3, figure 17 | open: no action |
| Q-008 | L2 | In the pending states of the controlling station (Pending STOPPED, Pending UNCONFIRMED STOPPED), figure 18 shows the I and S frames as loops without an action. §5.3 says the controlling station should send an S frame at once when it receives an I frame there, and should confirm the frames received before its STOPDT act. The text is followed. | 104 §5.3, figure 18 | open: text followed |
