use anyhow::{bail, Error, Result};
use http::{HeaderMap, HeaderValue, Request, Response, StatusCode, Version};
use http::request::Builder as RequestBuilder;
use http::response::Builder as ResponseBuilder;
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio_util::bytes::{Buf, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

use crate::http::Http;

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

fn convert_httparse_request(parsed_request: httparse::Request) -> Result<RequestBuilder> {
    let method = parsed_request.method;
    if method.is_none() {
        bail!("HTTP Method is undefined");
    }
    let method = method.unwrap();

    let uri = parsed_request.path;
    if uri.is_none() {
        bail!("HTTP Uri is undefined");
    }
    let uri = uri.unwrap();

    // Kill anything that is not HTTP/1.1
    let version = match parsed_request.version {
        Some(1) => Version::HTTP_11,
        None => Version::HTTP_11,
        _ => bail!("invalid HTTP version {}", parsed_request.version.unwrap()),
    };

    let mut builder = Request::builder().method(method).uri(uri).version(version);

    for header in parsed_request.headers {
        builder = builder.header(
            header.name,
            std::str::from_utf8(header.value).unwrap_or_default(),
        );
    }

    Ok(builder)
}

fn convert_httparse_response(parsed_response: httparse::Response) -> Result<ResponseBuilder> {
    // Kill anything that is not HTTP/1.1
    let version = match parsed_response.version {
        Some(1) => Version::HTTP_11,
        None => Version::HTTP_11,
        _ => bail!("invalid HTTP version {}", parsed_response.version.unwrap()),
    };

    let status_code =
        StatusCode::from_u16(parsed_response.code.unwrap_or(200)).unwrap_or(StatusCode::OK);
    let mut builder = Response::builder()
        .header("Content-Type", "application/json")
        .status(status_code)
        .version(version);

    for header in parsed_response.headers {
        builder = builder.header(
            header.name,
            std::str::from_utf8(header.value).unwrap_or_default(),
        );
    }

    Ok(builder)
}

fn is_request(src: &mut BytesMut) -> bool {
    let message_type = detect_message_type(src);
    message_type.is_some() && message_type.unwrap() == MessageType::Request
}

fn is_response(src: &mut BytesMut) -> bool {
    let message_type = detect_message_type(src);
    message_type.is_some() && message_type.unwrap() == MessageType::Response
}

fn decode_request<T: Serialize + DeserializeOwned>(
    src: &mut BytesMut,
) -> Result<Option<HttpMessage<T>>> {
    // Intentionally allow `req`'s ownership of `src` to be dropped here so we can slice
    // out the request body later.
    let (builder, amt) = {
        let mut headers = [httparse::EMPTY_HEADER; 16];
        let mut req = httparse::Request::new(&mut headers);
        let amt = {
            let status = req.parse(src)?;
            match status {
                httparse::Status::Complete(amt) => amt,
                httparse::Status::Partial => return Ok(None),
            }
        };

        // Let's make sure that the payload is going to be JSON
        if !req.headers.iter().any(|header| {
            header.name.eq_ignore_ascii_case("content-type") && header.value == b"application/json"
        }) {
            bail!("Content-Type header is unset, or is not `application/json`");
        }

        // Make the request builder object, and get the pointer to the body where the data is
        // being stored.
        (convert_httparse_request(req)?, amt)
    };

    if src.len() > amt {
        // Rip off the header data. We _must_ advance the pointer here
        let _headers = src.split_to(amt);

        // Ingest the body data
        let body = serde_json::from_slice::<T>(src)?;
        let req = builder.body(body).map_err(Error::from)?;

        // Move beyond the buffer
        src.advance(src.len());

        // This is likely overkill
        src.clear();

        Ok(Some(HttpMessage::Request(req)))
    } else {
        // Wait for more data
        Ok(None)
    }
}

fn decode_response<T: Serialize + DeserializeOwned>(
    src: &mut BytesMut,
) -> Result<Option<HttpMessage<T>>> {
    // Intentionally allow `req`'s ownership of `src` to be dropped here so we can slice
    // out the request body later.
    let (builder, amt) = {
        let mut headers = [httparse::EMPTY_HEADER; 16];
        let mut res = httparse::Response::new(&mut headers);
        let amt = {
            let status = res.parse(src)?;
            match status {
                httparse::Status::Complete(amt) => amt,
                httparse::Status::Partial => return Ok(None),
            }
        };

        // Let's make sure that the payload is going to be JSON
        if !res.headers.iter().any(|header| {
            header.name.eq_ignore_ascii_case("content-type") && header.value == b"application/json"
        }) {
            bail!("Content-Type header is unset, or is not `application/json`");
        }

        // Make the request builder object, and get the pointer to the body where the data is
        // being stored.
        (convert_httparse_response(res)?, amt)
    };

    if src.len() > amt {
        // Rip off the header data. We _must_ advance the pointer here
        let _headers = src.split_to(amt);

        // Ingest the body data
        let body = serde_json::from_slice::<T>(src)?;
        let res = builder.body(body).map_err(Error::from)?;

        // Move beyond the buffer
        src.advance(src.len());

        // This is likely overkill
        src.clear();

        Ok(Some(HttpMessage::Response(res)))
    } else {
        // Wait for more data
        Ok(None)
    }
}

fn try_decode<T: Serialize + DeserializeOwned>(
    src: &mut BytesMut,
) -> Result<Option<HttpMessage<T>>> {
    if is_request(src) {
        decode_request(src)
    } else if is_response(src) {
        decode_response(src)
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

impl<T: Serialize + DeserializeOwned> Encoder<Response<T>> for Http<T> {
    type Error = Error;

    fn encode(&mut self, item: Response<T>, dst: &mut BytesMut) -> Result<()> {
        use std::fmt::Write;

        let body_string = serde_json::to_string(&item.body())?;

        write!(
            dst,
            "\
            HTTP/1.1 {}\r\n\
            Server: HotStuff\r\n\
            Content-Length: {}\r\n\
            Date: {}\r\n\
            ",
            item.status(),
            body_string.len(),
            date_header_now(),
        )
        .map_err(Error::from)?;

        // Dump the rest of the headers
        encode_headers_and_body(item.headers(), body_string, dst);

        Ok(())
    }
}

impl<T: Serialize + DeserializeOwned> Encoder<Request<T>> for Http<T> {
    type Error = Error;

    fn encode(&mut self, item: Request<T>, dst: &mut BytesMut) -> Result<()> {
        use std::fmt::Write;

        let body_string = serde_json::to_string(&item.body())?;

        write!(
            dst,
            "\
            {} {} HTTP/1.1\r\n\
            Server: HotStuff\r\n\
            Content-Length: {}\r\n\
            Date: {}\r\n\
            ",
            item.method(),
            item.uri(),
            body_string.len(),
            date_header_now(),
        )
        .map_err(Error::from)?;

        // Dump the rest of the headers
        encode_headers_and_body(item.headers(), body_string, dst);

        Ok(())
    }
}
