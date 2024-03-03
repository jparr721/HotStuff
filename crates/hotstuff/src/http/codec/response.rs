use anyhow::{bail, Error};
use http::{Response, StatusCode, Version};
use http::response::Builder as ResponseBuilder;
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio_util::bytes::{Buf, BytesMut};
use tokio_util::codec::Encoder;

use crate::http::codec::{
    date_header_now, detect_message_type, encode_headers_and_body, HttpMessage, MessageType,
};
use crate::http::Http;

fn convert_httparse_response(
    parsed_response: httparse::Response,
) -> anyhow::Result<ResponseBuilder> {
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

pub(crate) fn is_response(src: &mut BytesMut) -> bool {
    let message_type = detect_message_type(src);
    message_type.is_some() && message_type.unwrap() == MessageType::Response
}

pub(crate) fn decode_response<T: Serialize + DeserializeOwned>(
    src: &mut BytesMut,
) -> anyhow::Result<Option<HttpMessage<T>>> {
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

impl<T: Serialize + DeserializeOwned> Encoder<Response<T>> for Http<T> {
    type Error = Error;

    fn encode(&mut self, item: Response<T>, dst: &mut BytesMut) -> anyhow::Result<()> {
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
