use std::net::{AddrParseError, SocketAddr};

pub use duration_string::DurationString;

/// Parse a string to DurationString
pub fn parse_duration(s: &str) -> Result<DurationString, String> {
    s.parse::<DurationString>().map_err(|e| e.to_string())
}

/// Parse a string to SocketAddr
pub fn parse_socket_addr(s: &str) -> Result<SocketAddr, String> {
    s.parse().map_err(|e: AddrParseError| e.to_string())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::parse_duration;

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("1s"), Ok(Duration::from_secs(1).into()));
        assert_eq!(
            parse_duration("1s100ms"),
            Ok(Duration::from_millis(1100).into())
        );
        assert_eq!(parse_duration("1m"), Ok(Duration::from_secs(60).into()));
        assert_eq!(parse_duration("1h"), Ok(Duration::from_secs(3600).into()));
        assert_eq!(
            parse_duration("1d"),
            Ok(Duration::from_secs(86400).into())
        );
        assert_eq!(
            parse_duration("1w"),
            Ok(Duration::from_secs(604800).into())
        );
    }
}
