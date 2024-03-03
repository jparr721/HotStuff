use anyhow::{bail, Error, Result};
use http::{HeaderMap, HeaderValue, Request, Response};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio_util::bytes::BytesMut;
use tokio_util::codec::Decoder;

use crate::http::Http;

pub(crate) mod request;
pub(crate) mod response;

#[derive(Debug, Clone, PartialEq)]
enum MessageType {
    Request,
    Response,
}

fn detect_message_type(bytes: &BytesMut) -> Option<MessageType> {
    const GET: &str = "GET ";
    const POST: &str = "POST";
    const HTTP: &str = "HTTP";

    // This is hot garbage
    if bytes.len() > 3 {
        let start = String::from_utf8_lossy(&bytes[0..4]).to_string();

        match start.as_str() {
            GET => Some(MessageType::Request),
            POST => Some(MessageType::Request),
            HTTP => Some(MessageType::Response),
            _ => None,
        }
    } else {
        None
    }
}

fn try_decode<T: Serialize + DeserializeOwned>(
    src: &mut BytesMut,
) -> Result<Option<HttpMessage<T>>> {
    if request::is_request(src) {
        request::decode_request(src)
    } else if response::is_response(src) {
        response::decode_response(src)
    } else {
        bail!("Invalid input message found");
    }
}

#[derive(Debug, Clone)]
pub enum HttpMessage<T: Serialize + DeserializeOwned> {
    Request(Request<T>),
    Response(Response<T>),
}

impl<T: Serialize + DeserializeOwned> HttpMessage<T> {
    /// Consumes the message and returns the value as a request, if it is one.
    #[inline]
    pub fn into_request(self) -> Result<Request<T>> {
        match self {
            HttpMessage::Request(req) => Ok(req),
            HttpMessage::Response(_) => bail!("Not a request"),
        }
    }

    /// Consumes the message and returns the value as a response, if it is one.
    #[inline]
    pub fn into_response(self) -> Result<Response<T>> {
        match self {
            HttpMessage::Request(_) => bail!("Not a response"),
            HttpMessage::Response(res) => Ok(res),
        }
    }
}

impl<T: Serialize + DeserializeOwned> Decoder for Http<T> {
    type Item = HttpMessage<T>;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // If we cannot even encode a GET or POST or HTTP, wait for more data.
        if src.len() < 4 {
            Ok(None)
        } else {
            try_decode(src)
        }
    }
}

pub fn date_header_now() -> String {
    let date = chrono::Utc::now();
    date.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

fn encode_headers_and_body(
    headers: &HeaderMap<HeaderValue>,
    body_string: String,
    dst: &mut BytesMut,
) {
    for (k, v) in headers {
        dst.extend_from_slice(k.as_str().as_bytes());
        dst.extend_from_slice(b": ");
        dst.extend_from_slice(v.as_bytes());
        dst.extend_from_slice(b"\r\n");
    }

    // Dump the body payload
    dst.extend_from_slice(b"\r\n");
    dst.extend_from_slice(body_string.as_bytes());
}
