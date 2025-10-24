//! SOCKS5 Protocol Constants

// SOCKS5 Protocol Version
pub const SOCKS5_VERSION: u8 = 0x05;

// SOCKS5 Commands
pub const SOCKS5_CMD_CONNECT: u8 = 0x01;
pub const SOCKS5_CMD_BIND: u8 = 0x02;
pub const SOCKS5_CMD_UDP_ASSOCIATE: u8 = 0x03;

// Address Types
pub const SOCKS5_ADDR_IPV4: u8 = 0x01;
pub const SOCKS5_ADDR_DOMAIN: u8 = 0x03;
pub const SOCKS5_ADDR_IPV6: u8 = 0x04;

// Authentication Methods
pub const SOCKS5_AUTH_NONE: u8 = 0x00;
pub const SOCKS5_AUTH_USERPASS: u8 = 0x02;
pub const SOCKS5_AUTH_UNSUPPORTED: u8 = 0xFF;

// Response Codes
pub const SOCKS5_REPLY_SUCCESS: u8 = 0x00;
pub const SOCKS5_REPLY_GENERAL_FAILURE: u8 = 0x01;
pub const SOCKS5_REPLY_CONNECTION_NOT_ALLOWED: u8 = 0x02;
pub const SOCKS5_REPLY_NETWORK_UNREACHABLE: u8 = 0x03;
pub const SOCKS5_REPLY_HOST_UNREACHABLE: u8 = 0x04;
pub const SOCKS5_REPLY_CONNECTION_REFUSED: u8 = 0x05;
pub const SOCKS5_REPLY_TTL_EXPIRED: u8 = 0x06;
pub const SOCKS5_REPLY_COMMAND_NOT_SUPPORTED: u8 = 0x07;
pub const SOCKS5_REPLY_ADDRESS_TYPE_NOT_SUPPORTED: u8 = 0x08;

// Reserved field value
pub const SOCKS5_RESERVED: u8 = 0x00;

// Username/Password authentication version
pub const SOCKS5_USERPASS_VERSION: u8 = 0x01;

// Username/Password authentication status codes
pub const SOCKS5_USERPASS_SUCCESS: u8 = 0x00;
pub const SOCKS5_USERPASS_FAILURE: u8 = 0x01;