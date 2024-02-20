use anyhow;
use futures::Future;
use log::info;

mod block;
mod crypto;
mod node;

pub async fn run(shutdown: impl Future) -> anyhow::Result<()> {
    let (shutdown_tx, shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);
    let mut terminate =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
    let mut node = node::Node::new("1".to_owned(), 2000, None, shutdown_rx).await?;
    tokio::select! {
        res = node.run() => {
            res
        },
        _ = terminate.recv() => {
            info!("terminating");
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
