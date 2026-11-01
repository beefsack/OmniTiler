# KWin QKeySequence D-Bus Abort

## Goal

Diagnose KWin PID `294764`'s 2026-09-12 `SIGABRT` without changing live Plasma
state or attributing a library fault to a caller without stack evidence.

## Scope And Non-Goals

- Read-only core, journal, dev-log, repository-source, and prior-core review.
- Do not run a KWin/Plasma lifecycle action, mutate shortcut settings, reload a
  script, remove evidence, or change product code.
- This is not the fixed PID `2336`/`2090` `SIGSEGV` path through
  `QV4::WeakMapPrototype::method_get`.

## Established Evidence

- `coredumpctl info 294764` identifies the host KWin 6.7.4 store package executable `bin/.kwin_wayland-wrapped`, PID `294764`,
  at `2026-09-12 19:41:24 AEST`, `SIGABRT`, `SI_TKILL`, boot
  `92256fedfc644803bf6d35fab46527d3`. The retained compressed core is present.
- The aborting main-thread backtrace is:

  ```text
  #0  __pthread_kill_implementation
  #1  raise
  #2  KCrash::defaultCrashHandler
  #3  <signal handler called>
  #4  __pthread_kill_implementation
  #5  raise
  #6  abort
  #7  _dbus_abort.cold
  #8  _dbus_warn_check_failed
  #9  _dbus_marshal_read_basic
  #10 QDBusArgument::operator>>(int&) const
  #11 operator>>(QDBusArgument const&, QKeySequence&)
  #12 QDBusMetaType::demarshall
  #13 QDBusConnectionPrivate::deliverCall
  #14 QDBusConnectionPrivate::activateCall
  #15 QDBusConnectionPrivate::activateObject
  #16 QDBusActivateObjectEvent::placeMetaCall
  #17 QObject::event
  #18 QApplicationPrivate::notify_helper
  #19 QCoreApplication::notifyInternal2
  #20 QCoreApplicationPrivate::sendPostedEvents
  #21 QEventDispatcherUNIX::processEvents
  #22 QUnixEventDispatcherQPA::processEvents
  #23 QEventLoop::exec
  #24 QCoreApplication::exec
  #25 main
  ```

- The aborting code was invoked by KWin's inbound Qt D-Bus dispatch, not an
  outgoing reply: `activateObject` dispatched a registered object and
  `deliverCall` demarshalled its `QKeySequence` argument before the target
  method ran. The journal records the libdbus assertion at the same timestamp:

  ```text
  kwin_wayland_wrapper[294764]: dbus[294764]: type invalid 0 not a basic type
  kwin_wayland_wrapper[294764]: dbus[294764]:   D-Bus not built with -rdynamic so unable to print a backtrace
  kwin_wayland_wrapper[294764]: KCrash: Application '.kwin_wayland-wrapped' crashing... crashRecursionCounter = 2
  ```

- Stripped Qt/KWin symbols and the missing retained `QDBusMessage` prevent
  recovering the inbound sender, object path, interface, or member. The exact
  target method is therefore not established.
- `thread apply all bt` has no `QV4`, `QJSValue`, `KWin::Script`, project JIT,
  or `omnitiler-active-border.so` frame. The native effect appears only
  as a loaded, deleted mapping without a build ID. This exonerates the native
  effect under the fault-stack criterion for this core. It also gives no basis
  to implicate the bundled script.
- PID `3568836` (2026-09-07) has the same executable, signal, and stack frames
  `#0-13` and `#15-25`; only the `activateCall` return offset differs. This is
  a repeat of that KWin/libdbus `QKeySequence` demarshalling abort, not a new
  signature relative to PID `3568836`, while it remains different from the
  fixed SIGSEGV.
- Repository D-Bus calls do not send `QKeySequence` arguments to KWin. The KCM
  decodes KGlobalAccel replies from the separate `org.kde.kglobalaccel` service;
  its KWin calls are `reconfigureEffect(QString)` and `reconfigure()`. The KWin
  script calls only the Planner and Tray services. These source facts exclude a
  repository path matching the inbound malformed call, but cannot identify its
  actual sender.
- The final project dev log is `omnitiler-dev.0L9vAz.log`,
  mtime `18:53:10`; its final two lines are:

  ```text
  [kwin] omnitiler:plan:cmd=plan-1-p64 kind=admit windows=1 outcome=rejected
  [kwin] omnitiler:plan:rejected kind=duplicate-window
  ```

  The KWin journal has no `omnitiler:plan` line between `18:53:10` and
  the abort. That `duplicate-window` result is the already-settled benign
  cross-workspace verdict, not an abort precursor.
- The bounded `19:35-19:41` journal window has no recorded lock, idle, DPMS,
  output change, suspend/resume, OOM, GPU hang, or DRM reset before the abort.
- KWin coredumped first at `19:41:24`; the four `devenv shell` processes
  (`428288`, `431420`, `295390`, and `295679`) coredumped at `19:41:25-26`.
  Their captured stacks are Rust `eprint` panic-to-`abort` paths. The subsequent
  journal says `The Wayland connection broke. Did the Wayland compositor die?`
  and starts a new compositor. Their ordering establishes that they did not
  cause KWin's abort.
- The seven `omnitiler-shortcut-reconciler-test`
  cores at `19:06-19:10` all ran `malformed`, faulted reading `0x18`, and were
  built before the current source and before the on-disk binary replaced them.
  GDB reports every core may not match that binary. The current `malformed`
  scenario is pure reply-parser coverage and has no matching unsafe path.
  The old binaries make the crashes unusable as evidence of a current
  `shortcutreconciler.cpp` defect.

## Inference And Limits

- The four terminal aborts are consequences of compositor loss, based on their
  later ordering and Wayland-break messages. Their exact panic text is absent.
- The lack of an idle/power journal event argues against, but does not disprove,
  an idle-time trigger. No observed event connects the user's absence to the
  malformed call.
- The available evidence supports an external or KWin-internal producer of a
  malformed inbound `(ai)` `QKeySequence` payload. It does not establish which.
  Capturing the sender, object path, interface, member, and raw signature before
  libdbus aborts would be needed for positive attribution.
- The repeated reconciler faults most likely came from a replaced temporary test-harness build or an in-progress experiment, rather than HEAD;
  the pre-HEAD binary and source are unavailable, so that provenance cannot be
  proved.

## Outcome

- Cause of the malformed D-Bus message is not established. No code fix is
  warranted or shipped.
- Read-only investigation only; no KWin, Plasma, shortcut, configuration, or
  core-dump mutation occurred.

## Backlog detail moved 2026-11-01

### Original backlog lines 2068-2071

- P2 | PID 3568836 SIGABRT (`QKeySequence` D-Bus abort) | Formally open;
  needs sender, method, and fault-stack evidence before attributing it to
  the effect or script.
  [record](kwin-qkeysequence-dbus-abort.md)
