use anyhow::Result;

pub trait Test {
    fn run(&self, n_nodes: usize) -> Result<()>;
}
