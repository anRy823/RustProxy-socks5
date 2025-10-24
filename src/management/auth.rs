//! Management API Authentication

use super::types::ApiAuthConfig;
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use base64::{engine::general_purpose, Engine as _};
use std::sync::Arc;
use tracing::{debug, warn};

/// API authentication middleware
pub struct ApiAuth {
    config: ApiAuthConfig,
}

impl ApiAuth {
    pub fn new(config: ApiAuthConfig) -> Self {
        Self { config }
    }
    
    /// Validate API key authentication
    fn validate_api_key(&self, headers: &HeaderMap) -> bool {
        if let Some(expected_key) = &self.config.api_key {
            if let Some(auth_header) = headers.get("x-api-key") {
                if let Ok(provided_key) = auth_header.to_str() {
                    return provided_key == expected_key;
                }
            }
        }
        false
    }
    
    /// Validate basic authentication
    fn validate_basic_auth(&self, headers: &HeaderMap) -> bool {
        if let Some(basic_config) = &self.config.basic_auth {
            if let Some(auth_header) = headers.get("authorization") {
                if let Ok(auth_str) = auth_header.to_str() {
                    if let Some(encoded) = auth_str.strip_prefix("Basic ") {
                        if let Ok(decoded) = general_purpose::STANDARD.decode(encoded) {
                            if let Ok(credentials) = String::from_utf8(decoded) {
                                let parts: Vec<&str> = credentials.splitn(2, ':').collect();
                                if parts.len() == 2 {
                                    return parts[0] == basic_config.username 
                                        && parts[1] == basic_config.password;
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }
    
    /// Authenticate request
    pub fn authenticate(&self, headers: &HeaderMap) -> bool {
        if !self.config.enabled {
            debug!("API authentication disabled, allowing request");
            return true;
        }
        
        // Try API key authentication first
        if self.validate_api_key(headers) {
            debug!("API key authentication successful");
            return true;
        }
        
        // Try basic authentication
        if self.validate_basic_auth(headers) {
            debug!("Basic authentication successful");
            return true;
        }
        
        // TODO: Add JWT authentication support
        
        warn!("API authentication failed");
        false
    }
}

/// Authentication middleware function
pub async fn auth_middleware(
    State(auth): State<Arc<ApiAuth>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let headers = request.headers();
    
    if auth.authenticate(headers) {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::management::types::BasicAuthConfig;
    use axum::http::HeaderValue;
    
    #[test]
    fn test_api_key_auth() {
        let config = ApiAuthConfig {
            enabled: true,
            api_key: Some("test-key".to_string()),
            basic_auth: None,
            jwt: None,
        };
        
        let auth = ApiAuth::new(config);
        let mut headers = HeaderMap::new();
        
        // Test without API key
        assert!(!auth.authenticate(&headers));
        
        // Test with correct API key
        headers.insert("x-api-key", HeaderValue::from_static("test-key"));
        assert!(auth.authenticate(&headers));
        
        // Test with incorrect API key
        headers.insert("x-api-key", HeaderValue::from_static("wrong-key"));
        assert!(!auth.authenticate(&headers));
    }
    
    #[test]
    fn test_basic_auth() {
        let config = ApiAuthConfig {
            enabled: true,
            api_key: None,
            basic_auth: Some(BasicAuthConfig {
                username: "admin".to_string(),
                password: "secret".to_string(),
            }),
            jwt: None,
        };
        
        let auth = ApiAuth::new(config);
        let mut headers = HeaderMap::new();
        
        // Test without auth header
        assert!(!auth.authenticate(&headers));
        
        // Test with correct credentials (admin:secret -> YWRtaW46c2VjcmV0)
        let encoded = general_purpose::STANDARD.encode("admin:secret");
        let auth_value = format!("Basic {}", encoded);
        headers.insert("authorization", HeaderValue::from_str(&auth_value).unwrap());
        assert!(auth.authenticate(&headers));
        
        // Test with incorrect credentials
        let encoded = general_purpose::STANDARD.encode("admin:wrong");
        let auth_value = format!("Basic {}", encoded);
        headers.insert("authorization", HeaderValue::from_str(&auth_value).unwrap());
        assert!(!auth.authenticate(&headers));
    }
    
    #[test]
    fn test_disabled_auth() {
        let config = ApiAuthConfig {
            enabled: false,
            api_key: Some("test-key".to_string()),
            basic_auth: None,
            jwt: None,
        };
        
        let auth = ApiAuth::new(config);
        let headers = HeaderMap::new();
        
        // Should allow all requests when disabled
        assert!(auth.authenticate(&headers));
    }
}