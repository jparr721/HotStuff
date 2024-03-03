use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Error, Result};
use futures::future::join_all;
use futures::SinkExt;
use http::{Request, Response, StatusCode};
use log::{debug, error, info};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::Receiver;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time;
use tokio_stream::StreamExt;
use tokio_util::codec::Framed;

use crate::async_helpers::{connect_valid_subset_ip_range, try_connect_with_retry};
use crate::block::Block;
use crate::http::Http;
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
                        Ok((stream, src)) => {
                            let storage = self.storage.clone();
                            let http_codec = Http::<Message>::new();
                            let mut transport = Framed::new(stream, http_codec);
                            self.tasks.push(tokio::spawn(async move {
                                let mut storage_guard = storage.lock().await;
                                while let Some(packet) = transport.next().await {
                                    match packet {
                                        Ok(message) => match message {
                                            crate::http::codec::HttpMessage::Request(req) => {
                                                let message = req.into_body();
                                                Node::handle_message(src, &mut transport, message, &mut storage_guard).await.unwrap()
                                            }
                                            crate::http::codec::HttpMessage::Response(res) => {
                                                let message = res.into_body();
                                                Node::handle_message(src, &mut transport, message, &mut storage_guard).await.unwrap()
                                            },
                                        },
                                        Err(e) => error!("Failed to process request; error = {}", e),
                                    }
                                }
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

        let bootnode_ip_and_port = format!("{}:{}", config.bootnode_ip_addr, config.bootnode_port);
        let dst_sock: SocketAddr = bootnode_ip_and_port.parse().context(format!(
            "Creating socket address for bootnode {}",
            bootnode_ip_and_port
        ))?;

        let stream = TcpStream::connect(dst_sock).await?;

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
                let response = response.into_response()?;
                info!("Response received.");
                Node::handle_message(dst_sock, &mut transport, response.into_body(), node_storage)
                    .await
            }
            Some(Err(e)) => Err(e),
            None => {
                // Handle the case where no response is received
                bail!("No response received from the server.");
            }
        }
    }

    async fn handle_message(
        src: SocketAddr,
        transport: &mut Framed<TcpStream, Http<Message>>,
        message: Message,
        node_storage: &mut NodeStorage,
    ) -> Result<()> {
        debug!("Handling message; message = {}", message);
        match message {
            Message::Hello(hello_message) => {
                let mut src = src;
                src.set_port(hello_message.port);

                // Update the peer connection table.
                node_storage.update_peer(hello_message.id, src).await?;

                let resp_msg = HelloResponseMessage {
                    peer_list: node_storage.peers.clone(),
                };
                let res = Response::builder()
                    .header("Content-Type", "application/json")
                    .status(StatusCode::OK)
                    .body(Message::HelloResponse(resp_msg))
                    .map_err(Error::from)?;
                transport.send(res).await
            }
            Message::HelloResponse(hello_response_message) => {
                info!("Updating local peer list.");
                for (id, sock) in hello_response_message.peer_list.into_iter() {
                    node_storage.update_peer(id, sock).await?;
                }
                Ok(())
            }
        }
    }
}
