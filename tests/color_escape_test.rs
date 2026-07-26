//! Tests to verify that ANSI color codes are not present in API responses
//!
//! This test suite validates that Display implementations which are serialized
//! to JSON (via API endpoints or error responses) do not contain ANSI escape sequences.
//!
//! ## Background
//! When colored strings are serialized to JSON, ANSI codes like `\x1b[31m` become
//! escaped as `\\x1b[31m`, appearing as literal text instead of colors.
//!
//! ## Fixed Issues
//! - `VERSION_INFO`: Removed colors to fix /api/version endpoint
//! - `Reason::Display`: Removed colors to fix `InvalidCookie` error responses

#[cfg(test)]
mod tests {
    use clewdr::config::Reason;

    /// ANSI escape sequence pattern
    const ANSI_ESCAPE_PATTERN: &str = "\x1b[";

    #[test]
    fn test_reason_display_no_ansi_codes() {
        // Reason::Display should NOT contain ANSI escape codes
        // This is used in InvalidCookie errors that go into JSON responses

        let test_cases = vec![
            (Reason::NormalPro, "Normal Pro account"),
            (Reason::Disabled, "Organization Disabled"),
            (Reason::Free, "Free account"),
            (Reason::Banned, "Banned"),
            (Reason::Null, "Null"),
            (Reason::Restricted(1735689600), "Restricted/Warning"),
            (Reason::TooManyRequest(1735689600), "429 Too many request"),
        ];

        for (reason, expected_substring) in test_cases {
            let display_text = reason.to_string();

            assert!(
                !display_text.contains(ANSI_ESCAPE_PATTERN),
                "Reason::{reason:?} Display contains ANSI escape codes: {display_text}"
            );

            assert!(
                display_text.contains(expected_substring),
                "Reason::{reason:?} Display doesn't contain expected text '{expected_substring}': {display_text}"
            );
        }
    }

    #[test]
    fn test_reason_with_timestamp_format() {
        // Verify that timestamps in Reason are formatted correctly without colors
        let restricted = Reason::Restricted(1735689600); // 2025-01-01 00:00:00 UTC
        let display_text = restricted.to_string();

        // Should contain UTC formatted date
        assert!(display_text.contains("UTC"));
        assert!(display_text.contains("2025")); // Year
        assert!(!display_text.contains(ANSI_ESCAPE_PATTERN));

        let too_many_request = Reason::TooManyRequest(1735689600);
        let display_text = too_many_request.to_string();

        assert!(display_text.contains("UTC"));
        assert!(display_text.contains("2025"));
        assert!(!display_text.contains(ANSI_ESCAPE_PATTERN));
    }

    #[test]
    fn test_json_serialization_no_escaped_ansi() {
        // Test that when Reason is serialized to JSON, there are no escaped ANSI codes
        use serde_json;

        let reason = Reason::TooManyRequest(1735660800);
        let json_str = serde_json::to_string(&reason).unwrap();

        // JSON should not contain escaped ANSI codes like \\x1b or \\u001b
        assert!(!json_str.contains("\\x1b"));
        assert!(!json_str.contains("\\u001b"));
        assert!(!json_str.contains('\x1b'));
    }
}
