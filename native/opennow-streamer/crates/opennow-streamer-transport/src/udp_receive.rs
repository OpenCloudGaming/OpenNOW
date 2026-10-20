//! Bounded-wait UDP receives for the NVST and WebRTC receive loops.
//!
//! On Windows a blocking receive that times out under 'SO_RCVTIMEO' cancels its AFD
//! receive and returns without waiting for the cancellation (mswsock 'WSPRecvFrom' calls
//! 'NtCancelIoFile' and returns 'STATUS_IO_TIMEOUT'). A datagram that completes the receive
//! in that window is written, with its I/O status block, into the stack frame of the call
//! that already returned, so whatever runs next at that depth finds a saved register
//! replaced by the datagram length. On Windows the wait therefore happens in 'WSAPoll',
//! whose AFD poll the kernel times out itself, and the receive runs only once a datagram is
//! queued; the socket keeps no receive timeout. Elsewhere the socket receive timeout is the
//! wait.

use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

/// Makes 'socket' ready for 'recv_from' with waits of 'wait'.
pub(crate) fn prepare(socket: &UdpSocket, wait: Duration) -> io::Result<()> {
    #[cfg(windows)]
    {
        let _ = wait;
        socket.set_read_timeout(None)
    }
    #[cfg(not(windows))]
    {
        socket.set_read_timeout(Some(wait))
    }
}

/// Receives one datagram, waiting at most 'wait'; 'WouldBlock' or 'TimedOut' when none
/// arrived. The socket must have been prepared with the same wait.
pub(crate) fn recv_from(
    socket: &UdpSocket,
    buffer: &mut [u8],
    wait: Duration,
) -> io::Result<(usize, SocketAddr)> {
    #[cfg(windows)]
    if !wait_readable(socket, wait)? {
        return Err(io::ErrorKind::WouldBlock.into());
    }
    #[cfg(not(windows))]
    let _ = wait;
    socket.recv_from(buffer)
}

#[cfg(windows)]
fn wait_readable(socket: &UdpSocket, wait: Duration) -> io::Result<bool> {
    use std::os::windows::io::AsRawSocket;
    use windows_sys::Win32::Networking::WinSock::{
        POLLRDNORM, SOCKET_ERROR, WSAGetLastError, WSAPOLLFD, WSAPoll,
    };

    let mut descriptor = WSAPOLLFD {
        fd: socket.as_raw_socket() as _,
        events: POLLRDNORM,
        revents: 0,
    };
    let timeout_ms = i32::try_from(wait.as_micros().div_ceil(1_000)).unwrap_or(i32::MAX);
    // SAFETY: one live descriptor for the duration of this synchronous call.
    let ready = unsafe { WSAPoll(&mut descriptor, 1, timeout_ms) };
    if ready == SOCKET_ERROR {
        // SAFETY: read the calling thread's Winsock error immediately on failure.
        return Err(io::Error::from_raw_os_error(unsafe { WSAGetLastError() }));
    }
    Ok(ready > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn an_idle_wait_times_out_and_a_queued_datagram_is_received() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        let wait = Duration::from_millis(20);
        prepare(&receiver, wait).unwrap();
        let mut buffer = [0_u8; 64];
        let started = Instant::now();
        let error = recv_from(&receiver, &mut buffer, wait).unwrap_err();
        assert!(
            matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ),
            "{error:?}"
        );
        assert!(started.elapsed() >= Duration::from_millis(15));
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        sender
            .send_to(b"datagram", receiver.local_addr().unwrap())
            .unwrap();
        let (length, source) = recv_from(&receiver, &mut buffer, wait).unwrap();
        assert_eq!(&buffer[..length], b"datagram");
        assert_eq!(source, sender.local_addr().unwrap());
    }
}
