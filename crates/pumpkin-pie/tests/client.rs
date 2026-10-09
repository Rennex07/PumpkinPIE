//! The batch length check, on the host target.
//!
//! `check_lines` is pure, so the testable half of the 32 KiB fix is tested here
//! rather than left to a live provider.

use pumpkin_pie::client::check_lines_for_testing;

#[test]
fn a_line_over_32_kib_is_an_error_not_a_dropped_line() {
    let fine = vec![("Steve", "%perm_prefix% hi")];
    assert!(check_lines_for_testing(&fine).is_ok(), "a normal line must pass");

    let huge = "x".repeat(32769);
    let over = vec![("Steve", huge.as_str())];
    let error = check_lines_for_testing(&over).expect_err("an over-long line must error");
    assert!(error.to_string().contains("32768"), "{error}");
}

#[test]
fn a_long_viewer_is_an_error_too() {
    // A Minecraft username is nowhere near `MAX_ID_LENGTH`, so this is defensive
    // rather than reachable. It was unchecked in the batch path at all, so it is
    // worth pinning.
    let long_name = "n".repeat(129);
    let over = vec![(long_name.as_str(), "fine")];
    let error = check_lines_for_testing(&over).expect_err("an over-long viewer must error");
    assert!(error.to_string().contains("128"), "{error}");
}

#[test]
fn one_bad_line_refuses_the_whole_batch() {
    let huge = "x".repeat(40000);
    let lines = vec![
        ("Steve", "%perm_prefix%"),
        ("Alex", "oops"),
        ("Steve", huge.as_str()),
    ];
    assert!(
        check_lines_for_testing(&lines).is_err(),
        "a batch with one over-long line must be refused whole, not trimmed"
    );
}

#[test]
fn an_empty_batch_is_fine() {
    let empty: [(&str, &str); 0] = [];
    assert!(check_lines_for_testing(&empty).is_ok());
}
