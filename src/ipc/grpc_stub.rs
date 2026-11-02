//! Phase 2 gRPC Agent Server (Tonic) Trait Boundary Stub
//!
//! When the `grpc-server` Cargo feature is enabled, this module provides
//! high-throughput, typed protobuf/gRPC bidirectional streaming for LLM agents,
//! multi-agent orchestrators, and external tools.
//!
//! The gRPC service decouples completely from the main rendering and physics loop
//! by consuming the exact same `tokio::sync::mpsc::Sender<PetCommand>` command channel
//! and `tokio::sync::watch::Receiver<DesktopSnapshot>` state channel as the Axum HTTP server.

#![allow(dead_code)]

use super::{DesktopSnapshot, PetCommand};
use tokio::sync::{mpsc, watch};

#[cfg(feature = "grpc-server")]
pub struct DesktopPetGrpcService {
    _cmd_tx: mpsc::Sender<PetCommand>,
    _state_rx: watch::Receiver<DesktopSnapshot>,
}

#[cfg(feature = "grpc-server")]
impl DesktopPetGrpcService {
    pub fn new(cmd_tx: mpsc::Sender<PetCommand>, state_rx: watch::Receiver<DesktopSnapshot>) -> Self {
        tracing::info!("Initializing tonic gRPC desktop agent service");
        Self {
            _cmd_tx: cmd_tx,
            _state_rx: state_rx,
        }
    }

    pub async fn run_server(self, addr: std::net::SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("Starting gRPC agent service on {}", addr);
        // Phase 2:
        // Server::builder()
        //     .add_service(DesktopPetServiceServer::new(self))
        //     .serve(addr)
        //     .await?;
        Ok(())
    }
}

#[cfg(not(feature = "grpc-server"))]
pub struct GrpcServerStub;

#[cfg(not(feature = "grpc-server"))]
impl GrpcServerStub {
    pub const COMPILED_IN: bool = false;

    pub fn explain_channels(
        _cmd_tx: &mpsc::Sender<PetCommand>,
        _state_rx: &watch::Receiver<DesktopSnapshot>,
    ) {
        tracing::debug!("gRPC server stub: channel wiring ready for Phase 2 tonic server");
    }
}
