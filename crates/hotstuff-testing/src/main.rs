use clap::Parser;

use crate::bootnode_init_and_stabilize::BootnodeInitAndStabilize;
use crate::traits::Test;

mod docker;
mod harness;
mod bootnode_init_and_stabilize;
mod traits;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Simple message test
    #[arg(long, default_value_t = false)]
    simple_message: bool,

    /// How many nodes?
    #[arg(long, default_value_t = 4)]
    n_nodes: usize,
}

fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();

    let args = Args::parse();

    if args.simple_message {
        let bootnode_test = BootnodeInitAndStabilize::new();
        bootnode_test.run(args.n_nodes).unwrap();
    }
}
