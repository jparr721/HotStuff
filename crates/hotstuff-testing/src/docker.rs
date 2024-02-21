//! Handles the types and creation of docker resource configuration files.

use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::Write;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct DockerComposeService {
    image: String,
    restart: String,
    ports: Vec<String>,
    networks: HashMap<String, NetworkConfig>,
}

impl DockerComposeService {
    pub fn new(port_mapping: String, ip_addr: String) -> Self {
        Self {
            image: "hotstuff".to_string(),
            restart: "always".to_string(),
            ports: vec![port_mapping],
            networks: HashMap::from([("stuffnet".to_string(), NetworkConfig::new(ip_addr))]),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct NetworkConfig {
    ipv4_address: String,
}

impl NetworkConfig {
    pub fn new(ipv4_address: String) -> Self {
        Self { ipv4_address }
    }
}

/// A docker-compose spec for generating distributed tooling.
#[derive(Debug, Serialize, Deserialize)]
struct DockerCompose {
    version: String,
    services: HashMap<String, DockerComposeService>,
    networks: HashMap<String, NetworkDetails>,
}

impl DockerCompose {
    pub fn new(n_nodes: usize) -> Self {
        let services = HashMap::from_iter(
            (0..n_nodes)
                .map(|ii| {
                    (
                        format!("node{}", ii),
                        DockerComposeService::new(
                            format!("{}:2000", 2000 + ii),
                            format!("172.20.0.{}", ii + 2),
                        ),
                    )
                })
                .collect::<Vec<_>>(),
        );

        Self {
            version: "3".to_string(),
            services,
            networks: HashMap::from([("stuffnet".to_string(), NetworkDetails::default())]),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct NetworkDetails {
    driver: String,
    ipam: Ipam,
}

impl Default for NetworkDetails {
    fn default() -> Self {
        NetworkDetails {
            driver: "bridge".to_string(),
            ipam: Ipam::default(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Ipam {
    driver: String,
    config: Vec<IpamConfig>,
}

impl Default for Ipam {
    fn default() -> Self {
        Self {
            driver: "default".to_string(),
            config: vec![IpamConfig::default()],
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct IpamConfig {
    subnet: String,
}

impl Default for IpamConfig {
    fn default() -> Self {
        Self {
            subnet: "172.20.0.0/16".to_string(),
        }
    }
}

pub fn emit_dockerfile() -> Result<()> {
    let file_contents = r#"
# Stage 1: Build the binary
FROM rust:latest as builder

# Copy the sources over
WORKDIR /hotstuff
COPY . .

# Build the project
RUN cargo build --release

# Stage 2: Setup the runtime environment
# Debian latest is bookworm, which is what rust:latest is based off of.
FROM debian:bookworm

# Copy the binary from the builder stage
COPY --from=builder /hotstuff/target/release/hotstuff-server /usr/local/bin/hotstuff-server

# Startup command runs the server node
CMD ["hotstuff-server"]"#;

    let mut dockerfile_handle = File::create("Dockerfile").context("Opening Dockerfile")?;
    dockerfile_handle
        .write(file_contents.as_bytes())
        .context("Writing Dockerfile")?;
    Ok(())
}

pub fn emit_docker_compose_file(n_nodes: usize) -> Result<()> {
    let docker_compose = DockerCompose::new(n_nodes);
    let mut compose_file_handle =
        File::create("docker-compose.yml").context("Opening docker-compose.yml")?;
    compose_file_handle
        .write(
            serde_yaml::to_string(&docker_compose)
                .context("Serializing docker_compose object")?
                .as_bytes(),
        )
        .context("Writing docker-compose.yml")?;
    Ok(())
}

pub fn delete_dockerfiles() -> Result<()> {
    fs::remove_file("Dockerfile")?;
    fs::remove_file("docker-compose.yml")?;
    Ok(())
}
