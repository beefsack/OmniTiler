#![forbid(unsafe_code)]
use tiler_windows::lifecycle::{is_medium_rid, parse_run_args, stop_request_matches};

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn run_defaults_and_bounds() {
    let options = parse_run_args(&[]).expect("defaults");
    assert_eq!(options.seconds, 120);
    assert!(!options.trace);
    assert_eq!(options.hide_hwnd, None);
    let options = parse_run_args(&strings(&["--seconds", "10", "--trace"])).expect("parsed");
    assert_eq!(options.seconds, 10);
    assert!(options.trace);
    assert!(parse_run_args(&strings(&["--seconds", "0"])).is_err());
    assert!(parse_run_args(&strings(&["--seconds", "601"])).is_err());
    assert!(parse_run_args(&strings(&["--bogus"])).is_err());
    assert!(parse_run_args(&strings(&["--seconds=10"])).is_err());
}

#[test]
fn run_hide_optional() {
    let options = parse_run_args(&strings(&["--hide", "12345"])).expect("hide decimal");
    assert_eq!(options.hide_hwnd, Some(12345));
    let options = parse_run_args(&strings(&["--hide", "0xABCD"])).expect("hide hex");
    assert_eq!(options.hide_hwnd, Some(0xABCD));
    assert!(parse_run_args(&strings(&["--hide"])).is_err());
    assert!(parse_run_args(&strings(&["--hide", "0"])).is_err());
    assert!(parse_run_args(&strings(&["--hide", "xyz"])).is_err());
}

#[test]
fn medium_range() {
    assert!(!is_medium_rid(8191));
    assert!(is_medium_rid(8192));
    assert!(is_medium_rid(8448));
    assert!(is_medium_rid(12287));
    assert!(!is_medium_rid(12288));
    assert!(!is_medium_rid(16384));
}

#[test]
fn stop_request_matching() {
    assert!(stop_request_matches("000000000000abcd", "000000000000abcd"));
    assert!(stop_request_matches(
        "000000000000abcd\n",
        "000000000000abcd"
    ));
    assert!(!stop_request_matches(
        "ffffffffffffffff",
        "000000000000abcd"
    ));
    assert!(!stop_request_matches("", "000000000000abcd"));
    assert!(!stop_request_matches("000000000000abcd", ""));
}
