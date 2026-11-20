# KDE border refresh scheduling

Focus and workspace changes can drop a native border while core waits for a
fresh observation. In the two-output dogfood trace, the normal 120 ms host
observation debounce dominated simple activation; workspace changes could
repeat it as native membership, view and desktop roster changes applied.
The fix is scheduling only, not a change to core eligibility, the renderer
coverage rules, or the wire formats.

Activation, desktop roster/view changes and membership changes coalesce a
post-event sample using the existing immediate `WaitPresentationFacts` read.
A reply from the previously held watch cannot establish that it covers the
new event, and a newer event arriving during the sample requires another
sample before expedited capture. Only a settled sample preempts the ordinary
debounce, using the existing capture flight. Commands and ordinary fallback
observation do not wait for the optional effect. The native renderer still
verifies coverage against its authoritative counter. Raw frame changes retain
their existing debounce. Native presentation still waits for core to publish a
covering descriptor and for the compositor to render it.

Deterministic adapter tests exercise the early capture, a late reply from
before activation, coalesced structural changes across a pending sample,
geometry batching, and ordinary observation while the sample is pending.
All four pass with TypeScript typechecking, and the full KWin suite passes
together with the shared-roster retirement fix.

The expected benefit is one avoided debounce on uncomplicated activation,
less the cost of the immediate effect sample. Visual latency on a real
desktop is unmeasured.
