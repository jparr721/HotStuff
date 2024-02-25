use log::info;

use crate::harness::TestDistributedSystem;
use crate::traits::Test;

#[derive(Debug)]
pub struct SimpleMessage {}

impl SimpleMessage {
    pub fn new() -> Self {
        Self {}
    }
}

impl Test for SimpleMessage {
    fn run(&self, n_nodes: usize) -> anyhow::Result<()> {
        let ds = TestDistributedSystem::new(n_nodes)?;
        ds.init()?;
        info!("Done initializing, ready to begin testing");

        Ok(())
    }
}
