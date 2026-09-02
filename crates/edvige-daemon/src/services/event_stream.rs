use std::pin::Pin;
use edvige_proto::{
    event_stream_service_server::EventStreamService, DaemonEventProto, SubscribeEventsRequest,
};
use futures::Stream;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status};

use crate::coordinator::DaemonCoordinator;

pub struct EventStreamServiceImpl {
    coordinator: DaemonCoordinator,
}

impl EventStreamServiceImpl {
    pub fn new(coordinator: DaemonCoordinator) -> Self {
        Self { coordinator }
    }
}

#[tonic::async_trait]
impl EventStreamService for EventStreamServiceImpl {
    type SubscribeEventsStream =
        Pin<Box<dyn Stream<Item = Result<DaemonEventProto, Status>> + Send + 'static>>;

    async fn subscribe_events(
        &self,
        _request: Request<SubscribeEventsRequest>,
    ) -> Result<Response<Self::SubscribeEventsStream>, Status> {
        let rx = self.coordinator.events().subscribe();
        let broadcast_stream = BroadcastStream::new(rx);

        let output_stream = broadcast_stream.filter_map(|res| match res {
            Ok(event) => Some(Ok(event)),
            Err(_) => None, // Ignore lagged errors
        });

        Ok(Response::new(Box::pin(output_stream) as Self::SubscribeEventsStream))
    }
}

