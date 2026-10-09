# AI log

This directory records the interaction between the human maintainers and the
AI coding agent that implements the tasks of `IMPLEMENTATION.md`.

## Convention

- One file per task, named after the task ID: `F1.md`, `C3.md`, and so on.
- Each file contains the prompt sent for that task, then a short summary of
  what the agent produced (files touched, tests added, anything a reviewer
  should look at).
- **No text from the IEC standards** in these files. Cite clauses and
  paraphrase; the standards are licensed and their text never enters this
  repository (see the rules in `IMPLEMENTATION.md` section 1).
- Open questions that the agent must not guess are not written here but in
  [QUESTIONS.md](QUESTIONS.md).

## Files

| File | Purpose |
| --- | --- |
| [QUESTIONS.md](QUESTIONS.md) | Open questions where the standards are silent, ambiguous or contradictory |
