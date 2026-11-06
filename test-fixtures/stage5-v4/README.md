# Stage 5 v4 counterpart fixtures

Genuine offline `broker_world_eval` entry recordings promoted from the
accepted K1 record run with snapshot-derived gesture barriers, not a live
native run. Each chain (`full`, `s2`) retains two
genesis captures and every intermediate envelope in manifest order.

Normalization: only the exact top-level `gen` value is rewritten to the
harness-fixed `gen-stage5-v4` in settings, envelopes, and projections. Every
other byte and key order is preserved, including `source`, `sourceSeq`,
`nativeInvalid`, `req`, and all report/facts/gesture/revision/witness lanes.
Capture replies and discovery replies are not promoted (nondeterministic
clocks).

Known seams (not evidence of native behavior): the recorded native report
uses fake zero-inset frames, the run parked one `Wait` lease, and these files
carry no native or auth evidence. Clock-epoch replay seam: the recorded
attempt stamps (max 7ms) postdate a fresh broker clock, so the test takes one
bounded sleep past the maximum recorded stamp before feeding genesis; attempt
bytes and the clock oracle are untouched. The legacy v3 `test-fixtures/stage5`
tree is retained unchanged.
