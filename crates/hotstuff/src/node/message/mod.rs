use std::collections::HashMap;
use std::net::SocketAddr;

use derive_more::Display;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Display)]
#[display(fmt = "HelloMessage(id={}, port={})", id, port)]
pub struct HelloMessage {
    pub id: String,
    pub port: u16,
}

impl HelloMessage {
    pub fn new(id: String, port: u16) -> Self {
        Self { id, port }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Display)]
#[display(fmt = "HelloResponseMessage(peer_list={:?})", peer_list)]
pub struct HelloResponseMessage {
    pub peer_list: HashMap<String, SocketAddr>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Display)]
#[serde(untagged)]
pub enum Message {
    Hello(HelloMessage),
    HelloResponse(HelloResponseMessage),
}
