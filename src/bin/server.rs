use anyhow;
use futures::Future;
use log::{info, warn};

pub async fn run(shutdown: impl Future) -> anyhow::Result<()> {
    let (shutdown_tx, shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);
    let mut terminate =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();

    let id = uuid::Uuid::new_v4().to_string();
    let mut node = node::Node::new(id, 2000, None, shutdown_rx).await?;
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
async fn main() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();

    run(tokio::signal::ctrl_c()).await
}
