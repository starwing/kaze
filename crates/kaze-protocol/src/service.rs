use tracing::info;

use kaze_service::AsyncService;

use crate::message::{Message, PacketWithAddr};

/// A service that convert packet to message
#[derive(Debug, Clone, Copy)]
pub struct ToMessageService;

impl AsyncService<PacketWithAddr> for ToMessageService {
    type Response = Message;
    type Error = anyhow::Error;

    async fn serve(
        &self,
        req: PacketWithAddr,
    ) -> Result<Self::Response, Self::Error> {
        Ok(req.into())
    }
}

/// A service that drops messages and log it.
#[derive(Debug, Clone, Copy)]
pub struct SinkMessage;

impl AsyncService<Message> for SinkMessage {
    type Response = Option<Message>;
    type Error = anyhow::Error;

    async fn serve(&self, req: Message) -> anyhow::Result<Self::Response> {
        info!("message dropped: {:?}", req);
        Ok(None)
    }
}

impl AsyncService<Option<Message>> for SinkMessage {
    type Response = Option<Message>;
    type Error = anyhow::Error;

    async fn serve(
        &self,
        req: Option<Message>,
    ) -> anyhow::Result<Self::Response> {
        if let Some(msg) = req {
            info!("message dropped: {:?}", msg);
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        message::{Destination, Source},
        packet::Packet,
        proto::{Hdr, RetCode},
    };

    use super::*;

    #[tokio::test]
    async fn test_to_message_service() {
        assert!(
            ToMessageService
                .serve((
                    Packet::from_retcode(Hdr::default(), RetCode::RetOk),
                    None
                ))
                .await
                .is_ok()
        );

        assert!(
            SinkMessage
                .serve(Message::new_with_destination(
                    Packet::from_retcode(Hdr::default(), RetCode::RetOk),
                    Source::Host,
                    Destination::Drop,
                ))
                .await
                .is_ok()
        );

        assert!(SinkMessage.serve(None).await.is_ok());
    }
}
