mod options;

use kaze_plugin::{protocol::message::Message, service::AsyncService, Plugin};
pub use options::Options;

#[derive(Debug, Clone, Copy)]
pub struct PrometheusService;

impl Plugin for PrometheusService {}

impl AsyncService<Message> for PrometheusService {
    type Response = Option<Message>;
    type Error = anyhow::Error;

    async fn serve(&self, msg: Message) -> anyhow::Result<Self::Response> {
        Ok(Some(msg))
    }
}
