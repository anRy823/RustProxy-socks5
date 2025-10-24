#!/usr/bin/env python3
"""
Simple SOCKS5 test client to verify the proxy server functionality
"""

import socket
import struct
import sys

def test_socks5_handshake(host='127.0.0.1', port=1080):
    """Test SOCKS5 handshake without authentication"""
    try:
        # Connect to SOCKS5 proxy
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(10)
        sock.connect((host, port))
        print(f"Connected to SOCKS5 proxy at {host}:{port}")
        
        # Send greeting (version 5, 1 method, no auth)
        greeting = b'\x05\x01\x00'
        sock.send(greeting)
        print("Sent SOCKS5 greeting")
        
        # Receive method selection
        response = sock.recv(2)
        if len(response) != 2:
            print(f"Invalid response length: {len(response)}")
            return False
            
        version, method = struct.unpack('BB', response)
        print(f"Received method selection: version={version}, method={method}")
        
        if version != 5:
            print(f"Invalid SOCKS version: {version}")
            return False
            
        if method == 0xFF:
            print("No acceptable authentication methods")
            return False
            
        if method == 0x02:
            print("Username/password authentication required")
            return test_socks5_auth(sock)
        elif method == 0x00:
            print("No authentication required")
            return test_socks5_connect(sock)
        else:
            print(f"Unsupported authentication method: {method}")
            return False
            
    except Exception as e:
        print(f"Error during handshake: {e}")
        return False
    finally:
        sock.close()

def test_socks5_auth(sock):
    """Test SOCKS5 username/password authentication"""
    try:
        # Send authentication request
        username = b'testuser'
        password = b'testpass'
        auth_request = b'\x01' + bytes([len(username)]) + username + bytes([len(password)]) + password
        sock.send(auth_request)
        print("Sent authentication request")
        
        # Receive authentication response
        response = sock.recv(2)
        if len(response) != 2:
            print(f"Invalid auth response length: {len(response)}")
            return False
            
        version, status = struct.unpack('BB', response)
        print(f"Received auth response: version={version}, status={status}")
        
        if version != 1:
            print(f"Invalid auth version: {version}")
            return False
            
        if status != 0:
            print(f"Authentication failed: status={status}")
            return False
            
        print("Authentication successful")
        return test_socks5_connect(sock)
        
    except Exception as e:
        print(f"Error during authentication: {e}")
        return False

def test_socks5_connect(sock):
    """Test SOCKS5 CONNECT request"""
    try:
        # Send CONNECT request to httpbin.org:80
        target_host = 'httpbin.org'
        target_port = 80
        
        # Build CONNECT request
        request = b'\x05\x01\x00\x03'  # VER CMD RSV ATYP(domain)
        request += bytes([len(target_host)]) + target_host.encode('ascii')
        request += struct.pack('>H', target_port)
        
        sock.send(request)
        print(f"Sent CONNECT request to {target_host}:{target_port}")
        
        # Receive CONNECT response
        response = sock.recv(4)
        if len(response) != 4:
            print(f"Invalid connect response header length: {len(response)}")
            return False
            
        version, reply, reserved, atyp = struct.unpack('BBBB', response)
        print(f"Received CONNECT response: version={version}, reply={reply}, atyp={atyp}")
        
        if version != 5:
            print(f"Invalid SOCKS version: {version}")
            return False
            
        if reply != 0:
            print(f"CONNECT failed: reply={reply}")
            return False
            
        # Read bind address based on address type
        if atyp == 1:  # IPv4
            addr_data = sock.recv(6)  # 4 bytes IP + 2 bytes port
        elif atyp == 3:  # Domain
            addr_len = struct.unpack('B', sock.recv(1))[0]
            addr_data = sock.recv(addr_len + 2)  # domain + 2 bytes port
        elif atyp == 4:  # IPv6
            addr_data = sock.recv(18)  # 16 bytes IP + 2 bytes port
        else:
            print(f"Unsupported address type: {atyp}")
            return False
            
        print("CONNECT successful!")
        
        # Try to send a simple HTTP request
        http_request = b"GET /ip HTTP/1.1\r\nHost: httpbin.org\r\nConnection: close\r\n\r\n"
        sock.send(http_request)
        print("Sent HTTP request")
        
        # Try to receive response
        response = sock.recv(1024)
        if response:
            print(f"Received HTTP response ({len(response)} bytes):")
            print(response.decode('utf-8', errors='ignore')[:200] + "...")
            return True
        else:
            print("No HTTP response received")
            return False
            
    except Exception as e:
        print(f"Error during CONNECT: {e}")
        return False

if __name__ == "__main__":
    print("Testing SOCKS5 proxy server...")
    success = test_socks5_handshake()
    if success:
        print("✅ SOCKS5 proxy test PASSED")
        sys.exit(0)
    else:
        print("❌ SOCKS5 proxy test FAILED")
        sys.exit(1)