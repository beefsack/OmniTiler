use super::*;

pub fn watcher_ready_path(dir: &Path, owner_creation: &str) -> PathBuf {
    dir.join(watcher_ready_filename(owner_creation))
}

/// Start the same-executable watcher bound to the exact owner and wait
/// until its live identity verifies. Call before the first product hide.
pub fn spawn_watcher(owner: &ProcessIdentity, dir: &Path) -> Result<std::process::Child> {
    match HeldProcess::open(owner.pid) {
        Ok(_) => {}
        Err(e) => return Err(err(format!("refuse: owner absent at watcher spawn {e}"))),
    }
    let exe_path = crate::native::current_exe_path().map_err(|e| err(format!("error: exe {e}")))?;
    let mut child = std::process::Command::new(&exe_path)
        .arg("watch-owner")
        .arg("--pid")
        .arg(owner.pid.to_string())
        .arg("--creation")
        .arg(owner.process_creation.clone())
        .spawn()
        .map_err(|e| err(format!("error: watcher spawn {e}")))?;
    let start = Instant::now();
    loop {
        match verify_watcher_ready(dir, owner) {
            Ok(()) => break,
            Err(e) if e.to_string().starts_with("refuse:") => {}
            Err(e) => {
                let _ = child.kill();
                return Err(e);
            }
        }
        if let Ok(Some(status)) = child
            .try_wait()
            .map_err(|e| err(format!("error: watcher wait {e}")))
        {
            return Err(err(format!("error: watcher exited early {status}")));
        }
        if start.elapsed() > Duration::from_millis(WATCHER_READY_TIMEOUT_MS) {
            let _ = child.kill();
            return Err(err("error: watcher readiness timeout"));
        }
        std::thread::sleep(Duration::from_millis(WATCHER_POLL_MS));
    }
    Ok(child)
}

/// Verify the live watcher: marker parses, owner creation matches, the
/// watcher pid is alive with the recorded creation, same exe/SID/session,
/// and medium integrity. Stale text alone never passes.
pub fn verify_watcher_ready(dir: &Path, owner: &ProcessIdentity) -> Result<()> {
    let not_ready = || err("refuse: recovery watcher not ready");
    let text = match std::fs::read_to_string(watcher_ready_path(dir, &owner.process_creation)) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(not_ready()),
        Err(e) => return Err(err(format!("error: watcher ready read: {e}"))),
    };
    let Some((creation, watcher_pid, watcher_creation)) = parse_watcher_ready(&text) else {
        return Err(not_ready());
    };
    if creation != owner.process_creation || watcher_pid == owner.pid {
        return Err(not_ready());
    }
    let held = match HeldProcess::open(watcher_pid) {
        Ok(h) => h,
        Err(_) => return Err(not_ready()),
    };
    let live = match held.identity() {
        Ok(l) => l,
        Err(_) => return Err(not_ready()),
    };
    if live.process_creation != watcher_creation || !held.is_alive() {
        return Err(not_ready());
    }
    if !exe_paths_equal(&live.exe_path, &owner.exe_path)
        || live.user_sid != owner.user_sid
        || live.session_id != owner.session_id
    {
        return Err(not_ready());
    }
    match held.integrity() {
        Ok(rid) if is_medium_rid(rid) => Ok(()),
        Ok(_) => Err(not_ready()),
        Err(_) => Err(not_ready()),
    }
}

pub fn stop_watcher(child: &mut std::process::Child, dir: &Path, owner_creation: &str) {
    let _ = child.kill();
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(2000) {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(_) => break,
        }
    }
    let _ = std::fs::remove_file(watcher_ready_path(dir, owner_creation));
}

fn peek_ledger(dir: &Path) -> Result<Option<crate::model::RecoveryLedger>> {
    match std::fs::read(dir.join(LEDGER_FILE_NAME)) {
        Ok(bytes) => {
            let text = std::str::from_utf8(&bytes).map_err(|_| err("refuse: corrupt ledger"))?;
            crate::model::parse_ledger(text)
                .map(Some)
                .map_err(|_| err("refuse: corrupt ledger"))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(err(format!("error: ledger read: {e}"))),
    }
}

/// Retry the lease until obtained or the ledger is verifiably gone or
/// replaced, then restore under that same lease. Contention never reports
/// handled success: exhaustion preserves the failure for the next reclaim.
fn watcher_restore(expected: &ProcessIdentity) -> Result<String> {
    let dir =
        crate::native::ledger_directory().map_err(|e| err(format!("error: ledger dir: {e}")))?;
    let start = Instant::now();
    loop {
        match LedgerStore::open(&dir) {
            Ok(store) => match crate::lifecycle::sys::restore_under_lease(&store, Some(expected)) {
                Ok((windows, _)) => {
                    let _ =
                        std::fs::remove_file(watcher_ready_path(&dir, &expected.process_creation));
                    return Ok(serde_json::json!({
                        "watched": true, "outcome": "restored", "windows": windows,
                    })
                    .to_string());
                }
                Err(e) if e.to_string() == "refuse: owner-replaced" => {
                    return Ok(serde_json::json!({
                        "watched": false, "outcome": "owner-replaced",
                    })
                    .to_string());
                }
                Err(e) => return Err(e),
            },
            Err(crate::storage::StorageError::LockContended) => match peek_ledger(&dir)? {
                None => {
                    let _ =
                        std::fs::remove_file(watcher_ready_path(&dir, &expected.process_creation));
                    return Ok(
                        serde_json::json!({"watched": true, "outcome": "no-ledger"}).to_string()
                    );
                }
                Some(record) if !watcher_may_restore(expected, &record.owner) => {
                    return Ok(serde_json::json!({
                        "watched": false, "outcome": "owner-replaced",
                    })
                    .to_string());
                }
                Some(_) => {
                    if start.elapsed() > Duration::from_millis(STORE_ACQUIRE_TIMEOUT_MS) {
                        return Err(err("error: watcher store contended"));
                    }
                    std::thread::sleep(Duration::from_millis(WATCHER_POLL_MS));
                }
            },
            Err(e) => return Err(store_err(e)),
        }
    }
}

/// Watcher entry: `tiler-windows watch-owner --pid PID --creation HEX`.
/// Binds the exact owner, publishes readiness, waits on the held owner
/// handle, then restores. Never touches a replacement owner's ledger.
pub fn cmd_watch_owner(pid: u32, creation: &str) -> Result<String> {
    if pid == 0 || creation.is_empty() {
        return Err(err("usage: watch-owner --pid PID --creation HEX"));
    }
    let dir =
        crate::native::ledger_directory().map_err(|e| err(format!("error: ledger dir: {e}")))?;
    let me = crate::native::current_identity().map_err(|e| err(format!("error: exe {e}")))?;
    let (held_opt, captured): (Option<HeldProcess>, Option<ProcessIdentity>) =
        match HeldProcess::open(pid) {
            Ok(h) => match h.identity() {
                Ok(live) if live.process_creation == creation => {
                    let snapshot = live.clone();
                    (Some(h), Some(snapshot))
                }
                Ok(_) => (None, None),
                Err(IdentityError::Absent) => (None, None),
                Err(e) => return Err(err(format!("error: owner bind {e}"))),
            },
            Err(IdentityError::Absent) => (None, None),
            Err(e) => return Err(err(format!("error: owner bind {e}"))),
        };
    std::fs::write(
        watcher_ready_path(&dir, creation),
        render_watcher_ready(creation, me.pid, &me.process_creation),
    )
    .map_err(|e| err(format!("error: watcher ready write: {e}")))?;
    if let Some(held) = held_opt {
        held.wait(u32::MAX);
    }
    let expected = match captured {
        Some(full) => full,
        None => match peek_ledger(&dir)? {
            Some(record)
                if record.owner.pid == pid && record.owner.process_creation == creation =>
            {
                record.owner
            }
            _ => {
                return Ok(serde_json::json!({
                    "watched": false, "outcome": "owner-replaced",
                })
                .to_string());
            }
        },
    };
    watcher_restore(&expected)
}

/// Restore dead-owner residue before a new product owner starts. Unknown
/// identity errors propagate (never treated as absent); contention without
/// a restore reports no work so the guarded refusal below still applies.
pub fn reclaim_dead_residue() -> Result<bool> {
    let dir =
        crate::native::ledger_directory().map_err(|e| err(format!("error: ledger dir: {e}")))?;
    if !dir.exists() {
        return Ok(false);
    }
    let record = match peek_ledger(&dir)? {
        Some(r) => r,
        None => return Ok(false),
    };
    let me = crate::native::current_identity().map_err(|e| err(format!("error: identity {e}")))?;
    if me.user_sid != record.owner.user_sid || me.session_id != record.owner.session_id {
        return Ok(false);
    }
    if !exe_paths_equal(&me.exe_path, &record.owner.exe_path) {
        return Ok(false);
    }
    let dead = match HeldProcess::open(record.owner.pid) {
        Ok(held) => match held.identity() {
            Ok(live) => live != record.owner || !held.is_alive(),
            Err(IdentityError::Absent) => true,
            Err(e) => return Err(err(format!("error: owner bind {e}"))),
        },
        Err(IdentityError::Absent) => true,
        Err(e) => return Err(err(format!("error: owner bind {e}"))),
    };
    if !dead {
        return Ok(false);
    }
    match crate::lifecycle::sys::cmd_restore() {
        Ok(_) => Ok(true),
        Err(e) if e.to_string().contains("another owner holds the store") => Ok(false),
        Err(e) => Err(e),
    }
}
