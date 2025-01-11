/// Duration as a string in config and args
pub use duration_string::DurationString;

/// Parse a string to DurationString
pub fn parse_duration(s: &str) -> Result<DurationString, String> {
    s.parse::<DurationString>().map_err(|e| e.to_string())
}
