// SPDX-License-Identifier: GPL-3.0-or-later
//! One NNG request/reply exchange (REQ0 to REP0) over KiCad's local IPC endpoint.
//!
//! This is the wire protocol NNG's IPC transport speaks, implemented directly so the native MCP
//! needs no C library:
//!
//! - Both peers send an 8-byte header `00 'S' 'P' 00 <protocol u16 BE> 00 00` on connect; REQ0 is
//!   0x30 and REP0 is 0x31.
//! - Every message is framed as one type byte (1) and a 64-bit big-endian length.
//! - A REQ0 message starts with a 32-bit request id whose top bit is set; REP0 echoes it in front
//!   of the reply.
//!
//! Like the Python client (`pynng.Req0(resend_time=-1)`), each request uses a fresh connection
//! and is never resent: a lost reply is reported, not retried, because KiCad may still apply it.
//! Connecting is retried until the deadline (NNG redials a missing endpoint the same way).

use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::{timeout_at, Instant};

const REQ0: u16 = 0x30;
const REP0: u16 = 0x31;
/// Largest reply accepted.  KiCad's replies (items, renders) are far smaller.
const MAX_REPLY: u64 = 512 * 1024 * 1024;
/// How long to wait before dialing a missing or busy endpoint again (NNG's reconnect minimum).
const REDIAL: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub enum TransportError {
    /// No reply in time.  The request may or may not have reached KiCad.
    Timeout,
    /// The peer closed the connection before replying.  The request may have reached KiCad.
    Closed,
    /// The peer is not an NNG REP0 endpoint, or sent a malformed frame.
    Protocol(String),
}

fn header(protocol: u16) -> [u8; 8] {
    let [hi, lo] = protocol.to_be_bytes();
    [0, b'S', b'P', 0, hi, lo, 0, 0]
}

fn request_id() -> u32 {
    let bytes = uuid::Uuid::new_v4().into_bytes();
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) | 0x8000_0000
}

fn closed(error: &std::io::Error) -> bool {
    use std::io::ErrorKind::*;
    matches!(
        error.kind(),
        UnexpectedEof | ConnectionReset | ConnectionAborted | BrokenPipe
    )
}

/// Sends `body` to the endpoint at `path` (a socket path, or a pipe name such as
/// `C:\Temp\kicad\api.sock` on Windows) and returns the reply body.  Connecting and sending get
/// `timeout`, and so does waiting for the reply, as with pynng's send and receive timeouts.
pub async fn request(
    path: &str,
    body: &[u8],
    timeout: Duration,
) -> Result<Vec<u8>, TransportError> {
    let send_deadline = Instant::now() + timeout;
    let id = request_id();
    let mut stream = timeout_at(send_deadline, async {
        let mut stream = connect(path, send_deadline).await?;
        stream.write_all(&header(REQ0)).await.map_err(io_error)?;
        let mut peer = [0u8; 8];
        stream.read_exact(&mut peer).await.map_err(io_error)?;
        if peer[..4] != [0, b'S', b'P', 0] || peer[6..] != [0, 0] {
            return Err(TransportError::Protocol(
                "the endpoint is not an NNG peer".into(),
            ));
        }
        if u16::from_be_bytes([peer[4], peer[5]]) != REP0 {
            return Err(TransportError::Protocol(
                "the endpoint is not an NNG reply socket".into(),
            ));
        }
        let mut frame = Vec::with_capacity(13 + body.len());
        frame.push(1);
        frame.extend_from_slice(&(4 + body.len() as u64).to_be_bytes());
        frame.extend_from_slice(&id.to_be_bytes());
        frame.extend_from_slice(body);
        stream.write_all(&frame).await.map_err(io_error)?;
        stream.flush().await.map_err(io_error)?;
        Ok(stream)
    })
    .await
    .map_err(|_| TransportError::Timeout)??;

    let receive_deadline = Instant::now() + timeout;
    timeout_at(receive_deadline, async {
        loop {
            let message = read_frame(&mut stream).await?;
            if message.len() < 4 {
                return Err(TransportError::Protocol(
                    "reply without a request id".into(),
                ));
            }
            // A reply to an earlier request id cannot arrive on a fresh connection; skip it anyway
            if message[..4] == id.to_be_bytes() {
                return Ok(message[4..].to_vec());
            }
        }
    })
    .await
    .map_err(|_| TransportError::Timeout)?
}

fn io_error(error: std::io::Error) -> TransportError {
    if closed(&error) {
        TransportError::Closed
    } else {
        TransportError::Protocol(error.to_string())
    }
}

async fn read_frame<S: AsyncRead + Unpin>(stream: &mut S) -> Result<Vec<u8>, TransportError> {
    let mut head = [0u8; 9];
    stream.read_exact(&mut head).await.map_err(io_error)?;
    if head[0] != 1 {
        return Err(TransportError::Protocol(format!(
            "unexpected IPC message type {}",
            head[0]
        )));
    }
    let length = u64::from_be_bytes(head[1..].try_into().expect("8 bytes"));
    if length > MAX_REPLY {
        return Err(TransportError::Protocol(format!(
            "reply of {length} bytes is too large"
        )));
    }
    let mut message = vec![0u8; length as usize];
    stream.read_exact(&mut message).await.map_err(io_error)?;
    Ok(message)
}

trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

/// Whether connecting again may succeed: the endpoint is missing, refusing or busy.
fn retryable(error: &std::io::Error) -> bool {
    use std::io::ErrorKind::*;
    // ERROR_PIPE_BUSY: every instance of the pipe is in use
    matches!(error.kind(), NotFound | ConnectionRefused) || error.raw_os_error() == Some(231)
}

async fn connect(path: &str, deadline: Instant) -> Result<Box<dyn Stream>, TransportError> {
    loop {
        match open(path).await {
            Ok(stream) => return Ok(stream),
            Err(error) if retryable(&error) && Instant::now() + REDIAL < deadline => {
                tokio::time::sleep(REDIAL).await;
            }
            Err(error) if retryable(&error) => {
                tokio::time::sleep_until(deadline).await;
                return Err(TransportError::Timeout);
            }
            Err(error) => return Err(TransportError::Protocol(error.to_string())),
        }
    }
}

#[cfg(unix)]
async fn open(path: &str) -> std::io::Result<Box<dyn Stream>> {
    Ok(Box::new(tokio::net::UnixStream::connect(path).await?))
}

#[cfg(windows)]
async fn open(path: &str) -> std::io::Result<Box<dyn Stream>> {
    let name = format!(r"\\.\pipe\{path}");
    Ok(Box::new(
        tokio::net::windows::named_pipe::ClientOptions::new().open(name)?,
    ))
}

/// A minimal NNG REP0 endpoint for tests: answers each request with `respond(body)`, or drops it
/// when `respond` returns None (the client then sees no reply).
#[cfg(test)]
pub(crate) mod testing {
    use super::*;

    pub async fn serve_one<S: AsyncRead + AsyncWrite + Unpin>(
        stream: &mut S,
        respond: impl Fn(&[u8]) -> Option<Vec<u8>>,
    ) -> Result<(), TransportError> {
        stream.write_all(&header(REP0)).await.map_err(io_error)?;
        let mut peer = [0u8; 8];
        stream.read_exact(&mut peer).await.map_err(io_error)?;
        assert_eq!(u16::from_be_bytes([peer[4], peer[5]]), REQ0);
        let message = read_frame(stream).await?;
        if let Some(reply) = respond(&message[4..]) {
            let mut frame = vec![1];
            frame.extend_from_slice(&(4 + reply.len() as u64).to_be_bytes());
            frame.extend_from_slice(&message[..4]);
            frame.extend_from_slice(&reply);
            stream.write_all(&frame).await.map_err(io_error)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_and_ids() {
        assert_eq!(header(REQ0), [0, b'S', b'P', 0, 0, 0x30, 0, 0]);
        assert!(request_id() & 0x8000_0000 != 0);
    }

    #[tokio::test]
    async fn a_duplex_exchange_round_trips() {
        let (mut client, mut server) = tokio::io::duplex(4096);
        let peer = tokio::spawn(async move {
            testing::serve_one(&mut server, |body| Some([b"re:", body].concat()))
                .await
                .unwrap();
        });
        client.write_all(&header(REQ0)).await.unwrap();
        let mut head = [0u8; 8];
        client.read_exact(&mut head).await.unwrap();
        assert_eq!(head, header(REP0));
        let mut frame = vec![1];
        frame.extend_from_slice(&7u64.to_be_bytes());
        frame.extend_from_slice(&0x8000_0001u32.to_be_bytes());
        frame.extend_from_slice(b"abc");
        client.write_all(&frame).await.unwrap();
        let reply = read_frame(&mut client).await.unwrap();
        assert_eq!(
            reply,
            [&0x8000_0001u32.to_be_bytes()[..], b"re:abc"].concat()
        );
        peer.await.unwrap();
    }
}
