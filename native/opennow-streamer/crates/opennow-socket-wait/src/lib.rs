//! Socket reads and writes bounded by a wait, without Windows socket timeouts.
//!
//! On Windows a blocking receive or send that times out under `SO_RCVTIMEO` or
//! `SO_SNDTIMEO` cancels its AFD request and returns without waiting for the cancellation
//! (mswsock `WSPRecvFrom` calls `NtCancelIoFile` and returns `STATUS_IO_TIMEOUT`). Data that
//! completes the request in that window is written, with its I/O status block, into the
//! stack frame of the call that already returned, so whatever runs next at that depth finds
//! a saved register replaced by a byte count. On Windows these helpers therefore wait in
//! `WSAPoll`, whose AFD poll the kernel times out itself, and run the socket call only once
//! it can complete; sockets keep no timeout. Elsewhere the socket timeouts are the wait.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::time::Duration;

/// Makes `socket` ready for [recv_from] with waits of `wait`.
pub fn prepare(socket: &UdpSocket, wait: Duration) -> io::Result<()> {
    socket.set_read_timeout((!cfg!(windows)).then_some(wait))
}

/// Makes `socket` ready for [send_to] with waits of `wait`.
pub fn prepare_send(socket: &UdpSocket, wait: Duration) -> io::Result<()> {
    socket.set_write_timeout((!cfg!(windows)).then_some(wait))
}

/// Receives one datagram, waiting at most `wait`; `WouldBlock` or `TimedOut` when none
/// arrived. The socket must have been prepared with the same wait.
pub fn recv_from(
    socket: &UdpSocket,
    buffer: &mut [u8],
    wait: Duration,
) -> io::Result<(usize, SocketAddr)> {
    #[cfg(windows)]
    if !windows::wait_ready(socket, windows::READABLE, Some(wait))? {
        return Err(io::ErrorKind::WouldBlock.into());
    }
    #[cfg(not(windows))]
    let _ = wait;
    socket.recv_from(buffer)
}

/// Sends one datagram, waiting at most `wait` for send buffer space; `WouldBlock` or
/// `TimedOut` when none freed up. The socket must have been prepared with the same wait.
pub fn send_to(
    socket: &UdpSocket,
    buffer: &[u8],
    destination: SocketAddr,
    wait: Duration,
) -> io::Result<usize> {
    #[cfg(windows)]
    if !windows::wait_ready(socket, windows::WRITABLE, Some(wait))? {
        return Err(io::ErrorKind::WouldBlock.into());
    }
    #[cfg(not(windows))]
    let _ = wait;
    socket.send_to(buffer, destination)
}

/// A TCP stream whose reads and writes give up after their timeout with `WouldBlock` or
/// `TimedOut`, like [TcpStream::set_read_timeout] and [TcpStream::set_write_timeout].
pub struct TimedTcpStream {
    stream: TcpStream,
    #[cfg(windows)]
    read_timeout: Option<Duration>,
    #[cfg(windows)]
    write_timeout: Option<Duration>,
}

impl TimedTcpStream {
    /// Takes over `stream` with no timeouts; on Windows it becomes non-blocking.
    pub fn new(stream: TcpStream) -> io::Result<Self> {
        stream.set_nonblocking(cfg!(windows))?;
        Ok(Self {
            stream,
            #[cfg(windows)]
            read_timeout: None,
            #[cfg(windows)]
            write_timeout: None,
        })
    }

    pub fn get_ref(&self) -> &TcpStream {
        &self.stream
    }

    pub fn set_read_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        #[cfg(windows)]
        {
            self.read_timeout = windows::checked(timeout)?;
            Ok(())
        }
        #[cfg(not(windows))]
        self.stream.set_read_timeout(timeout)
    }

    pub fn set_write_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        #[cfg(windows)]
        {
            self.write_timeout = windows::checked(timeout)?;
            Ok(())
        }
        #[cfg(not(windows))]
        self.stream.set_write_timeout(timeout)
    }
}

impl Read for TimedTcpStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        #[cfg(windows)]
        {
            let mut stream = &self.stream;
            windows::retry(&self.stream, windows::READABLE, self.read_timeout, || {
                stream.read(buffer)
            })
        }
        #[cfg(not(windows))]
        self.stream.read(buffer)
    }
}

impl Write for TimedTcpStream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        #[cfg(windows)]
        {
            let mut stream = &self.stream;
            windows::retry(&self.stream, windows::WRITABLE, self.write_timeout, || {
                stream.write(buffer)
            })
        }
        #[cfg(not(windows))]
        self.stream.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

#[cfg(windows)]
mod windows {
    use std::io;
    use std::os::windows::io::AsRawSocket;
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Networking::WinSock::{
        POLLRDNORM, POLLWRNORM, SOCKET_ERROR, WSAGetLastError, WSAPOLLFD, WSAPoll,
    };

    pub(super) const READABLE: i16 = POLLRDNORM;
    pub(super) const WRITABLE: i16 = POLLWRNORM;

    pub(super) fn checked(timeout: Option<Duration>) -> io::Result<Option<Duration>> {
        if timeout == Some(Duration::ZERO) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot set a 0 duration timeout",
            ));
        }
        Ok(timeout)
    }

    pub(super) fn retry<T>(
        socket: &impl AsRawSocket,
        events: i16,
        timeout: Option<Duration>,
        mut attempt: impl FnMut() -> io::Result<T>,
    ) -> io::Result<T> {
        let deadline = timeout.map(|timeout| Instant::now() + timeout);
        loop {
            match attempt() {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                result => return result,
            }
            let wait = deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()));
            if wait == Some(Duration::ZERO) || !wait_ready(socket, events, wait)? {
                return Err(io::ErrorKind::TimedOut.into());
            }
        }
    }

    pub(super) fn wait_ready(
        socket: &impl AsRawSocket,
        events: i16,
        wait: Option<Duration>,
    ) -> io::Result<bool> {
        let mut descriptor = WSAPOLLFD {
            fd: socket.as_raw_socket() as _,
            events,
            revents: 0,
        };
        let timeout_ms = wait.map_or(-1, |wait| {
            i32::try_from(wait.as_micros().div_ceil(1_000)).unwrap_or(i32::MAX)
        });
        // SAFETY: one live descriptor for the duration of this synchronous call.
        let ready = unsafe { WSAPoll(&mut descriptor, 1, timeout_ms) };
        if ready == SOCKET_ERROR {
            // SAFETY: read the calling thread`s Winsock error immediately on failure.
            return Err(io::Error::from_raw_os_error(unsafe { WSAGetLastError() }));
        }
        Ok(ready > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::time::Instant;

    fn timed_out(error: &io::Error) -> bool {
        matches!(
            error.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
        )
    }

    #[test]
    fn an_idle_udp_wait_times_out_and_a_queued_datagram_is_received() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        let wait = Duration::from_millis(20);
        prepare(&receiver, wait).unwrap();
        let mut buffer = [0_u8; 64];
        let started = Instant::now();
        let error = recv_from(&receiver, &mut buffer, wait).unwrap_err();
        assert!(timed_out(&error), "{error:?}");
        assert!(started.elapsed() >= Duration::from_millis(15));
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        prepare_send(&sender, wait).unwrap();
        send_to(&sender, b"datagram", receiver.local_addr().unwrap(), wait).unwrap();
        let (length, source) = recv_from(&receiver, &mut buffer, wait).unwrap();
        assert_eq!(&buffer[..length], b"datagram");
        assert_eq!(source, sender.local_addr().unwrap());
    }

    fn tcp_pair() -> (TimedTcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        (TimedTcpStream::new(client).unwrap(), server)
    }

    #[test]
    fn an_idle_tcp_read_times_out_and_then_reads_what_arrives() {
        let (mut client, mut server) = tcp_pair();
        client
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        let mut buffer = [0_u8; 64];
        let started = Instant::now();
        let error = client.read(&mut buffer).unwrap_err();
        assert!(timed_out(&error), "{error:?}");
        assert!(started.elapsed() >= Duration::from_millis(15));
        server.write_all(b"reply").unwrap();
        let length = client.read(&mut buffer).unwrap();
        assert_eq!(&buffer[..length], b"reply");
        drop(server);
        assert_eq!(client.read(&mut buffer).unwrap(), 0);
    }

    #[test]
    fn a_tcp_write_to_a_peer_that_stops_reading_times_out() {
        let (mut client, _server) = tcp_pair();
        client
            .set_write_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        let chunk = vec![0_u8; 64 * 1024];
        let started = Instant::now();
        let error = loop {
            match client.write(&chunk) {
                Ok(written) => assert!(written > 0),
                Err(error) => break error,
            }
            assert!(started.elapsed() < Duration::from_secs(10));
        };
        assert!(timed_out(&error), "{error:?}");
    }
}
