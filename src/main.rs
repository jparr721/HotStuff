use anyhow;

pub mod block;
pub mod crypto;
pub mod node;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    Ok(())
}
