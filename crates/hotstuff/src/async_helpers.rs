use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{bail, Context, Error, Result};
use log::error;
use rand::prelude::SliceRandom;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time;

pub async fn with_timeout<F, Fut, T>(duration: Duration, f: F) -> Result<T>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    match time::timeout(duration, f()).await {
        Ok(result) => result.map_err(Error::from),
        Err(_) => bail!("Request timed out"),
    }
}

pub async fn send_request_with_timeout(
    stream: &mut TcpStream,
    request_data: &[u8],
    duration: Option<Duration>,
) -> Result<Vec<u8>> {
    let timeout_duration = duration.unwrap_or(Duration::from_secs(5));

    // Send request with timeout
    with_timeout(timeout_duration, || async {
        stream.write_all(request_data).await.map_err(Error::from)
    })
    .await?;

    // Read response with timeout
    let mut buffer = Vec::new();
    with_timeout(timeout_duration, || async {
        stream.read_to_end(&mut buffer).await.map_err(Error::from)
    })
    .await?;

    Ok(buffer)
}

pub async fn try_connect_with_retry(
    sock: &SocketAddr,
    max_retry_times: usize,
) -> Result<TcpStream> {
    loop {
        let stream = TcpStream::connect(sock).await;

        let mut retries = 0;
        if stream.is_err() {
            tokio::time::sleep(Duration::from_secs(5)).await;
            retries += 1;

            // If we've exceeded our maximum configured number of retries, then return `None`.
            if retries >= max_retry_times {
                error!(
                    "Node {} failed to connect, exiting; error = {:?}",
                    sock,
                    stream.err().unwrap()
                );
                bail!("Failed to connect to node.")
            }
            error!("Node {} failed to connect, retrying", sock);
        } else {
            // We can be reasonably sure that this unwrap will go successfully since we've
            // already checked the error. If this somehow does not work,
            // then we know something terrible has happened.
            return stream.context("Something went horribly wrong acquiring the TCP connection");
        }
    }
}

pub async fn connect_valid_subset_ip_range(
    peers: &HashMap<String, SocketAddr>,
    max_num_connections: usize,
    max_retry_times: Option<usize>,
) -> Result<HashMap<String, SocketAddr>> {
    let max_retry_times = max_retry_times.unwrap_or(5);
    let mut rng = rand::thread_rng();

    // Get the connections as results, then we can process them below.
    let mut selected = HashMap::new();
    let mut attempted_keys: HashSet<SocketAddr> = HashSet::new();

    // Get the candidates as a vector that we can arbitrarily select into. Cloning the memory
    // is not explicitly needed here, but it is tiny, and makes things a little easier
    // to express.
    let candidates: Vec<(String, SocketAddr)> =
        peers.iter().map(|(k, v)| (k.clone(), *v)).collect();

    while selected.len() < max_num_connections && attempted_keys.len() < peers.len() {
        if let Some((id, sock)) = candidates.choose(&mut rng) {
            // This isn't great for large N, but conceivably this will never run that often.
            if attempted_keys.contains(sock) {
                // Skip any keys we've already tried
                continue;
            }

            attempted_keys.insert(*sock);

            let stream = try_connect_with_retry(sock, max_retry_times).await;
            if stream.is_ok() {
                selected.insert(id.clone(), *sock);
            }
        }
    }

    // We want an error if we connect to no nodes
    match selected.len() {
        0 => bail!("Failed to connect to any nodes."),
        _ => Ok(selected),
    }
}
