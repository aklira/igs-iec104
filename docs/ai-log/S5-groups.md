# One redundancy group per controlling station address

## Prompt (summarised)

The station served every connection in one redundancy group, so two controlling stations could not transfer data at
once. Clause 10.2 requires a different redundancy group, with its own process image, for each controlling station that
accesses the same controlled station at once. The maintainer chose: one group per controlling station address (A), and
the changes propagated to every group (propagation).

## Summary

- `crates/igs-iec104/src/server.rs`: the station keeps a registry of groups (`Groups`), keyed by the peer address. A
  template of the points is kept with the values, and each group holds the same points. A publication (a point, its
  group, a change) reaches the template and every group; a change queues in each group, and its started connection
  sends it. A new group starts from a snapshot of the template's values. The first group takes the events raised
  before any controlling station connected, so they wait for it, as before.
- `process_image.rs`: `snapshot`, `clear_events`, `take_events`, `set_events` (the image of a group without a queue).
- The connections of a group share its process: join, start, supersede, and the carry-over of unacknowledged events are
  per group.
- The shutdown report counts the events waiting in the template and in every group.

## Tests

- `server::tests::controlling_stations_at_different_addresses_transfer_at_once_and_each_gets_the_changes`: two controlling
  stations at two addresses are started together, and a change reaches both.
- The existing tests keep one address (127.0.0.1) and so one group: the supersede, switchover, and window tests still
  hold. `the_events_no_connection_took_are_counted_at_the_stop` counts the events of the template.
- Workspace: fmt, clippy with `-D warnings`, all tests, the documentation with `-D warnings`, `cargo deny`: passing.

## Decisions

See `PROVENANCE.md`, D-032. The key is the peer address: two controlling stations behind one address share a group.
