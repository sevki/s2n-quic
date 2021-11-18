// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::Result;
use bytes::Bytes;
use core::str::FromStr;
use s2n_quic::stream::{ReceiveStream, SendStream};
use std::io;

/// From https://github.com/marten-seemann/quic-interop-runner#test-cases
#[derive(Clone, Copy, Debug)]
pub enum Testcase {
    /// Tests that a server sends a Version Negotiation packet in response to an unknown QUIC version number.
    ///
    /// The client should start a connection using an unsupported version number (it can use a reserved version number to do so),
    /// and should abort the connection attempt when receiving the Version Negotiation packet. Currently disabled due to #20.
    VersionNegotiation,

    /// Tests the successful completion of the handshake.
    ///
    /// The client is expected to establish a single QUIC connection to
    /// the server and download one or multiple small files. Servers should not send a Retry packet in this test case.
    Handshake,

    /// Tests both flow control and stream multiplexing.
    ///
    /// The client should use small initial flow control windows for both
    /// stream- and connection-level flow control, such that the during the transfer of files on the order of 1 MB the flow
    /// control window needs to be increased. The client is expected to establish a single QUIC connection, and use multiple
    /// streams to concurrently download the files.
    Transfer,

    /// Tests support for ChaCha20.
    ///
    /// In this test, client and server are expected to offer only ChaCha20 as a ciphersuite. The client then downloads the files.
    ChaCha20,

    /// Tests support for key updates (client only)
    ///
    /// The client is expected to make sure that a key update happens early in the connection (during the first MB transferred).
    /// It doesn't matter which peer actually initiated the update.
    KeyUpdate,

    /// Tests that the server can generate a Retry, and that the client can act upon it.
    ///
    /// The client should use the Token provided in the Retry packet in the Initial packet.
    Retry,

    /// Tests QUIC session resumption (without 0-RTT).
    ///
    /// The client is expected to establish a connection and download the first file. The server is expected to
    /// provide the client with a session ticket that allows it to resume the connection.
    /// After downloading the first file, the client has to close the connection, establish a resumed connection using the
    /// session ticket, and use this connection to download the remaining file(s).
    Resumption,

    /// Tests QUIC 0-RTT.
    ///
    /// The client is expected to establish a connection and download the first file. The server is expected to provide the
    /// client with a session ticket that allows it establish a 0-RTT connection on the next connection attempt. After downloading
    ///  the first file, the client has to close the connection, establish and request the remaining file(s) in 0-RTT.
    ZeroRtt,

    /// Tests a simple HTTP/3 connection.
    ///
    /// The client is expected to download multiple files using HTTP/3. Files should be requested and transferred in parallel.
    Http3,

    /// Tests resilience of the handshake to high loss.
    ///
    ///  The client is expected to establish multiple connections, sequential or in parallel, and use each connection to download a single file.
    Multiconnect,

    /// Tests support for ECN markings
    Ecn,

    /// Tests an active connection migration
    ///
    /// A transfer succeeded during which the client performed an active migration.
    ConnectionMigration,
}

impl Testcase {
    pub const TESTCASES: &'static [Self] = &[
        Self::VersionNegotiation,
        Self::Handshake,
        Self::Transfer,
        Self::ChaCha20,
        Self::KeyUpdate,
        Self::Retry,
        Self::Resumption,
        Self::ZeroRtt,
        Self::Http3,
        Self::Multiconnect,
        Self::Ecn,
        Self::ConnectionMigration,
    ];

    pub const fn as_str(self) -> &'static str {
        use Testcase::*;
        match self {
            VersionNegotiation => "versionnegotiation",
            Handshake => "handshake",
            Transfer => "transfer",
            ChaCha20 => "chacha20",
            KeyUpdate => "keyupdate",
            Retry => "retry",
            Resumption => "resumption",
            ZeroRtt => "zerortt",
            Http3 => "http3",
            Multiconnect => "multiconnect",
            Ecn => "ecn",
            ConnectionMigration => "connectionmigration",
        }
    }

    pub fn supported(f: impl Fn(Self) -> bool) -> Vec<&'static str> {
        let mut results = vec![];

        for testcase in Self::TESTCASES.iter().copied() {
            if f(testcase) {
                results.push(testcase.as_str());
            }
        }

        results
    }
}

impl FromStr for Testcase {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use Testcase::*;

        Ok(match s {
            "versionnegotiation" => VersionNegotiation,
            "handshake" => Handshake,
            "transfer" => Transfer,
            "chacha20" => ChaCha20,
            "keyupdate" => KeyUpdate,
            "retry" => Retry,
            "resumption" => Resumption,
            "zerortt" => ZeroRtt,
            "http3" => Http3,
            "multiconnect" => Multiconnect,
            "ecn" => Ecn,
            "connectionmigration" => ConnectionMigration,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Unsupported test case: {}", s),
                )
                .into())
            }
        })
    }
}

pub async fn write_request(mut stream: SendStream, request: &str) -> Result<()> {
    static GET: Bytes = Bytes::from_static(b"GET ");
    static END_OF_REQUEST: Bytes = Bytes::from_static(b"\r\n");

    stream
        .send_vectored(&mut [
            GET.clone(),
            Bytes::copy_from_slice(request.as_bytes()),
            END_OF_REQUEST.clone(),
        ])
        .await?;

    stream.finish()?;

    Ok(())
}

pub async fn read_request(mut stream: ReceiveStream) -> Result<String> {
    let mut path = String::new();
    let mut chunks = vec![Bytes::new(), Bytes::new()];
    let mut total_chunks = 0;
    loop {
        // grow the chunks
        if chunks.len() == total_chunks {
            chunks.push(Bytes::new());
        }
        let (consumed, is_open) = stream.receive_vectored(&mut chunks[total_chunks..]).await?;
        total_chunks += consumed;
        if parse_h09_request(&chunks[..total_chunks], &mut path, is_open)? {
            return Ok(path);
        }
    }
}

fn parse_h09_request(chunks: &[Bytes], path: &mut String, is_open: bool) -> Result<bool> {
    let mut bytes = chunks.iter().flat_map(|chunk| chunk.iter().cloned());

    macro_rules! expect {
        ($char:literal) => {
            match bytes.next() {
                Some($char) => {}
                None if is_open => return Ok(false),
                _ => return Err("invalid request".into()),
            }
        };
    }

    expect!(b'G');
    expect!(b'E');
    expect!(b'T');
    expect!(b' ');
    expect!(b'/');

    // reset the copied path in case this isn't the first time a path is being parsed
    path.clear();

    loop {
        match bytes.next() {
            Some(c @ b'0'..=b'9') => path.push(c as char),
            Some(c @ b'a'..=b'z') => path.push(c as char),
            Some(c @ b'A'..=b'Z') => path.push(c as char),
            Some(b'.') => path.push('.'),
            Some(b'/') => path.push('/'),
            Some(b'-') => path.push('-'),
            Some(b'\n') | Some(b'\r') => return Ok(true),
            Some(c) => return Err(format!("invalid request {}", c as char).into()),
            None => return Ok(!is_open),
        }
    }
}

#[test]
fn parse_h09_request_test() {
    macro_rules! test {
        ([$($chunk:expr),* $(,)?], $expected:pat) => {{
            let chunks = [$(Bytes::from_static($chunk.as_bytes())),*];
            let mut path = String::new();

            for idx in 0..chunks.len() {
                let _ = parse_h09_request(&chunks[..idx], &mut path, true);
            }

            let result = parse_h09_request(&chunks, &mut path, false);
            let result = result.map(|has_request| if has_request { Some(path) } else { None });
            let result = result.as_ref().map(|v| v.as_deref());

            assert!(matches!(result, $expected), "{:?}", result);
        }}
    }

    test!([], Err(_));
    test!(["GET /"], Ok(Some("")));
    test!(["GET /abc"], Ok(Some("abc")));
    test!(["GET /abc/123"], Ok(Some("abc/123")));
    test!(["GET /CAPS/lower"], Ok(Some("CAPS/lower")));
    test!(["GET /abc\rextra stuff"], Ok(Some("abc")));
    test!(
        ["G", "E", "T", " ", "/", "t", "E", "s", "T"],
        Ok(Some("tEsT"))
    );
}