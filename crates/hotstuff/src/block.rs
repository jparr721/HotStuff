use serde::{Deserialize, Serialize};

use crate::crypto::{zero_digest, Digest};

#[derive(Debug, Clone)]
pub struct Transaction {}

#[derive(Debug, Clone)]
pub struct QuorumCertificate {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    parent_hashes: Vec<Digest>,
    tx_hashes: Vec<Digest>,

    hash: Digest,
    parents: Vec<Block>,
    height: u32,
    delivered: bool,
    decision: bool,
}

impl Default for Block {
    fn default() -> Self {
        Self {
            parent_hashes: Default::default(),
            tx_hashes: Default::default(),
            hash: zero_digest(),
            parents: Default::default(),
            height: Default::default(),
            delivered: Default::default(),
            decision: Default::default(),
        }
    }
}

impl Block {}
