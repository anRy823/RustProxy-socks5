//! SOCKS5 Protocol Types

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use crate::protocol::constants::*;

/// SOCKS5 Commands
#[derive(Debug, Clone, PartialEq)]
pub enum Socks5Command {
    Connect { addr: TargetAddr, port: u16 },
    Bind { addr: TargetAddr, port: u16 },
    UdpAssociate { addr: TargetAddr, port: u16 },
}

impl Socks5Command {
    /// Get the command code for this command
    pub fn command_code(&self) -> u8 {
        match self {
            Socks5Command::Connect { .. } => SOCKS5_CMD_CONNECT,
            Socks5Command::Bind { .. } => SOCKS5_CMD_BIND,
            Socks5Command::UdpAssociate { .. } => SOCKS5_CMD_UDP_ASSOCIATE,
        }
    }

    /// Get the target address and port
    pub fn target(&self) -> (&TargetAddr, u16) {
        match self {
            Socks5Command::Connect { addr, port } => (addr, *port),
            Socks5Command::Bind { addr, port } => (addr, *port),
            Socks5Command::UdpAssociate { addr, port } => (addr, *port),
        }
    }
}

/// Target address types supported by SOCKS5
#[derive(Debug, Clone, PartialEq)]
pub enum TargetAddr {
    Ipv4(Ipv4Addr),
    Ipv6(Ipv6Addr),
    Domain(String),
}

impl TargetAddr {
    /// Get the address type code for this address
    pub fn address_type(&self) -> u8 {
        match self {
            TargetAddr::Ipv4(_) => SOCKS5_ADDR_IPV4,
            TargetAddr::Ipv6(_) => SOCKS5_ADDR_IPV6,
            TargetAddr::Domain(_) => SOCKS5_ADDR_DOMAIN,
        }
    }

    /// Convert to string representation
    pub fn to_string(&self) -> String {
        match self {
            TargetAddr::Ipv4(ip) => ip.to_string(),
            TargetAddr::Ipv6(ip) => ip.to_string(),
            TargetAddr::Domain(domain) => domain.clone(),
        }
    }

    /// Create from socket address
    pub fn from_socket_addr(addr: &SocketAddr) -> Self {
        match addr {
            SocketAddr::V4(v4) => TargetAddr::Ipv4(*v4.ip()),
            SocketAddr::V6(v6) => TargetAddr::Ipv6(*v6.ip()),
        }
    }
}

/// Authentication methods
#[derive(Debug, Clone, PartialEq)]
pub enum AuthMethod {
    NoAuth,
    UserPass,
    Unsupported,
}

impl AuthMethod {
    /// Convert to method code
    pub fn method_code(&self) -> u8 {
        match self {
            AuthMethod::NoAuth => SOCKS5_AUTH_NONE,
            AuthMethod::UserPass => SOCKS5_AUTH_USERPASS,
            AuthMethod::Unsupported => SOCKS5_AUTH_UNSUPPORTED,
        }
    }

    /// Create from method code
    pub fn from_code(code: u8) -> Self {
        match code {
            SOCKS5_AUTH_NONE => AuthMethod::NoAuth,
            SOCKS5_AUTH_USERPASS => AuthMethod::UserPass,
            _ => AuthMethod::Unsupported,
        }
    }
}

/// SOCKS5 Response
#[derive(Debug, Clone)]
pub struct Socks5Response {
    pub reply_code: u8,
    pub bind_addr: TargetAddr,
    pub bind_port: u16,
}

impl Socks5Response {
    /// Create a success response
    pub fn success(bind_addr: TargetAddr, bind_port: u16) -> Self {
        Self {
            reply_code: SOCKS5_REPLY_SUCCESS,
            bind_addr,
            bind_port,
        }
    }

    /// Create an error response
    pub fn error(reply_code: u8) -> Self {
        Self {
            reply_code,
            bind_addr: TargetAddr::Ipv4(Ipv4Addr::new(0, 0, 0, 0)),
            bind_port: 0,
        }
    }
}

/// SOCKS5 Greeting message from client
#[derive(Debug, Clone)]
pub struct Socks5Greeting {
    pub version: u8,
    pub methods: Vec<u8>,
}

/// SOCKS5 Connection request from client
#[derive(Debug, Clone)]
pub struct Socks5Request {
    pub version: u8,
    pub command: u8,
    pub reserved: u8,
    pub address_type: u8,
    pub target_addr: TargetAddr,
    pub target_port: u16,
}