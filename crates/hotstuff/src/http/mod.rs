use std::collections::HashMap;
use std::marker::PhantomData;

use anyhow::{bail, Error, Result};
use futures::SinkExt;
use http::{Response, StatusCode};
use log::{error, info};
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use tokio::net::TcpStream;
use tokio_stream::StreamExt;
use tokio_util::bytes::Buf;
use tokio_util::codec::{Decoder, Encoder, Framed};

use crate::http::codec::HttpMessage;
use crate::node::message::{HelloResponseMessage, Message};

mod codec;

pub struct Http<T: DeserializeOwned> {
    _marker: std::marker::PhantomData<T>,
}
impl<T: Serialize + DeserializeOwned> Http<T> {
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

pub async fn read(stream: TcpStream) -> Result<()> {
    let http_codec = Http::<Message>::new();
    let mut transport = Framed::new(stream, http_codec);

    while let Some(packet) = transport.next().await {
        match packet {
            Ok(message) => match message {
                HttpMessage::Request(req) => match req.body() {
                    Message::Hello(hello_message) => {
                        let resp_msg = HelloResponseMessage {
                            peer_list: HashMap::new(),
                        };
                        let res = Response::builder()
                            .header("Content-Type", "application/json")
                            .status(StatusCode::OK)
                            .body(Message::HelloResponse(resp_msg))
                            .map_err(Error::from)?;
                        transport.send(res).await?
                    }
                    Message::HelloResponse(hrm) => {
                        error!("Invalid Hello Response");
                        bail!("Expected Hello Message Request")
                    }
                },
                HttpMessage::Response(res) => match res.body() {
                    Message::Hello(hello_message) => {
                        error!("Invalid Hello Response Response");
                        bail!("Expected Hello Response Message")
                    }
                    Message::HelloResponse(hrm) => {
                        info!("Updating");
                    }
                },
            },
            Err(e) => bail!("Failed to process request; error = {}", e),
        }
    }

    Ok(())
}

// pub async fn send(request: Request<String>) -> Response<Message> {}
