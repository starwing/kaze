mod options;

use kaze_plugin::{protocol::message::Message, service::AsyncService, Plugin};
pub use options::Options;
use tracing::{debug, error, info, trace, warn};

#[derive(Debug, Clone, Copy)]
pub struct LogService;

impl Plugin for LogService {}

impl AsyncService<Message> for LogService {
    type Response = Option<Message>;
    type Error = anyhow::Error;

    async fn serve(&self, msg: Message) -> anyhow::Result<Self::Response> {
        fn fetch_body(msg: &Message) -> Option<&str> {
            if let Ok(msg) = std::str::from_utf8(msg.packet().body()) {
                Some(msg)
            } else {
                error!(
                    hdr = ?msg.packet().hdr(),
                    body = ?msg.packet().body(),
                    "Failed to parse log message"
                );
                None
            }
        }
        macro_rules! match_log {
            ( $ty:ident, $msg:ident ) => {{
                if let Some(msg) = fetch_body(&$msg) {
                    $ty!(body = msg, "Log message");
                }
                Ok(None)
            }};
        }
        match msg.packet().hdr().body_type.as_str() {
            "log" => match_log!(info, msg),
            "log_trace" => match_log!(trace, msg),
            "log_debug" => match_log!(debug, msg),
            "log_info" => match_log!(info, msg),
            "log_warn" => match_log!(warn, msg),
            "log_error" => match_log!(error, msg),
            _ => Ok(Some(msg)),
        }
    }
}
