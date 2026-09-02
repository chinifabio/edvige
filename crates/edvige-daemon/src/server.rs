use std::path::{Path, PathBuf};
use edvige_proto::{
    AccountServiceServer, EventStreamServiceServer, FolderServiceServer, MessageServiceServer,
    MutationServiceServer, OutboxServiceServer,
};
use tokio::net::UnixListener;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::Server;

use crate::coordinator::DaemonCoordinator;
use crate::services::{
    AccountServiceImpl, EventStreamServiceImpl, FolderServiceImpl, MessageServiceImpl,
    MutationServiceImpl, OutboxServiceImpl,
};

pub struct DaemonServer {
    coordinator: DaemonCoordinator,
}

impl DaemonServer {
    pub fn new(coordinator: DaemonCoordinator) -> Self {
        Self { coordinator }
    }

    pub fn router(&self) -> tonic::transport::server::Router {
        Server::builder()
            .add_service(AccountServiceServer::new(AccountServiceImpl::new(
                self.coordinator.clone(),
            )))
            .add_service(FolderServiceServer::new(FolderServiceImpl::new(
                self.coordinator.clone(),
            )))
            .add_service(MessageServiceServer::new(MessageServiceImpl::new(
                self.coordinator.clone(),
            )))
            .add_service(MutationServiceServer::new(MutationServiceImpl::new(
                self.coordinator.clone(),
            )))
            .add_service(OutboxServiceServer::new(OutboxServiceImpl::new(
                self.coordinator.clone(),
            )))
            .add_service(EventStreamServiceServer::new(EventStreamServiceImpl::new(
                self.coordinator.clone(),
            )))
    }

    pub async fn serve_uds(
        &self,
        socket_path: impl AsRef<Path>,
        shutdown: impl std::future::Future<Output = ()>,
    ) -> anyhow::Result<()> {
        let socket_path: PathBuf = socket_path.as_ref().to_path_buf();
        if let Some(parent) = socket_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        if tokio::fs::try_exists(&socket_path).await.unwrap_or(false) {
            let _ = tokio::fs::remove_file(&socket_path).await;
        }

        let uds = UnixListener::bind(&socket_path)?;
        let uds_stream = UnixListenerStream::new(uds);
        tracing::info!("Edvige daemon listening on UDS: {}", socket_path.display());

        self.router()
            .serve_with_incoming_shutdown(uds_stream, shutdown)
            .await?;

        let _ = tokio::fs::remove_file(&socket_path).await;
        Ok(())
    }

    pub async fn serve_tcp(
        &self,
        addr: std::net::SocketAddr,
        shutdown: impl std::future::Future<Output = ()>,
    ) -> anyhow::Result<()> {
        tracing::info!("Edvige daemon listening on TCP: {}", addr);
        self.router()
            .serve_with_shutdown(addr, shutdown)
            .await?;
        Ok(())
    }
}

