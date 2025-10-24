//! SOCKS5 Protocol Handler

use super::{AuthMethod, Socks5Command, Socks5Response, Socks5Greeting, Socks5Request, TargetAddr};
use crate::protocol::constants::*;
use crate::Result;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use anyhow::anyhow;

/// SOCKS5 protocol handler for client connections
pub struct Socks5Handler {
    stream: TcpStream,
}

impl Socks5Handler {
    /// Create a new SOCKS5 handler for the given stream
    pub fn new(stream: TcpStream) -> Self {
        Self { stream }
    }

    /// Handle the SOCKS5 handshake
    pub async fn handle_handshake(&mut self) -> Result<AuthMethod> {
        // Read the greeting message
        let greeting = self.read_greeting().await?;
        
        // Validate version
        if greeting.version != SOCKS5_VERSION {
            return Err(anyhow!(
                "Unsupported SOCKS version: {}", greeting.version
            ));
        }

        // Select authentication method
        let selected_method = self.select_auth_method(&greeting.methods);
        
        // Send method selection response
        self.send_auth_method_response(selected_method.clone()).await?;
        
        Ok(selected_method)
    }

    /// Read the client greeting message
    async fn read_greeting(&mut self) -> Result<Socks5Greeting> {
        // Read version and number of methods
        let mut buf = [0u8; 2];
        self.stream.read_exact(&mut buf).await
            .map_err(|e| anyhow!("Failed to read greeting header: {}", e))?;
        
        let version = buf[0];
        let n_methods = buf[1];
        
        if n_methods == 0 {
            return Err(anyhow!("No authentication methods provided"));
        }

        // Read authentication methods
        let mut methods = vec![0u8; n_methods as usize];
        self.stream.read_exact(&mut methods).await
            .map_err(|e| anyhow!("Failed to read auth methods: {}", e))?;
        
        Ok(Socks5Greeting { version, methods })
    }

    /// Select the best authentication method from client's offered methods
    fn select_auth_method(&self, methods: &[u8]) -> AuthMethod {
        // For now, prefer no authentication if available, otherwise username/password
        if methods.contains(&SOCKS5_AUTH_NONE) {
            AuthMethod::NoAuth
        } else if methods.contains(&SOCKS5_AUTH_USERPASS) {
            AuthMethod::UserPass
        } else {
            AuthMethod::Unsupported
        }
    }

    /// Send authentication method selection response
    async fn send_auth_method_response(&mut self, method: AuthMethod) -> Result<()> {
        let response = [SOCKS5_VERSION, method.method_code()];
        self.stream.write_all(&response).await
            .map_err(|e| anyhow!("Failed to send auth method response: {}", e))?;
        Ok(())
    }

    /// Handle the connection request
    pub async fn handle_request(&mut self) -> Result<Socks5Command> {
        let request = self.read_request().await?;
        
        // Validate version
        if request.version != SOCKS5_VERSION {
            return Err(anyhow!("Invalid SOCKS version in request: {}", request.version));
        }

        // Validate reserved field
        if request.reserved != SOCKS5_RESERVED {
            return Err(anyhow!("Invalid reserved field in request: {}", request.reserved));
        }

        // Convert to command enum
        let command = match request.command {
            SOCKS5_CMD_CONNECT => Socks5Command::Connect {
                addr: request.target_addr,
                port: request.target_port,
            },
            SOCKS5_CMD_BIND => Socks5Command::Bind {
                addr: request.target_addr,
                port: request.target_port,
            },
            SOCKS5_CMD_UDP_ASSOCIATE => Socks5Command::UdpAssociate {
                addr: request.target_addr,
                port: request.target_port,
            },
            _ => return Err(anyhow!("Unsupported command: {}", request.command)),
        };

        Ok(command)
    }

    /// Read the connection request from client
    async fn read_request(&mut self) -> Result<Socks5Request> {
        // Read fixed header: VER CMD RSV ATYP
        let mut header = [0u8; 4];
        self.stream.read_exact(&mut header).await
            .map_err(|e| anyhow!("Failed to read request header: {}", e))?;

        let version = header[0];
        let command = header[1];
        let reserved = header[2];
        let address_type = header[3];

        // Read address based on type
        let target_addr = match address_type {
            SOCKS5_ADDR_IPV4 => {
                let mut addr_bytes = [0u8; 4];
                self.stream.read_exact(&mut addr_bytes).await
                    .map_err(|e| anyhow!("Failed to read IPv4 address: {}", e))?;
                TargetAddr::Ipv4(std::net::Ipv4Addr::from(addr_bytes))
            },
            SOCKS5_ADDR_IPV6 => {
                let mut addr_bytes = [0u8; 16];
                self.stream.read_exact(&mut addr_bytes).await
                    .map_err(|e| anyhow!("Failed to read IPv6 address: {}", e))?;
                TargetAddr::Ipv6(std::net::Ipv6Addr::from(addr_bytes))
            },
            SOCKS5_ADDR_DOMAIN => {
                // Read domain length
                let mut len_buf = [0u8; 1];
                self.stream.read_exact(&mut len_buf).await
                    .map_err(|e| anyhow!("Failed to read domain length: {}", e))?;
                let domain_len = len_buf[0] as usize;

                if domain_len == 0 {
                    return Err(anyhow!("Domain name length cannot be zero"));
                }

                // Read domain name
                let mut domain_bytes = vec![0u8; domain_len];
                self.stream.read_exact(&mut domain_bytes).await
                    .map_err(|e| anyhow!("Failed to read domain name: {}", e))?;
                
                let domain = String::from_utf8(domain_bytes)
                    .map_err(|e| anyhow!("Invalid UTF-8 in domain name: {}", e))?;
                
                TargetAddr::Domain(domain)
            },
            _ => return Err(anyhow!("Unsupported address type: {}", address_type)),
        };

        // Read port (2 bytes, big-endian)
        let mut port_bytes = [0u8; 2];
        self.stream.read_exact(&mut port_bytes).await
            .map_err(|e| anyhow!("Failed to read port: {}", e))?;
        let target_port = u16::from_be_bytes(port_bytes);

        Ok(Socks5Request {
            version,
            command,
            reserved,
            address_type,
            target_addr,
            target_port,
        })
    }

    /// Handle username/password authentication (RFC 1929)
    pub async fn handle_userpass_auth(&mut self) -> Result<Vec<u8>> {
        // Read the authentication request
        // Format: +----+------+----------+------+----------+
        //         |VER | ULEN |  UNAME   | PLEN |  PASSWD  |
        //         +----+------+----------+------+----------+
        //         | 1  |  1   | 1 to 255 |  1   | 1 to 255 |
        //         +----+------+----------+------+----------+

        // Read version and username length
        let mut header = [0u8; 2];
        self.stream.read_exact(&mut header).await
            .map_err(|e| anyhow!("Failed to read userpass auth header: {}", e))?;

        let version = header[0];
        let username_len = header[1] as usize;

        if version != 0x01 {
            return Err(anyhow!("Invalid userpass auth version: {}", version));
        }

        if username_len == 0 || username_len > 255 {
            return Err(anyhow!("Invalid username length: {}", username_len));
        }

        // Read username
        let mut username_bytes = vec![0u8; username_len];
        self.stream.read_exact(&mut username_bytes).await
            .map_err(|e| anyhow!("Failed to read username: {}", e))?;

        // Read password length
        let mut plen_buf = [0u8; 1];
        self.stream.read_exact(&mut plen_buf).await
            .map_err(|e| anyhow!("Failed to read password length: {}", e))?;
        let password_len = plen_buf[0] as usize;

        if password_len == 0 || password_len > 255 {
            return Err(anyhow!("Invalid password length: {}", password_len));
        }

        // Read password
        let mut password_bytes = vec![0u8; password_len];
        self.stream.read_exact(&mut password_bytes).await
            .map_err(|e| anyhow!("Failed to read password: {}", e))?;

        // Construct the credentials packet for the auth manager
        let mut credentials = Vec::new();
        credentials.push(version);
        credentials.push(username_len as u8);
        credentials.extend_from_slice(&username_bytes);
        credentials.push(password_len as u8);
        credentials.extend_from_slice(&password_bytes);

        Ok(credentials)
    }

    /// Send username/password authentication response
    pub async fn send_userpass_auth_response(&mut self, success: bool) -> Result<()> {
        // Response format: +----+--------+
        //                  |VER | STATUS |
        //                  +----+--------+
        //                  | 1  |   1    |
        //                  +----+--------+
        // STATUS: 0x00 = success, any other value = failure

        let status = if success { 0x00 } else { 0x01 };
        let response = [0x01, status];
        
        self.stream.write_all(&response).await
            .map_err(|e| anyhow!("Failed to send userpass auth response: {}", e))?;
        
        Ok(())
    }

    /// Send response to client
    pub async fn send_response(&mut self, response: Socks5Response) -> Result<()> {
        let mut response_bytes = Vec::new();
        
        // VER REP RSV ATYP
        response_bytes.push(SOCKS5_VERSION);
        response_bytes.push(response.reply_code);
        response_bytes.push(SOCKS5_RESERVED);
        response_bytes.push(response.bind_addr.address_type());
        
        // Add bind address
        match &response.bind_addr {
            TargetAddr::Ipv4(ip) => {
                response_bytes.extend_from_slice(&ip.octets());
            },
            TargetAddr::Ipv6(ip) => {
                response_bytes.extend_from_slice(&ip.octets());
            },
            TargetAddr::Domain(domain) => {
                if domain.len() > 255 {
                    return Err(anyhow!("Domain name too long: {}", domain.len()));
                }
                response_bytes.push(domain.len() as u8);
                response_bytes.extend_from_slice(domain.as_bytes());
            },
        }
        
        // Add bind port (big-endian)
        response_bytes.extend_from_slice(&response.bind_port.to_be_bytes());
        
        // Send the response
        self.stream.write_all(&response_bytes).await
            .map_err(|e| anyhow!("Failed to send response: {}", e))?;
        
        Ok(())
    }

    /// Send SOCKS5 greeting (for client mode)
    pub async fn send_greeting(&mut self, methods: &[u8]) -> Result<()> {
        let mut greeting = Vec::new();
        greeting.push(SOCKS5_VERSION);
        greeting.push(methods.len() as u8);
        greeting.extend_from_slice(methods);
        
        self.stream.write_all(&greeting).await
            .map_err(|e| anyhow!("Failed to send greeting: {}", e))?;
        
        Ok(())
    }

    /// Receive authentication method selection (for client mode)
    pub async fn receive_auth_method(&mut self) -> Result<u8> {
        let mut response = [0u8; 2];
        self.stream.read_exact(&mut response).await
            .map_err(|e| anyhow!("Failed to read auth method response: {}", e))?;
        
        if response[0] != SOCKS5_VERSION {
            return Err(anyhow!("Invalid SOCKS version in auth response: {}", response[0]));
        }
        
        Ok(response[1])
    }

    /// Authenticate with username/password (for client mode)
    pub async fn authenticate_username_password(&mut self, username: &str, password: &str) -> Result<()> {
        // Send authentication request
        let mut auth_request = Vec::new();
        auth_request.push(0x01); // Version
        auth_request.push(username.len() as u8);
        auth_request.extend_from_slice(username.as_bytes());
        auth_request.push(password.len() as u8);
        auth_request.extend_from_slice(password.as_bytes());
        
        self.stream.write_all(&auth_request).await
            .map_err(|e| anyhow!("Failed to send auth request: {}", e))?;
        
        // Read authentication response
        let mut response = [0u8; 2];
        self.stream.read_exact(&mut response).await
            .map_err(|e| anyhow!("Failed to read auth response: {}", e))?;
        
        if response[0] != 0x01 {
            return Err(anyhow!("Invalid auth response version: {}", response[0]));
        }
        
        if response[1] != 0x00 {
            return Err(anyhow!("Authentication failed"));
        }
        
        Ok(())
    }

    /// Send CONNECT request (for client mode)
    pub async fn send_connect_request(&mut self, target: &TargetAddr, port: u16) -> Result<()> {
        let mut request = Vec::new();
        
        // VER CMD RSV ATYP
        request.push(SOCKS5_VERSION);
        request.push(SOCKS5_CMD_CONNECT);
        request.push(SOCKS5_RESERVED);
        request.push(target.address_type());
        
        // Add target address
        match target {
            TargetAddr::Ipv4(ip) => {
                request.extend_from_slice(&ip.octets());
            },
            TargetAddr::Ipv6(ip) => {
                request.extend_from_slice(&ip.octets());
            },
            TargetAddr::Domain(domain) => {
                if domain.len() > 255 {
                    return Err(anyhow!("Domain name too long: {}", domain.len()));
                }
                request.push(domain.len() as u8);
                request.extend_from_slice(domain.as_bytes());
            },
        }
        
        // Add port (big-endian)
        request.extend_from_slice(&port.to_be_bytes());
        
        // Send the request
        self.stream.write_all(&request).await
            .map_err(|e| anyhow!("Failed to send connect request: {}", e))?;
        
        Ok(())
    }

    /// Receive CONNECT response (for client mode)
    pub async fn receive_connect_response(&mut self) -> Result<Socks5Response> {
        // Read fixed header: VER REP RSV ATYP
        let mut header = [0u8; 4];
        self.stream.read_exact(&mut header).await
            .map_err(|e| anyhow!("Failed to read connect response header: {}", e))?;

        let version = header[0];
        let reply = header[1];
        let _reserved = header[2];
        let address_type = header[3];

        if version != SOCKS5_VERSION {
            return Err(anyhow!("Invalid SOCKS version in response: {}", version));
        }

        // Read bind address based on type
        let bind_addr = match address_type {
            SOCKS5_ADDR_IPV4 => {
                let mut addr_bytes = [0u8; 4];
                self.stream.read_exact(&mut addr_bytes).await
                    .map_err(|e| anyhow!("Failed to read IPv4 bind address: {}", e))?;
                TargetAddr::Ipv4(std::net::Ipv4Addr::from(addr_bytes))
            },
            SOCKS5_ADDR_IPV6 => {
                let mut addr_bytes = [0u8; 16];
                self.stream.read_exact(&mut addr_bytes).await
                    .map_err(|e| anyhow!("Failed to read IPv6 bind address: {}", e))?;
                TargetAddr::Ipv6(std::net::Ipv6Addr::from(addr_bytes))
            },
            SOCKS5_ADDR_DOMAIN => {
                // Read domain length
                let mut len_buf = [0u8; 1];
                self.stream.read_exact(&mut len_buf).await
                    .map_err(|e| anyhow!("Failed to read bind domain length: {}", e))?;
                let domain_len = len_buf[0] as usize;

                if domain_len == 0 {
                    return Err(anyhow!("Bind domain name length cannot be zero"));
                }

                // Read domain name
                let mut domain_bytes = vec![0u8; domain_len];
                self.stream.read_exact(&mut domain_bytes).await
                    .map_err(|e| anyhow!("Failed to read bind domain name: {}", e))?;
                
                let domain = String::from_utf8(domain_bytes)
                    .map_err(|e| anyhow!("Invalid UTF-8 in bind domain name: {}", e))?;
                
                TargetAddr::Domain(domain)
            },
            _ => return Err(anyhow!("Unsupported bind address type: {}", address_type)),
        };

        // Read bind port (2 bytes, big-endian)
        let mut port_bytes = [0u8; 2];
        self.stream.read_exact(&mut port_bytes).await
            .map_err(|e| anyhow!("Failed to read bind port: {}", e))?;
        let bind_port = u16::from_be_bytes(port_bytes);

        Ok(Socks5Response {
            reply_code: reply,
            bind_addr,
            bind_port,
        })
    }

    /// Get the underlying stream (for proxy chaining)
    pub fn into_stream(self) -> TcpStream {
        self.stream
    }
}
