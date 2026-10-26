//! Core-only high-volume stress acceptance (development-only).
//!
//! Runs the shared [`tiler_protocol::world_stress`] driver on one case file
//! across the ordered, batched and reordered simulated-host schedules and
//! checks the hand-authored final oracle plus explicit hide/reveal
//! checkpoints. Usage:
//! `world_stress_acceptance CASE [--schedule ordered|batched|reordered|all]`.

use tiler_protocol::world_stress::{StressSchedule, load_stress_case, run_stress};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (case_path, schedule) = match args.as_slice() {
        [_, case] => (case.as_str(), None),
        [_, case, flag, value] if flag == "--schedule" => (
            case.as_str(),
            Some(match value.as_str() {
                "ordered" => vec![StressSchedule::Ordered],
                "batched" => vec![StressSchedule::Batched],
                "reordered" => vec![StressSchedule::Reordered],
                "all" => StressSchedule::all().to_vec(),
                other => {
                    eprintln!("REJECT: unknown schedule {other:?}");
                    std::process::exit(2);
                }
            }),
        ),
        _ => {
            eprintln!(
                "usage: world_stress_acceptance CASE [--schedule ordered|batched|reordered|all]"
            );
            std::process::exit(2);
        }
    };
    let case = match load_stress_case(case_path) {
        Ok(case) => case,
        Err(error) => {
            eprintln!("REJECT: load: {error}");
            std::process::exit(1);
        }
    };
    let schedules = schedule.unwrap_or_else(|| StressSchedule::all().to_vec());
    for schedule in schedules {
        match run_stress(&case, schedule) {
            Ok((lines, stats)) => {
                for line in &lines {
                    println!("{line}");
                }
                println!(
                    "ACCEPT {}:{} reports={} checkpoints={}",
                    case.name(),
                    schedule.as_str(),
                    stats.reports,
                    stats.checkpoints
                );
            }
            Err(error) => {
                eprintln!("REJECT {}:{}: {error}", case.name(), schedule.as_str());
                std::process::exit(1);
            }
        }
    }
}
