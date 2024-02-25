use std::{collections::HashMap, time::Duration};
use std::sync::Arc;

use anyhow::Context;
use futures::future::join_all;
use log::{error, info};
use rand::seq::IteratorRandom;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::Receiver;
use tokio::sync::Mutex;

use crate::block::Block;

static MAX_NUM_PEERS: usize = 25;
static MAX_RETRY_TIMES: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloMessage {
    id: usize,
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewPeersMessage {
    peers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Hello(HelloMessage),
    NewPeers(NewPeersMessage),
}

/// A Node that is able to participate in consensus.
#[derive(Debug)]
pub struct Node {
    /// The genesis block.
    pub b0: Block,

    /// The locked block.
    pub b_lock: Block,

    /// The last executed block.
    pub b_exec: Block,

    /// The node ID
    pub id: String,

    /// The peer IP Port combinations. Thread safe.
    pub peers: Arc<Mutex<HashMap<String, usize>>>,

    /// The peer subset of connections. Thread safe.
    pub peer_cxns: Arc<Mutex<Vec<Option<TcpStream>>>>,

    /// The TCP listener for the node
    pub listener: TcpListener,

    /// Termination handler
    shutdown_rx: Receiver<()>,
}

impl Node {
    pub async fn new(
        id: String,
        port: usize,
        peers: Option<HashMap<String, usize>>,
        shutdown_rx: Receiver<()>,
    ) -> anyhow::Result<Self> {
        // TODO change this to be a real block
        // Make the genesis block.
        let b0 = Block::default();

        let ip_addr = format!("0.0.0.0:{}", port);
        let listener = TcpListener::bind(&ip_addr).await?;

        let peers = Arc::new(Mutex::new(peers.unwrap_or_default()));

        info!("Node {} bound on {}", id, ip_addr);

        info!("Node {} connecting to peer nodes", id);

        let mut rng = rand::thread_rng();
        // TODO handle node init failures and re-fetch from the map.
        let peer_cxns = Arc::new(Mutex::new(
            join_all(
                peers
                    .lock()
                    .await
                    .iter()
                    .choose_multiple(&mut rng, MAX_NUM_PEERS)
                    .into_iter()
                    .map(|(ip, port)| async move {
                        info!("Establishing connection to {}:{}", ip, port);
                        let mut retries = 0;
                        loop {
                            // Try to make the connection.
                            let stream = TcpStream::connect(format!("{}:{}", ip, port)).await;

                            // If this failed, then sleep for 5 seconds and try again.
                            if stream.is_err() {
                                tokio::time::sleep(Duration::from_secs(5)).await;
                                retries += 1;

                                // If we've exceeded our maximum configured number of retries, then return `None`.
                                if retries >= MAX_RETRY_TIMES {
                                    error!(
                                        "Node {}:{} failed to connect, exiting; error = {:?}",
                                        ip,
                                        port,
                                        stream.err().unwrap()
                                    );
                                    // TODO don't let this happen. Instead, try a different IP address and use that. If we've
                                    // exhausted our pool, log an error and move on.
                                    return None;
                                } else {
                                    error!("Node {}:{} failed to connect, retrying", ip, port);
                                }
                            } else {
                                // We can be reasonably sure that this unwrap will go successfully since we've
                                // already checked the error. If this somehow does not work,
                                // then we know something terible has happened.
                                return Some(stream.context(
                                    "Something went horribly wrong acquiring the TCP connection",
                                ).unwrap());
                            }
                        }
                    })
                    .collect::<Vec<_>>(),
            )
            .await,
        ));

        Ok(Self {
            b_lock: b0.clone(),
            b_exec: b0.clone(),
            b0,
            id,
            peers,
            peer_cxns,
            listener,
            shutdown_rx,
        })
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        loop {
            let accept_future = self.listener.accept();
            tokio::select! {
                accept_res = accept_future => {
                    match accept_res {
                        // TODO handle processing in a different function.
                        Ok((mut stream, _)) => {
                            // Grab a reference to listen for updates
                            let _peer_cxns = self.peer_cxns.clone();

                            tokio::spawn(async move {
                                let mut buffer = Vec::new();
                                match stream.read_to_end(&mut buffer).await {
                                    Ok(_) => match bincode::deserialize::<Message>(&buffer) {
                                        Ok(message) => {
                                            match message {
                                                Message::Hello(hello_message) => {
                                                    info!("Got hello message {:?}", hello_message);
                                                },
                                                Message::NewPeers(new_peers_message) => {
                                                    info!("Got new peer message {:?}", new_peers_message);
                                                },
                                            }
                                        }
                                        Err(e) => error!("Failed to deserialize message; error = {:?}", e),
                                    },
                                    Err(e) => error!("Failed to read data from socket; error = {:?}", e),
                                };
                            });
                        }
                        Err(e) => error!("Error accepting socket; error = {:?}", e),
                    }
                }
                _ = self.shutdown_rx.recv() => {
                    break;
                }
            }
        }

        Ok(())
    }
}
