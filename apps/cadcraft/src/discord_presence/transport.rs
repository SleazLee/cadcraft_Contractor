//! Only documented local Discord endpoints. Nonblocking opens, reads and writes.
use std::io::{self, Read, Write};

#[cfg(windows)]
type NativeStream = interprocess::os::windows::named_pipe::DuplexPipeStream<interprocess::os::windows::named_pipe::pipe_mode::Bytes>;
#[cfg(unix)]
type NativeStream = std::os::unix::net::UnixStream;
#[cfg(not(any(windows, unix)))]
type NativeStream = io::Cursor<Vec<u8>>;

pub(super) struct Stream(NativeStream);

impl Read for Stream {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let read = self.0.read(bytes)?;
        // Windows byte pipes in PIPE_NOWAIT mode can succeed with zero bytes while idle.
        // Unix streams use zero for EOF; normalize only Windows. This library also maps
        // closed pipes to zero, so heartbeat writes/deadlines detect Windows disconnects.
        // Keep Read's empty-buffer contract.
        #[cfg(windows)]
        if read == 0 && !bytes.is_empty() {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Ok(read)
    }
}
impl Write for Stream {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(windows)]
impl Drop for Stream {
    fn drop(&mut self) {
        // Don't spawn the library's background flush threads for unresponsive peers. Normal
        // RPC acknowledgements already establish receipt; disconnect removes the activity.
        self.0.assume_flushed();
    }
}

pub(super) fn connect() -> io::Result<Stream> {
    #[cfg(windows)]
    for index in 0..10 {
        let path = format!(r"\\?\pipe\discord-ipc-{index}");
        if let Ok(stream) =
            NativeStream::connect_by_path_with_wait_mode(path.as_str(), interprocess::ConnectWaitMode::Timeout(std::time::Duration::ZERO))
        {
            stream.set_nonblocking(true)?;
            return Ok(Stream(stream));
        }
    }
    #[cfg(unix)]
    {
        use std::path::PathBuf;
        let mut roots: Vec<PathBuf> = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
            .iter()
            .filter_map(std::env::var_os)
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .collect();
        roots.push(PathBuf::from("/tmp"));
        roots.dedup();
        for root in roots {
            for index in 0..10 {
                if let Ok(stream) = connect_unix(&root.join(format!("discord-ipc-{index}"))) {
                    return Ok(stream);
                }
            }
        }
    }
    Err(io::ErrorKind::NotFound.into())
}

#[cfg(unix)]
fn connect_unix(path: &std::path::Path) -> io::Result<Stream> {
    let socket = socket2::Socket::new(socket2::Domain::UNIX, socket2::Type::STREAM, None)?;
    socket.set_nonblocking(true)?;
    socket.connect(&socket2::SockAddr::unix(path)?)?;
    // socket2 gates its direct UnixStream conversion behind the optional `all` feature.
    // Transfer ownership through the standard descriptor type instead, including on FreeBSD,
    // preserving nonblocking mode without raw FDs or platform-dependent feature unification.
    let descriptor: std::os::fd::OwnedFd = socket.into();
    Ok(Stream(descriptor.into()))
}

#[cfg(all(test, unix))]
mod unix_tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use std::time::{Duration, Instant};

    #[test]
    fn unix_socket_conversion_preserves_nonblocking_io_and_eof() {
        // Own a unique temporary directory; never probe the user's Discord endpoints.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ipc");
        let listener = UnixListener::bind(&path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut client = connect_unix(&path).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        server.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut bytes = [0u8; 8];
        assert_eq!(client.read(&mut bytes).unwrap_err().kind(), io::ErrorKind::WouldBlock);
        server.write_all(b"ready").unwrap();
        server.shutdown(std::net::Shutdown::Write).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut received = Vec::new();
        loop {
            match client.read(&mut bytes) {
                Ok(0) => break,
                Ok(count) => received.extend_from_slice(&bytes[..count]),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(1));
                }
                result => panic!("unexpected Unix socket read: {result:?}"),
            }
            assert!(received.len() <= 5 && Instant::now() < deadline);
        }
        assert_eq!(received, b"ready");
        client.write_all(b"ping").unwrap();
        server.read_exact(&mut bytes[..4]).unwrap();
        assert_eq!(&bytes[..4], b"ping");
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use interprocess::{
        ConnectWaitMode,
        os::windows::named_pipe::{PipeListenerOptions, pipe_mode::Bytes},
    };
    use std::time::Duration;

    #[test]
    fn idle_windows_byte_pipe_is_not_eof_and_closed_peer_fails_heartbeat_write() {
        let path = format!(r"\\.\pipe\cadcraft-discord-idle-regression-{}", std::process::id());
        let listener = PipeListenerOptions::new().path(path.as_str()).create_duplex::<Bytes>().unwrap();
        listener.set_nonblocking(true).unwrap();
        let native = NativeStream::connect_by_path_with_wait_mode(path.as_str(), ConnectWaitMode::Timeout(Duration::ZERO)).unwrap();
        native.set_nonblocking(true).unwrap();
        let mut client = Stream(native);
        let mut server = listener.accept().unwrap();
        let mut bytes = [0u8; 8];
        assert_eq!(client.read(&mut bytes).unwrap_err().kind(), io::ErrorKind::WouldBlock);
        server.write_all(b"ready").unwrap();
        assert_eq!(client.read(&mut bytes).unwrap(), 5);
        assert_eq!(&bytes[..5], b"ready");
        server.assume_flushed();
        drop(server);
        drop(listener);
        assert_eq!(client.write(b"ping").unwrap_err().kind(), io::ErrorKind::BrokenPipe);
    }
}
