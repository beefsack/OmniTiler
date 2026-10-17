#![forbid(unsafe_code)]
use tiler_windows::test_window::{
    HELPER_DEFAULT_SECONDS, parse_close_args, parse_helper_run_args, parse_hwnd,
    parse_tagged_hwnd_args,
};

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn hwnd_forms() {
    assert_eq!(parse_hwnd("12345"), Some(12345));
    assert_eq!(parse_hwnd("0xABCD"), Some(0xABCD));
    assert_eq!(parse_hwnd("0"), None);
    assert_eq!(parse_hwnd("xyz"), None);
}

#[test]
fn helper_run_requires_receipt_and_bounds_seconds() {
    let opts = parse_helper_run_args(&strings(&["--receipt", "r.json"])).expect("defaults");
    assert_eq!(opts.seconds, HELPER_DEFAULT_SECONDS);
    assert!(!opts.passive, "spike behavior stays the default");
    let passive =
        parse_helper_run_args(&strings(&["--receipt", "r.json", "--passive"])).expect("passive");
    assert!(passive.passive);
    assert!(parse_helper_run_args(&strings(&[])).is_err());
    assert!(parse_helper_run_args(&strings(&["--receipt", "r.json", "--seconds", "0"])).is_err());
    assert!(parse_helper_run_args(&strings(&["--receipt", "r.json", "--seconds", "601"])).is_err());
    assert!(parse_helper_run_args(&strings(&["--receipt", "r.json", "--bogus"])).is_err());
}

#[test]
fn tagged_hwnd_requires_hwnd_and_nonempty_tag() {
    let tagged = parse_tagged_hwnd_args("show", &strings(&["123", "--tag", "abcdef0123456789"]))
        .expect("parsed");
    assert_eq!(tagged.hwnd, 123);
    assert_eq!(tagged.tag, "abcdef0123456789");
    assert!(parse_tagged_hwnd_args("show", &strings(&["123"])).is_err());
    assert!(parse_tagged_hwnd_args("show", &strings(&["123", "--tag", ""])).is_err());
    assert!(parse_tagged_hwnd_args("show", &strings(&["xyz", "--tag", "t"])).is_err());
    assert!(parse_tagged_hwnd_args("show", &strings(&["0", "--tag", "t"])).is_err());
    assert!(parse_tagged_hwnd_args("show", &strings(&["123", "--tag", "t", "extra"])).is_err());
}

#[test]
fn close_args_bare_stays_tagged_binds_tag() {
    // Bare form stays for the WinArrow harness.
    assert_eq!(parse_close_args(&strings(&["123"])), Ok((123, None)));
    // Proof close/recovery paths pass the captured tag.
    assert_eq!(
        parse_close_args(&strings(&["123", "--tag", "abcdef0123456789"])),
        Ok((123, Some("abcdef0123456789".to_owned())))
    );
    assert!(parse_close_args(&strings(&[])).is_err());
    assert!(parse_close_args(&strings(&["123", "--tag", ""])).is_err());
    assert!(parse_close_args(&strings(&["xyz", "--tag", "t"])).is_err());
    assert!(parse_close_args(&strings(&["0", "--tag", "t"])).is_err());
    assert!(parse_close_args(&strings(&["123", "--tag"])).is_err());
    assert!(parse_close_args(&strings(&["123", "--tag", "t", "extra"])).is_err());
    assert!(parse_close_args(&strings(&["123", "--bogus"])).is_err());
}
