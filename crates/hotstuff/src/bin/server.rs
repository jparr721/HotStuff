use anyhow::Result;
use clap::Parser;
use futures::Future;
use log::{info, warn};

use hotstuff::node;
use hotstuff::node::config::HotStuffConfig;

pub(crate) async fn run(args: HotStuffConfig, shutdown: impl Future) -> Result<()> {
    let (shutdown_tx, shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);
    let mut terminate =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();

    let id = uuid::Uuid::new_v4().to_string();
    let mut node = node::Node::new(id, None, shutdown_rx, Some(args)).await?;
    tokio::select! {
        res = node.run() => {
            res
        },
        _ = terminate.recv() => {
            warn!("terminating");
            let _ = shutdown_tx.send(()).await;
            Ok(())
        },
        _ = shutdown => {
            info!("shutting down");
            let _ = shutdown_tx.send(()).await;
            Ok(())
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();

    let args = HotStuffConfig::parse();
    run(args, tokio::signal::ctrl_c()).await
}

