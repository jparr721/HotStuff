//! The test harness configures the local distributed system and fires the containers.

use std::process::Command;

use anyhow::{bail, Context, Result};
use log::{error, info};

use crate::docker::{delete_dockerfiles, emit_docker_compose_file, emit_dockerfile};

pub struct TestDistributedSystem {
    #[allow(dead_code)]
    n_nodes: usize,
}

impl TestDistributedSystem {
    pub fn new(n_nodes: usize) -> Result<Self> {
        // Always re-create the docker-compose and Dockerfile files.
        emit_docker_compose_file(n_nodes)?;
        emit_dockerfile()?;
        Ok(Self { n_nodes })
    }

    /// init starts the local distributed system
    pub fn init(&self) -> Result<()> {
        info!("Building docker container");
        self.build_docker_container()?;
        info!("Starting docker-compose setup");
        self.start_distributed_system()
    }

    #[allow(dead_code)]
    pub fn n_nodes(&self) -> usize {
        self.n_nodes
    }

    fn build_docker_container(&self) -> Result<()> {
        let output = Command::new("docker")
            .arg("build")
            .arg("--rm")
            .args(["-f", "Dockerfile"])
            .args(["-t", "hotstuff"])
            .arg(".")
            .output()
            .context("Running docker build command for hotstuff container")
            .unwrap();

        if !output.status.success() {
            bail!(
                "Failed to build hotstuff docker image; error={}",
                std::str::from_utf8(&output.stderr).unwrap()
            );
        }

        Ok(())
    }

    fn start_distributed_system(&self) -> Result<()> {
        let output = Command::new("docker-compose")
            .args(["up", "-d"])
            .output()
            .unwrap();
        if !output.status.success() {
            bail!(
                "Failed to build docker-compose; error={}",
                std::str::from_utf8(&output.stderr).unwrap()
            );
        }
        Ok(())
    }
}

impl Drop for TestDistributedSystem {
    /// This function shuts down the docker-compose setup
    fn drop(&mut self) {
        let output = Command::new("docker-compose")
            .args(["down", "--rmi", "all"])
            .output()
            .unwrap();
        if !output.status.success() {
            error!(
                "error removing hotstuff cluster, leaving docker files around for manual cleanup; \
                error={}",
                std::str::from_utf8(&output.stderr).unwrap()
            );
        } else {
            delete_dockerfiles()
                .context("Deleting Dockerfile and docker-compose.yml")
                .expect("Failed to delete docker resources");
        }
    }
}
