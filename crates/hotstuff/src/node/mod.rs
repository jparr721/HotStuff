use std::collections::HashMap;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Error, Result};
use futures::future::join_all;
use futures::SinkExt;
use http::{Request, Response};
use log::{debug, error, info};
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::Receiver;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time;
use tokio_stream::StreamExt;
use tokio_util::codec::Framed;

use crate::async_helpers::{
    connect_valid_subset_ip_range, send_request_with_timeout, try_connect_with_retry,
};
use crate::block::Block;
use crate::http::{read, Http};
use crate::node::config::HotStuffConfig;
use crate::node::message::{HelloMessage, HelloResponseMessage, Message};

pub mod config;
pub mod message;

static MAX_NUM_PEERS: usize = 25;
static MAX_RETRY_TIMES: usize = 5;

#[derive(Debug)]
struct NodeStorage {
    /// The peer IP Port combinations. Thread safe.
    pub peers: HashMap<String, SocketAddr>,

    /// The peer subset of connections. Thread safe.
    pub peer_cxns: HashMap<String, SocketAddr>,
}

impl NodeStorage {
    pub async fn update_peer(&mut self, id: String, sock: SocketAddr) -> Result<()> {
        // Try to insert the new value, getting the old one if found.
        let prev_peer = self.peers.insert(id.clone(), sock);

        // If we insert a new peer, and we're below our max connection limit, then attempt to connect.
        if prev_peer.is_none() && self.peer_cxns.len() < MAX_NUM_PEERS {
            try_connect_with_retry(&sock, MAX_RETRY_TIMES)
                .await
                .context(format!("Connecting to new peer at {}", sock))?;

            // Add the valid connection to the peer list for broadcast messaging
            self.peer_cxns.insert(id, sock);
        }

        // If the value already existed _and_ it was the same as an already existing entry, notify
        // that someone is pushing duplicates around.
        // if prev_peer.is_some() && sock == prev_peer.unwrap() {
        //     bail!("Attempted to insert a duplicate entry into the peers table.");
        // }

        // Otherwise, we're good to go.
        Ok(())
    }
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

    /// The TCP listener for the node
    pub listener: TcpListener,

    /// The storage of the node, including any modifiable data.
    storage: Arc<Mutex<NodeStorage>>,

    /// Active task in the server
    tasks: Vec<JoinHandle<()>>,

    /// Termination handler
    shutdown_rx: Receiver<()>,

    /// The configuration for a consensus node
    consensus_config: HotStuffConfig,
}

impl Node {
    pub async fn new(
        id: String,
        peers: Option<HashMap<String, SocketAddr>>,
        shutdown_rx: Receiver<()>,
        config: Option<HotStuffConfig>,
    ) -> Result<Self> {
        let config = config.unwrap_or_default();

        // TODO change this to be a real block
        // Make the genesis block.
        let b0 = Block::default();

        let ip_addr = format!("0.0.0.0:{}", config.hotstuff_port);
        let listener = TcpListener::bind(&ip_addr).await?;

        let peers = peers.unwrap_or_default();

        info!("Node {} bound on {}; config = {:?}", id, ip_addr, config);

        let peer_cxns = match peers.len() {
            0 => HashMap::new(),
            _ => {
                connect_valid_subset_ip_range(&peers, MAX_NUM_PEERS, Some(MAX_RETRY_TIMES)).await?
            }
        };

        let storage = NodeStorage { peers, peer_cxns };

        Ok(Self {
            b_lock: b0.clone(),
            b_exec: b0.clone(),
            b0,
            id,
            storage: Arc::new(Mutex::new(storage)),
            listener,
            tasks: Vec::new(),
            shutdown_rx,
            consensus_config: config,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut interval = time::interval(Duration::from_secs(5));
        loop {
            let accept_future = self.listener.accept();
            tokio::select! {
                _ = interval.tick() => {
                    // Only ping the bootnode if we aren't a bootnode.
                    if !self.consensus_config.is_bootnode {
                        let storage = self.storage.clone();
                        let config = self.consensus_config.clone();
                        let id = self.id.clone();

                        self.tasks.push(tokio::spawn(async move {
                            let mut storage_guard = storage.lock().await;
                            if let Err(e) = Node::sync_to_bootnode(id, config, &mut storage_guard).await {
                                error!("Failed to sync to bootnode; error = {:?}", e);
                            }
                        }));
                    }
                },
                accept_res = accept_future => {
                    match accept_res {
                        Ok((stream, _)) => {
                            let storage = self.storage.clone();

                            self.tasks.push(tokio::spawn(async move {
                                if let Err(e) = read(stream).await {
                                    error!("Failed to process input stream; error = {}", e);
                                }
                            //     // TODO BUG BUG BUG BUG BUG
                            //     // This CANNOT handle messages > 4096 and creates a whole mess.
                            //     // We need to handle larger messages using something like HTTP.
                            //     let mut buffer = [0; 4096];
                            //     let mut storage_guard = storage.lock().await;
                            //
                            //     // TODO merge this into a helper function.
                            //     match stream.read(&mut buffer[..]).await {
                            //         Ok(n) => match serde_json::from_slice::<Message>(&buffer[0..n]) {
                            //             Ok(message) => if let Err(e) = Node::handle_message(
                            //                 &mut stream, message, &mut storage_guard).await {
                            //                 error!("Failed to process message; error = {:?}", e);
                            //             },
                            //             Err(e) => error!("Failed to deserialize message; error = {:?}; got = {}", e, String::from_utf8_lossy(&buffer)),
                            //         },
                            //         Err(e) => error!("Failed to read data from socket; error = {:?}", e),
                            //     };
                            }));
                        }
                        Err(e) => error!("Error accepting socket; error = {:?}", e),
                    }
                }
                _ = self.shutdown_rx.recv() => {
                    // Explicitly make sure all the tasks complete before shutdown
                    info!("Waiting for {} tasks to complete before shutdown", self.tasks.len());
                    join_all(&mut self.tasks).await;
                    break;
                }
            }
        }

        Ok(())
    }

    async fn sync_to_bootnode(
        id: String,
        config: HotStuffConfig,
        node_storage: &mut NodeStorage,
    ) -> Result<()> {
        info!("Requesting new peers list");

        let mut stream = TcpStream::connect(format!(
            "{}:{}",
            config.bootnode_ip_addr, config.bootnode_port
        ))
        .await?;

        let http_codec = Http::<Message>::new();
        let mut transport = Framed::new(stream, http_codec);

        let hello = Message::Hello(HelloMessage::new(id, config.hotstuff_port));
        let request = Request::builder()
            .method("POST")
            .header("Content-Type", "application/json")
            .body(hello)?;

        transport.send(request).await?;

        // Await the response
        let response = transport.next().await;

        match response {
            Some(Ok(response)) => {
                // Process the response here
                info!("Received response: {:?}", response);
            }
            Some(Err(e)) => return Err(e), // Convert the error into your function's error type
            None => {
                // Handle the case where no response is received
                error!("No response received from the server.");
            }
        }

        // stream.write_all(&request).await?;

        // let ser = serde_json::to_vec(&hello)?;
        // let response = send_request_with_timeout(&mut stream, &ser, None).await?;
        // let response = serde_json::from_slice::<HelloResponseMessage>(&response)?;
        // debug!("Response {}", response);

        // info!("Got new peer list, updating local storage");
        // for (id, sock) in response.peer_list.into_iter() {
        //     node_storage.update_peer(id, sock).await?;
        // }
        // info!("Update occurred successfully");

        Ok(())
    }

    async fn handle_message(
        stream: &mut TcpStream,
        message: Message,
        node_storage: &mut NodeStorage,
    ) -> Result<()> {
        debug!("Handling message; message = {}", message);
        match message {
            Message::Hello(hello_message) => {
                let mut peer = stream.peer_addr()?;
                peer.set_port(hello_message.port);

                let update_result = node_storage.update_peer(hello_message.id, peer).await;
                if update_result.is_err() {
                    error!(
                        "Error processing hello message; error = {:?}",
                        update_result.err()
                    );
                }

                let response = HelloResponseMessage {
                    peer_list: node_storage.peers.clone(),
                };
                stream
                    .write_all(&serde_json::to_vec(&response).unwrap())
                    .await?;

                Ok(())
            }
            Message::HelloResponse(hello_response_message) => {
                info!("Updating local peer list.");
                Ok(())
            }
        }
    }
}
