//! GeoIP Support for Access Control

use std::net::IpAddr;
use std::path::Path;
use tracing::{debug, warn};

#[cfg(feature = "geoip")]
use maxminddb::{Reader, geoip2};

/// GeoIP database reader for country-based filtering
pub struct GeoIpReader {
    #[cfg(feature = "geoip")]
    reader: Option<Reader<Vec<u8>>>,
    #[cfg(not(feature = "geoip"))]
    _phantom: std::marker::PhantomData<()>,
}

impl GeoIpReader {
    /// Create a new GeoIP reader from database file
    pub fn new<P: AsRef<Path>>(_db_path: P) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        #[cfg(feature = "geoip")]
        {
            match Reader::open_readfile(_db_path) {
                Ok(reader) => {
                    debug!("Successfully loaded GeoIP database");
                    Ok(Self {
                        reader: Some(reader),
                    })
                }
                Err(e) => {
                    error!("Failed to load GeoIP database: {}", e);
                    Err(Box::new(e))
                }
            }
        }
        
        #[cfg(not(feature = "geoip"))]
        {
            warn!("GeoIP feature not enabled, creating disabled reader");
            Ok(Self {
                _phantom: std::marker::PhantomData,
            })
        }
    }

    /// Create a disabled GeoIP reader (no database)
    pub fn disabled() -> Self {
        #[cfg(feature = "geoip")]
        {
            Self { reader: None }
        }
        
        #[cfg(not(feature = "geoip"))]
        {
            Self {
                _phantom: std::marker::PhantomData,
            }
        }
    }

    /// Look up country code for an IP address
    pub fn lookup_country(&self, ip: IpAddr) -> Option<String> {
        #[cfg(feature = "geoip")]
        {
            if let Some(reader) = &self.reader {
                match reader.lookup::<geoip2::Country>(ip) {
                    Ok(country) => {
                        if let Some(country_info) = country.country {
                            if let Some(iso_code) = country_info.iso_code {
                                debug!("GeoIP lookup for {}: {}", ip, iso_code);
                                return Some(iso_code.to_string());
                            }
                        }
                        debug!("GeoIP lookup for {}: no country data", ip);
                    }
                    Err(e) => {
                        debug!("GeoIP lookup failed for {}: {}", ip, e);
                    }
                }
            }
            None
        }
        
        #[cfg(not(feature = "geoip"))]
        {
            debug!("GeoIP lookup for {} skipped (feature disabled)", ip);
            None
        }
    }

    /// Check if GeoIP is available
    pub fn is_available(&self) -> bool {
        #[cfg(feature = "geoip")]
        {
            self.reader.is_some()
        }
        
        #[cfg(not(feature = "geoip"))]
        {
            false
        }
    }
}

/// GeoIP-based access control helper
pub struct GeoIpFilter {
    reader: GeoIpReader,
}

impl GeoIpFilter {
    /// Create a new GeoIP filter
    pub fn new(reader: GeoIpReader) -> Self {
        Self { reader }
    }

    /// Check if an IP address is from an allowed country
    pub fn is_country_allowed(&self, ip: IpAddr, allowed_countries: &[String]) -> bool {
        if allowed_countries.is_empty() {
            // No country restrictions
            return true;
        }

        if let Some(country) = self.reader.lookup_country(ip) {
            let allowed = allowed_countries.iter().any(|c| c.eq_ignore_ascii_case(&country));
            debug!("Country check for {} ({}): allowed={}", ip, country, allowed);
            allowed
        } else {
            // If we can't determine the country, default to blocked for security
            warn!("Could not determine country for {}, blocking by default", ip);
            false
        }
    }

    /// Check if an IP address is from a blocked country
    pub fn is_country_blocked(&self, ip: IpAddr, blocked_countries: &[String]) -> bool {
        if blocked_countries.is_empty() {
            // No country blocks
            return false;
        }

        if let Some(country) = self.reader.lookup_country(ip) {
            let blocked = blocked_countries.iter().any(|c| c.eq_ignore_ascii_case(&country));
            debug!("Country block check for {} ({}): blocked={}", ip, country, blocked);
            blocked
        } else {
            // If we can't determine the country, don't block based on unknown country
            debug!("Could not determine country for {}, not blocking", ip);
            false
        }
    }

    /// Get country code for an IP address
    pub fn get_country(&self, ip: IpAddr) -> Option<String> {
        self.reader.lookup_country(ip)
    }

    /// Check if GeoIP is available
    pub fn is_available(&self) -> bool {
        self.reader.is_available()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_disabled_geoip_reader() {
        let reader = GeoIpReader::disabled();
        assert!(!reader.is_available());
        
        let ip = IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8));
        assert_eq!(reader.lookup_country(ip), None);
    }

    #[test]
    fn test_geoip_filter_no_restrictions() {
        let reader = GeoIpReader::disabled();
        let filter = GeoIpFilter::new(reader);
        
        let ip = IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8));
        
        // No restrictions should allow all
        assert!(filter.is_country_allowed(ip, &[]));
        assert!(!filter.is_country_blocked(ip, &[]));
    }

    #[test]
    fn test_geoip_filter_with_disabled_reader() {
        let reader = GeoIpReader::disabled();
        let filter = GeoIpFilter::new(reader);
        
        let ip = IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8));
        let allowed_countries = vec!["US".to_string(), "CA".to_string()];
        let blocked_countries = vec!["CN".to_string(), "RU".to_string()];
        
        // With disabled reader, country allowlist should block (can't verify country)
        assert!(!filter.is_country_allowed(ip, &allowed_countries));
        
        // With disabled reader, country blocklist should not block (can't verify country)
        assert!(!filter.is_country_blocked(ip, &blocked_countries));
    }
}