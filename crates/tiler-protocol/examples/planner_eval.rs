#![forbid(unsafe_code)]
//! Minimal offline test seam: line-delimited DescribePlan evaluation.
//!
//! Reads one JSON request per stdin line, evaluates each against a single
//! retained [`tiler_protocol::planner_protocol::Planner`], and writes one JSON
//! reply per stdout line (empty lines skipped). This exercises the exact
//! request validation, retained Engine state, and wire serialization the
//! shipped `planner-service` binary uses, without D-Bus, activation, or any
//! live host state. Test-only seam; not part of the shipped service.

use std::io::{BufRead, Write};

fn main() {
    let planner = std::sync::Mutex::new(tiler_protocol::planner_protocol::Planner::new());
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let reply = planner
            .lock()
            .expect("planner lock is usable")
            .evaluate(&line);
        out.write_all(reply.as_bytes()).expect("stdout is writable");
        out.write_all(b"\n").expect("stdout is writable");
    }
    out.flush().ok();
}
