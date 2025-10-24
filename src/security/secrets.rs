//! Secure Configuration and Secrets Management
//! 
//! Provides encrypted configuration storage and environment variable support
//! for sensitive data like passwords and API keys.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn, info};
use crate::Result;

/// Secure configuration manager
pub struct SecretsManager {
    config: SecureConfigSettings,
    secrets_cache: HashMap<String, String>,
}

/// Secure configuration settings
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SecureConfigSettings {
    pub encrypt_config: bool,
    pub use_env_secrets: bool,
    pub secret_key_env: String,
    pub config_encryption_key_env: String,
    pub env_prefix: String,
}

impl Default for SecureConfigSettings {
    fn default() -> Self {
        Self {
            encrypt_config: false,
            use_env_secrets: true,
            secret_key_env: "SOCKS5_SECRET_KEY".to_string(),
            config_encryption_key_env: "SOCKS5_CONFIG_KEY".to_string(),
            env_prefix: "SOCKS5_".to_string(),
        }
    }
}

/// Secure configuration wrapper
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SecureConfig {
    pub auth_users: Vec<SecureUserConfig>,
    pub proxy_credentials: Vec<SecureProxyCredentials>,
    pub tls_certificates: Vec<SecureTlsConfig>,
    pub api_keys: HashMap<String, String>,
}

/// Secure user configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SecureUserConfig {
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_hash: Option<String>,
    pub enabled: bool,
    pub roles: Vec<String>,
}

/// Secure proxy credentials
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SecureProxyCredentials {
    pub name: String,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_env: Option<String>,
}

/// Secure TLS configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SecureTlsConfig {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cert_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cert_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_env: Option<String>,
}

impl SecretsManager {
    /// Create a new secrets manager
    pub fn new(config: SecureConfigSettings) -> Self {
        Self {
            config,
            secrets_cache: HashMap::new(),
        }
    }

    /// Load secure configuration from file or environment
    pub fn load_secure_config(&mut self, config_path: Option<&Path>) -> Result<SecureConfig> {
        let mut secure_config = SecureConfig {
            auth_users: Vec::new(),
            proxy_credentials: Vec::new(),
            tls_certificates: Vec::new(),
            api_keys: HashMap::new(),
        };

        // Load from file if provided
        if let Some(path) = config_path {
            if path.exists() {
                secure_config = self.load_from_file(path)?;
                info!("Loaded secure configuration from file: {}", path.display());
            } else {
                debug!("Secure config file not found: {}", path.display());
            }
        }

        // Override with environment variables if enabled
        if self.config.use_env_secrets {
            self.load_from_environment(&mut secure_config)?;
        }

        // Resolve all secret references
        self.resolve_secrets(&mut secure_config)?;

        Ok(secure_config)
    }

    /// Load configuration from encrypted file
    fn load_from_file(&self, path: &Path) -> Result<SecureConfig> {
        let content = fs::read_to_string(path)?;
        
        if self.config.encrypt_config {
            // Decrypt the content first
            let decrypted_content = self.decrypt_content(&content)?;
            let config: SecureConfig = toml::from_str(&decrypted_content)?;
            Ok(config)
        } else {
            // Load as plain text
            let config: SecureConfig = toml::from_str(&content)?;
            Ok(config)
        }
    }

    /// Load secrets from environment variables
    fn load_from_environment(&mut self, config: &mut SecureConfig) -> Result<()> {
        let prefix = &self.config.env_prefix;
        
        // Load user credentials from environment
        let mut user_index = 0;
        loop {
            let username_key = format!("{}USER_{}_USERNAME", prefix, user_index);
            let password_key = format!("{}USER_{}_PASSWORD", prefix, user_index);
            let enabled_key = format!("{}USER_{}_ENABLED", prefix, user_index);
            
            if let Ok(username) = env::var(&username_key) {
                let password = env::var(&password_key).ok();
                let enabled = env::var(&enabled_key)
                    .unwrap_or_else(|_| "true".to_string())
                    .parse()
                    .unwrap_or(true);
                
                let user_config = SecureUserConfig {
                    username,
                    password,
                    password_env: None,
                    password_hash: None,
                    enabled,
                    roles: vec!["user".to_string()],
                };
                
                config.auth_users.push(user_config);
                user_index += 1;
                
                debug!("Loaded user {} from environment", user_index);
            } else {
                break;
            }
        }

        // Load proxy credentials from environment
        let mut proxy_index = 0;
        loop {
            let name_key = format!("{}PROXY_{}_NAME", prefix, proxy_index);
            let username_key = format!("{}PROXY_{}_USERNAME", prefix, proxy_index);
            let password_key = format!("{}PROXY_{}_PASSWORD", prefix, proxy_index);
            
            if let Ok(name) = env::var(&name_key) {
                let username = env::var(&username_key).unwrap_or_default();
                let password = env::var(&password_key).ok();
                
                let proxy_config = SecureProxyCredentials {
                    name,
                    username,
                    password,
                    password_env: None,
                };
                
                config.proxy_credentials.push(proxy_config);
                proxy_index += 1;
                
                debug!("Loaded proxy credentials {} from environment", proxy_index);
            } else {
                break;
            }
        }

        // Load API keys from environment
        for (key, value) in env::vars() {
            if key.starts_with(&format!("{}API_KEY_", prefix)) {
                let api_name = key.strip_prefix(&format!("{}API_KEY_", prefix))
                    .unwrap_or("unknown")
                    .to_lowercase();
                debug!("Loaded API key '{}' from environment", api_name);
                config.api_keys.insert(api_name, value);
            }
        }

        Ok(())
    }

    /// Resolve all secret references in the configuration
    fn resolve_secrets(&mut self, config: &mut SecureConfig) -> Result<()> {
        // Resolve user passwords
        for user in &mut config.auth_users {
            if user.password.is_none() && user.password_env.is_some() {
                let env_var = user.password_env.as_ref().unwrap();
                if let Ok(password) = env::var(env_var) {
                    user.password = Some(password);
                    debug!("Resolved password for user '{}' from environment", user.username);
                } else {
                    warn!("Environment variable '{}' not found for user '{}'", env_var, user.username);
                }
            }
        }

        // Resolve proxy credentials
        for proxy in &mut config.proxy_credentials {
            if proxy.password.is_none() && proxy.password_env.is_some() {
                let env_var = proxy.password_env.as_ref().unwrap();
                if let Ok(password) = env::var(env_var) {
                    proxy.password = Some(password);
                    debug!("Resolved password for proxy '{}' from environment", proxy.name);
                } else {
                    warn!("Environment variable '{}' not found for proxy '{}'", env_var, proxy.name);
                }
            }
        }

        // Resolve TLS certificates
        for tls in &mut config.tls_certificates {
            if tls.cert_env.is_some() {
                let env_var = tls.cert_env.as_ref().unwrap();
                if let Ok(cert_content) = env::var(env_var) {
                    // Store certificate content in cache
                    let cache_key = format!("tls_cert_{}", tls.name);
                    self.secrets_cache.insert(cache_key, cert_content);
                    debug!("Resolved TLS certificate for '{}' from environment", tls.name);
                }
            }
            
            if tls.key_env.is_some() {
                let env_var = tls.key_env.as_ref().unwrap();
                if let Ok(key_content) = env::var(env_var) {
                    // Store key content in cache
                    let cache_key = format!("tls_key_{}", tls.name);
                    self.secrets_cache.insert(cache_key, key_content);
                    debug!("Resolved TLS key for '{}' from environment", tls.name);
                }
            }
        }

        Ok(())
    }

    /// Encrypt configuration content
    fn encrypt_content(&self, content: &str) -> Result<String> {
        // Get encryption key from environment
        let key = env::var(&self.config.config_encryption_key_env)
            .map_err(|_| anyhow::anyhow!("Encryption key not found in environment variable: {}", 
                                       self.config.config_encryption_key_env))?;

        // Simple XOR encryption (in production, use proper encryption like AES)
        let encrypted = self.simple_encrypt(content, &key);
        use base64::{Engine as _, engine::general_purpose};
        let encoded = general_purpose::STANDARD.encode(encrypted);
        
        Ok(encoded)
    }

    /// Decrypt configuration content
    fn decrypt_content(&self, encrypted_content: &str) -> Result<String> {
        // Get encryption key from environment
        let key = env::var(&self.config.config_encryption_key_env)
            .map_err(|_| anyhow::anyhow!("Encryption key not found in environment variable: {}", 
                                       self.config.config_encryption_key_env))?;

        // Decode and decrypt
        use base64::{Engine as _, engine::general_purpose};
        let encrypted_bytes = general_purpose::STANDARD.decode(encrypted_content)
            .map_err(|e| anyhow::anyhow!("Failed to decode encrypted content: {}", e))?;
        
        let decrypted = self.simple_decrypt(&encrypted_bytes, &key);
        
        Ok(decrypted)
    }

    /// Simple XOR encryption (replace with proper encryption in production)
    fn simple_encrypt(&self, data: &str, key: &str) -> Vec<u8> {
        let key_bytes = key.as_bytes();
        data.bytes()
            .enumerate()
            .map(|(i, byte)| byte ^ key_bytes[i % key_bytes.len()])
            .collect()
    }

    /// Simple XOR decryption (replace with proper decryption in production)
    fn simple_decrypt(&self, data: &[u8], key: &str) -> String {
        let key_bytes = key.as_bytes();
        let decrypted_bytes: Vec<u8> = data
            .iter()
            .enumerate()
            .map(|(i, &byte)| byte ^ key_bytes[i % key_bytes.len()])
            .collect();
        
        String::from_utf8_lossy(&decrypted_bytes).to_string()
    }

    /// Save secure configuration to encrypted file
    pub fn save_secure_config(&self, config: &SecureConfig, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(config)?;
        
        let final_content = if self.config.encrypt_config {
            self.encrypt_content(&content)?
        } else {
            content
        };
        
        fs::write(path, final_content)?;
        info!("Saved secure configuration to: {}", path.display());
        
        Ok(())
    }

    /// Get a secret from cache
    pub fn get_secret(&self, key: &str) -> Option<&String> {
        self.secrets_cache.get(key)
    }

    /// Store a secret in cache
    pub fn store_secret(&mut self, key: String, value: String) {
        self.secrets_cache.insert(key, value);
    }

    /// Clear all cached secrets
    pub fn clear_secrets_cache(&mut self) {
        self.secrets_cache.clear();
        debug!("Cleared secrets cache");
    }

    /// Validate user credentials securely
    pub fn validate_user_credentials(&self, config: &SecureConfig, username: &str, password: &str) -> bool {
        for user in &config.auth_users {
            if user.username == username && user.enabled {
                if let Some(user_password) = &user.password {
                    // Simple password comparison (use proper hashing in production)
                    if self.secure_compare(password, user_password) {
                        return true;
                    }
                } else if let Some(password_hash) = &user.password_hash {
                    // Compare against hash (implement proper password hashing)
                    if self.verify_password_hash(password, password_hash) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Secure string comparison to prevent timing attacks
    fn secure_compare(&self, a: &str, b: &str) -> bool {
        if a.len() != b.len() {
            return false;
        }
        
        let mut result = 0u8;
        for (byte_a, byte_b) in a.bytes().zip(b.bytes()) {
            result |= byte_a ^ byte_b;
        }
        
        result == 0
    }

    /// Verify password against hash (placeholder implementation)
    fn verify_password_hash(&self, _password: &str, _hash: &str) -> bool {
        // TODO: Implement proper password hashing verification (bcrypt, argon2, etc.)
        warn!("Password hash verification not implemented - using plaintext comparison");
        false
    }

    /// Generate password hash (placeholder implementation)
    pub fn hash_password(&self, _password: &str) -> Result<String> {
        // TODO: Implement proper password hashing (bcrypt, argon2, etc.)
        Err(anyhow::anyhow!("Password hashing not implemented"))
    }

    /// Get proxy credentials by name
    pub fn get_proxy_credentials<'a>(&self, config: &'a SecureConfig, name: &str) -> Option<(&'a str, &'a str)> {
        for proxy in &config.proxy_credentials {
            if proxy.name == name {
                if let Some(password) = &proxy.password {
                    return Some((&proxy.username, password));
                }
            }
        }
        None
    }

    /// Get API key by name
    pub fn get_api_key<'a>(&self, config: &'a SecureConfig, name: &str) -> Option<&'a String> {
        config.api_keys.get(name)
    }

    /// Check if secrets are properly configured
    pub fn validate_secrets_config(&self) -> Result<()> {
        if self.config.encrypt_config {
            // Check if encryption key is available
            env::var(&self.config.config_encryption_key_env)
                .map_err(|_| anyhow::anyhow!("Encryption key not found in environment: {}", 
                                           self.config.config_encryption_key_env))?;
        }

        if self.config.use_env_secrets {
            debug!("Environment secrets are enabled with prefix: {}", self.config.env_prefix);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_simple_encryption_decryption() {
        let manager = SecretsManager::new(SecureConfigSettings::default());
        let original = "test secret data";
        let key = "encryption_key_123";
        
        let encrypted = manager.simple_encrypt(original, key);
        let decrypted = manager.simple_decrypt(&encrypted, key);
        
        assert_eq!(original, decrypted);
    }

    #[test]
    fn test_secure_compare() {
        let manager = SecretsManager::new(SecureConfigSettings::default());
        
        assert!(manager.secure_compare("password123", "password123"));
        assert!(!manager.secure_compare("password123", "password124"));
        assert!(!manager.secure_compare("password123", "password12"));
    }

    #[test]
    fn test_environment_loading() {
        env::set_var("SOCKS5_USER_0_USERNAME", "testuser");
        env::set_var("SOCKS5_USER_0_PASSWORD", "testpass");
        env::set_var("SOCKS5_API_KEY_TEST", "test_api_key");
        
        let mut manager = SecretsManager::new(SecureConfigSettings::default());
        let mut config = SecureConfig {
            auth_users: Vec::new(),
            proxy_credentials: Vec::new(),
            tls_certificates: Vec::new(),
            api_keys: HashMap::new(),
        };
        
        manager.load_from_environment(&mut config).unwrap();
        
        assert_eq!(config.auth_users.len(), 1);
        assert_eq!(config.auth_users[0].username, "testuser");
        assert_eq!(config.auth_users[0].password, Some("testpass".to_string()));
        assert_eq!(config.api_keys.get("test"), Some(&"test_api_key".to_string()));
        
        // Cleanup
        env::remove_var("SOCKS5_USER_0_USERNAME");
        env::remove_var("SOCKS5_USER_0_PASSWORD");
        env::remove_var("SOCKS5_API_KEY_TEST");
    }

    #[test]
    fn test_user_validation() {
        let manager = SecretsManager::new(SecureConfigSettings::default());
        let config = SecureConfig {
            auth_users: vec![
                SecureUserConfig {
                    username: "user1".to_string(),
                    password: Some("pass1".to_string()),
                    password_env: None,
                    password_hash: None,
                    enabled: true,
                    roles: vec!["user".to_string()],
                },
                SecureUserConfig {
                    username: "user2".to_string(),
                    password: Some("pass2".to_string()),
                    password_env: None,
                    password_hash: None,
                    enabled: false,
                    roles: vec!["user".to_string()],
                },
            ],
            proxy_credentials: Vec::new(),
            tls_certificates: Vec::new(),
            api_keys: HashMap::new(),
        };
        
        assert!(manager.validate_user_credentials(&config, "user1", "pass1"));
        assert!(!manager.validate_user_credentials(&config, "user1", "wrong"));
        assert!(!manager.validate_user_credentials(&config, "user2", "pass2")); // disabled
        assert!(!manager.validate_user_credentials(&config, "nonexistent", "pass"));
    }
}