//! Windows wildcard sockets must exclude more specific sockets on the same port.

use std::io;
use std::net::{TcpListener, UdpSocket};
use std::os::windows::io::{AsRawSocket, FromRawSocket, OwnedSocket};
use std::sync::OnceLock;
use windows::Win32::Networking::WinSock::{
    AF_INET, IPPROTO_TCP, IPPROTO_UDP, SO_EXCLUSIVEADDRUSE, SOCK_DGRAM, SOCK_STREAM, SOCKADDR_IN,
    SOCKET, SOCKET_ERROR, SOL_SOCKET, WSA_FLAG_NO_HANDLE_INHERIT, WSA_FLAG_OVERLAPPED, WSADATA,
    WSAGetLastError, WSASocketW, WSAStartup, bind, listen, setsockopt,
};

fn last_error() -> io::Error {
    io::Error::from_raw_os_error(unsafe { WSAGetLastError() }.0)
}

fn socket_on(port: u16, kind: i32, protocol: i32) -> io::Result<OwnedSocket> {
    // Keep one Winsock initialization for the process, not one per host or retry.
    static STARTUP: OnceLock<i32> = OnceLock::new();
    let code = *STARTUP.get_or_init(|| {
        let mut data = WSADATA::default();
        unsafe { WSAStartup(0x0202, &mut data) }
    });
    if code != 0 {
        return Err(io::Error::from_raw_os_error(code));
    }
    let socket = unsafe {
        WSASocketW(
            AF_INET.0.into(),
            kind,
            protocol,
            None,
            0,
            WSA_FLAG_OVERLAPPED | WSA_FLAG_NO_HANDLE_INHERIT,
        )
    }
    .map_err(|_| last_error())?;
    // Own the socket immediately so every failed reservation closes it, and do not
    // inherit it in child tools. Exclusivity must be set before binding either transport.
    let owned = unsafe { OwnedSocket::from_raw_socket(socket.0 as _) };
    let exclusive = 1i32.to_ne_bytes();
    if unsafe { setsockopt(socket, SOL_SOCKET, SO_EXCLUSIVEADDRUSE, Some(&exclusive)) }
        == SOCKET_ERROR
    {
        return Err(last_error());
    }
    let address = SOCKADDR_IN {
        sin_family: AF_INET,
        sin_port: port.to_be(),
        ..Default::default()
    };
    if unsafe {
        bind(
            socket,
            (&address as *const SOCKADDR_IN).cast(),
            std::mem::size_of_val(&address) as i32,
        )
    } == SOCKET_ERROR
    {
        return Err(last_error());
    }
    Ok(owned)
}

pub(super) fn udp_socket(port: u16) -> io::Result<UdpSocket> {
    Ok(UdpSocket::from(socket_on(
        port,
        SOCK_DGRAM.0,
        IPPROTO_UDP.0,
    )?))
}

pub(super) fn tcp_listener(port: u16) -> io::Result<TcpListener> {
    let socket = socket_on(port, SOCK_STREAM.0, IPPROTO_TCP.0)?;
    if unsafe { listen(SOCKET(socket.as_raw_socket() as _), 128) } == SOCKET_ERROR {
        return Err(last_error());
    }
    Ok(TcpListener::from(socket))
}
