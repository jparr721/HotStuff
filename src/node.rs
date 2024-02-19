use serde::{Deserialize, Serialize};

use crate::block::Block;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloMessage {
    id: u8,
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Hello(HelloMessage),
}

/// A Node that is able to participate in consensus.
#[derive(Debug, Clone)]
struct Node {
    // The genesis block.
    pub b0: Block,

    // The locked block.
    pub b_lock: Block,

    // The last executed block.
    pub b_exec: Block,
}

impl Node {
    pub fn new() -> Self {
        // TODO change this to be a real block
        // Make the genesis block.
        let b0 = Block::default();

        Self {
            b_lock: b0.clone(),
            b_exec: b0.clone(),
            b0,
        }
    }

    async fn run() {
        loop {}
    }
}
