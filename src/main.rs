use anyhow;

mod block;
mod crypto;
mod node;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();

    let mut node = node::Node::new("1".to_owned(), 2000, None).await?;
    node.run().await
}
