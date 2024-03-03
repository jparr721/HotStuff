use std::net::Ipv4Addr;

use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(version, about, long_about = None)]
pub struct HotStuffConfig {
    /// The port to connect to
    #[arg(long, default_value_t = 2000)]
    pub hotstuff_port: u16,

    /// The maximum number of connections that this node can have.
    #[arg(long, default_value_t = 25)]
    pub n_max_connections: usize,

    /// The maximum number of retries this node should attempt when connecting to a peer before giving up.
    #[arg(long, default_value_t = 25)]
    pub n_max_retries: usize,

    /// The IP address of the bootnode.
    #[arg(long, default_value = "172.20.0.2")]
    pub bootnode_ip_addr: Ipv4Addr,

    /// The port of the bootnode.
    #[arg(long, default_value_t = 2000)]
    pub bootnode_port: u16,

    /// Indicates whether this node is a bootnode.
    #[arg(long, default_value_t = false)]
    pub is_bootnode: bool,
}

impl Default for HotStuffConfig {
    fn default() -> Self {
        Self {
            hotstuff_port: 2000,
            n_max_connections: 25,
            n_max_retries: 5,
            bootnode_ip_addr: "172.20.0.2".parse().unwrap(),
            bootnode_port: 2000,
            is_bootnode: false,
        }
    }
}
