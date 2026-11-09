//! The protocol between the app and its elevated broker (ADR 0002): frames of JSON,
//! each prefixed by its length and carrying the protocol version, with one operation
//! from a closed list. Never a shell command, never a free path.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::hash::{BuildHasher, RandomState};
use std::io::{self, ErrorKind, Read, Write};

/// Both ends must speak the same version: the broker is the same binary, so a
/// mismatch means something else is on the pipe.
pub const VERSION: u32 = 1;

/// Largest frame accepted, checked before anything is allocated.
const MAX_FRAME: u32 = 64 * 1024;

/// What the app asks the broker. Each new elevated operation is a new variant.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum Request {
    /// Checks the channel and the version right after connecting.
    Ping,
}

/// The broker's answer to one `Request`.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum Reply {
    Pong,
}

#[derive(Serialize, Deserialize)]
struct Frame<T> {
    version: u32,
    body: T,
}

/// Read first, so a frame from another version is refused by its number rather than
/// by a body it cannot parse.
#[derive(Deserialize)]
struct Version {
    version: u32,
}

/// A fresh pipe name that another process cannot guess.
pub fn pipe_name() -> String {
    let a = RandomState::new().hash_one(std::process::id());
    let b = RandomState::new().hash_one(a);
    format!("uninstaller-broker-{a:016x}{b:016x}")
}

pub fn send(to: &mut impl Write, body: &impl Serialize) -> io::Result<()> {
    let bytes = serde_json::to_vec(&Frame {
        version: VERSION,
        body,
    })?;
    let len = u32::try_from(bytes.len())
        .ok()
        .filter(|len| *len <= MAX_FRAME)
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "frame too large"))?;
    to.write_all(&len.to_le_bytes())?;
    to.write_all(&bytes)?;
    to.flush()
}

/// The next frame's body, or `None` when the other end closed between frames.
pub fn receive<T: DeserializeOwned>(from: &mut impl Read) -> io::Result<Option<T>> {
    let mut len = [0; 4];
    match from.read_exact(&mut len) {
        Err(e) if e.kind() == ErrorKind::UnexpectedEof => return Ok(None),
        read => read?,
    }
    let len = u32::from_le_bytes(len);
    if len > MAX_FRAME {
        return Err(invalid("frame too large"));
    }
    let mut bytes = vec![0; len as usize];
    from.read_exact(&mut bytes)?;
    let Version { version } = serde_json::from_slice(&bytes)?;
    if version != VERSION {
        return Err(invalid("protocol version mismatch"));
    }
    let frame: Frame<T> = serde_json::from_slice(&bytes)?;
    Ok(Some(frame.body))
}

/// Answers requests on `channel` until the other end closes it.
pub fn serve(
    channel: &mut (impl Read + Write),
    mut handle: impl FnMut(Request) -> Reply,
) -> io::Result<()> {
    while let Some(request) = receive(channel)? {
        send(channel, &handle(request))?;
    }
    Ok(())
}

fn invalid(why: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, why)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::win::pipe;
    use std::io::Cursor;

    fn frame(json: &str) -> Vec<u8> {
        let mut bytes = (json.len() as u32).to_le_bytes().to_vec();
        bytes.extend_from_slice(json.as_bytes());
        bytes
    }

    #[test]
    fn frames_round_trip_and_end_cleanly() {
        let mut bytes = Vec::new();
        send(&mut bytes, &Request::Ping).expect("send");
        assert_eq!(bytes[4..], *br#"{"version":1,"body":"Ping"}"#);
        let mut from = Cursor::new(bytes);
        assert_eq!(receive(&mut from).expect("ping"), Some(Request::Ping));
        assert_eq!(receive::<Request>(&mut from).expect("end"), None);
    }

    #[test]
    fn refuses_other_versions_unknown_operations_and_huge_frames() {
        let refused = |bytes: Vec<u8>| receive::<Request>(&mut Cursor::new(bytes)).is_err();
        assert!(refused(frame(r#"{"version":2,"body":"Ping"}"#)));
        assert!(refused(frame(r#"{"version":2,"body":{"Later":1}}"#)));
        assert!(refused(frame(r#"{"version":1,"body":"Format"}"#)));
        assert!(refused(frame(r#"{"body":"Ping"}"#)));
        // Only the length is read: nothing that size is allocated.
        assert!(refused(u32::MAX.to_le_bytes().to_vec()));
        // A frame cut short is an error, not a clean end.
        assert!(refused(
            frame(r#"{"version":1,"body":"Ping"}"#)[..10].to_vec()
        ));
    }

    #[test]
    fn pipe_names_differ() {
        assert_ne!(pipe_name(), pipe_name());
        assert!(pipe_name().starts_with("uninstaller-broker-"));
    }

    /// Both ends over a real pipe, without elevation: the broker side runs on a thread.
    #[test]
    fn serves_over_a_named_pipe_until_closed() {
        let name = pipe_name();
        let server = pipe::Server::create(&name).expect("create");
        let broker = std::thread::spawn(move || {
            let mut channel = server.accept().expect("accept");
            serve(&mut channel, |Request::Ping| Reply::Pong)
        });
        let mut app = pipe::connect(&name).expect("connect");
        for _ in 0..2 {
            send(&mut app, &Request::Ping).expect("send");
            assert_eq!(receive(&mut app).expect("reply"), Some(Reply::Pong));
        }
        drop(app);
        broker.join().expect("join").expect("serve ends cleanly");
    }
}
