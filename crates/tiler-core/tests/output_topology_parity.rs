//! Shared core/KDE output-topology parity (strict unique-reciprocal map).
//!
//! Consumes `test-fixtures/output-topology-parity.json`, the frozen capture
//! of `kwin/src/world-observe.ts::computeAdjacency`, and requires exact map
//! equality from the core derivation in
//! [`tiler_core::output_selection::unique_reciprocal_adjacency`]. The next
//! KDE unit consumes the same file against the TypeScript implementation;
//! any approved split-edge policy-alignment delta needs separately named
//! expected-result tests, never an edit of this file's expectations.

use tiler_core::directional::{Direction, OutputId};
use tiler_core::geometry::Rect;
use tiler_core::output_selection::{OutputEntry, unique_reciprocal_adjacency};

fn parse_direction(token: &str) -> Direction {
    match token {
        "left" => Direction::Left,
        "right" => Direction::Right,
        "up" => Direction::Up,
        "down" => Direction::Down,
        other => panic!("unknown direction token {other:?}"),
    }
}

#[test]
fn shared_fixture_strict_map_matches_kde_capture() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/output-topology-parity.json");
    let raw = std::fs::read_to_string(&path).expect("parity fixture present");
    let fixture: serde_json::Value = serde_json::from_str(&raw).expect("valid fixture JSON");
    assert_eq!(fixture["version"], serde_json::Value::from(1));
    let cases = fixture["cases"].as_array().expect("cases array");
    assert!(!cases.is_empty(), "fixture carries parity cases");
    for case in cases {
        let name = case["name"].as_str().expect("case name");
        let mut entries = Vec::new();
        let mut names = Vec::new();
        for output in case["outputs"].as_array().expect("outputs array") {
            let out = output["out"].as_str().expect("output name");
            let rect = output["rect"].as_array().expect("rect array");
            let nums: Vec<i32> = rect
                .iter()
                .map(|v| {
                    v.as_i64()
                        .and_then(|n| i32::try_from(n).ok())
                        .expect("i32 rect")
                })
                .collect();
            assert_eq!(nums.len(), 4, "{name}: rect is [x,y,w,h]");
            entries.push(OutputEntry {
                id: OutputId(out.to_owned()),
                rect: Rect {
                    x: nums[0],
                    y: nums[1],
                    w: nums[2],
                    h: nums[3],
                },
            });
            names.push(out.to_owned());
        }
        let map = unique_reciprocal_adjacency(&entries).expect("{name}: fixture rects readable");
        // KDE emits one row per output (possibly empty); the core map only
        // carries resolving pairs, so compare per (output, direction) over
        // the full vocabulary.
        let expected = case["expected"].as_object().expect("expected object");
        assert_eq!(
            expected.len(),
            names.len(),
            "{name}: expected names every fixture output"
        );
        for out in &names {
            assert!(
                expected.contains_key(out.as_str()),
                "{name}: expected has {out}"
            );
        }
        for (out, dirs) in expected {
            let dirs = dirs.as_object().expect("direction map");
            for token in ["left", "right", "up", "down"] {
                let direction = parse_direction(token);
                let actual = map
                    .get(&(OutputId(out.clone()), direction))
                    .map(|id| id.0.clone());
                let wanted = dirs
                    .get(token)
                    .map(|v| v.as_str().expect("target name").to_owned());
                assert_eq!(actual, wanted, "{name}: {out} {token}");
            }
        }
        // No resolving pair outside the expected vocabulary.
        let mut count = 0;
        for dirs in expected.values() {
            count += dirs.as_object().expect("direction map").len();
        }
        assert_eq!(map.len(), count, "{name}: no extra resolving pairs");
    }
}

#[test]
fn shared_fixture_captures_split_edge_ambiguity() {
    // Guard that the fixture still discriminates the world/legacy split:
    // at least one case omits a geometrically touching direction (forward
    // or reverse multi-touch), which the per-use ranked selectors would
    // resolve but the strict map must not.
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/output-topology-parity.json");
    let raw = std::fs::read_to_string(&path).expect("parity fixture present");
    let fixture: serde_json::Value = serde_json::from_str(&raw).expect("valid fixture JSON");
    let cases = fixture["cases"].as_array().expect("cases array");
    let omitted = cases
        .iter()
        .filter(|case| {
            let outputs = case["outputs"].as_array().expect("outputs array");
            let expected = case["expected"].as_object().expect("expected object");
            // A touching pair exists geometrically (checked via the core
            // candidates), yet the expected strict map omits that direction.
            let entries: Vec<OutputEntry> = outputs
                .iter()
                .map(|output| {
                    let rect = output["rect"].as_array().expect("rect");
                    let nums: Vec<i32> = rect
                        .iter()
                        .map(|v| v.as_i64().and_then(|n| i32::try_from(n).ok()).expect("i32"))
                        .collect();
                    OutputEntry {
                        id: OutputId(output["out"].as_str().expect("name").to_owned()),
                        rect: Rect {
                            x: nums[0],
                            y: nums[1],
                            w: nums[2],
                            h: nums[3],
                        },
                    }
                })
                .collect();
            let has_touch = entries.iter().any(|source| {
                [
                    Direction::Left,
                    Direction::Right,
                    Direction::Up,
                    Direction::Down,
                ]
                .iter()
                .any(|dir| {
                    tiler_core::output_selection::candidates_in_direction(source, *dir, &entries)
                        .map(|cands| !cands.is_empty())
                        .unwrap_or(false)
                })
            });
            let all_empty = expected
                .values()
                .all(|dirs| dirs.as_object().expect("direction map").is_empty());
            has_touch && all_empty
        })
        .count();
    // mirrored-overlap-above-wide is fully empty despite geometric
    // touches; every other ambiguous case keeps its unambiguous sibling
    // resolutions (split rows, stacked pairs).
    assert_eq!(omitted, 1, "fixture keeps full-omission ambiguity cases");
}
