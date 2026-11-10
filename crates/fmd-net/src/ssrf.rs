use crate::{NetError, Result};
use std::net::{IpAddr, Ipv4Addr};
use url::Url;

/// Checks if an IP address is considered safe (i.e. not loopback, private, link-local metadata, multicast, or broadcast).
pub fn is_safe_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => {
            let octets = ipv4.octets();
            // 0.0.0.0/8 (unspecified)
            if octets[0] == 0 {
                return false;
            }
            // 127.0.0.0/8 (loopback)
            if octets[0] == 127 {
                return false;
            }
            // 10.0.0.0/8 (RFC 1918)
            if octets[0] == 10 {
                return false;
            }
            // 172.16.0.0/12 (RFC 1918)
            if octets[0] == 172 && (16..=31).contains(&octets[1]) {
                return false;
            }
            // 192.168.0.0/16 (RFC 1918)
            if octets[0] == 192 && octets[1] == 168 {
                return false;
            }
            // 169.254.0.0/16 (link-local, cloud metadata AWS/GCP/Azure/OpenStack)
            if octets[0] == 169 && octets[1] == 254 {
                return false;
            }
            // 100.64.0.0/10 (carrier-grade NAT)
            if octets[0] == 100 && (64..=127).contains(&octets[1]) {
                return false;
            }
            // 224.0.0.0/4 (multicast)
            if octets[0] >= 224 && octets[0] <= 239 {
                return false;
            }
            // 255.255.255.255 (broadcast)
            if ipv4.is_broadcast() {
                return false;
            }
            true
        }
        IpAddr::V6(ipv6) => {
            if ipv6.is_loopback() || ipv6.is_unspecified() {
                return false;
            }
            let segments = ipv6.segments();
            // IPv4-mapped IPv6: ::ffff:a.b.c.d
            if segments[0] == 0
                && segments[1] == 0
                && segments[2] == 0
                && segments[3] == 0
                && segments[4] == 0
                && segments[5] == 0xffff
            {
                let octets = [
                    (segments[6] >> 8) as u8,
                    segments[6] as u8,
                    (segments[7] >> 8) as u8,
                    segments[7] as u8,
                ];
                return is_safe_ip(IpAddr::V4(Ipv4Addr::from(octets)));
            }
            // Unique local address fc00::/7
            if (segments[0] & 0xfe00) == 0xfc00 {
                return false;
            }
            // Link-local address fe80::/10
            if (segments[0] & 0xffc0) == 0xfe80 {
                return false;
            }
            // Multicast ff00::/8
            if (segments[0] & 0xff00) == 0xff00 {
                return false;
            }
            true
        }
    }
}

/// Validates that a URL is safe against SSRF (Server-Side Request Forgery).
///
/// Ensures:
/// - Protocol is strictly http or https
/// - Host is not localhost, local, internal, etc.
/// - Resolved IP is public and not in private/reserved ranges
pub async fn validate_safe_url(raw_url: &str) -> Result<Url> {
    let parsed = Url::parse(raw_url)
        .map_err(|e| NetError::Other(format!("Invalid URL '{}': {}", raw_url, e)))?;

    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(NetError::SsrfBlocked(format!(
            "Forbidden scheme '{}': only http and https are permitted",
            scheme
        )));
    }

    let host_str = parsed
        .host_str()
        .ok_or_else(|| NetError::SsrfBlocked("URL missing host component".to_string()))?;

    let host_lower = host_str.to_lowercase();
    if host_lower == "localhost"
        || host_lower.ends_with(".localhost")
        || host_lower.ends_with(".local")
        || host_lower.ends_with(".internal")
        || host_lower.ends_with(".lan")
    {
        return Err(NetError::SsrfBlocked(format!(
            "Forbidden local/internal domain '{}'",
            host_str
        )));
    }

    // If host is an IP literal
    if let Ok(ip) = host_str.parse::<IpAddr>() {
        if !is_safe_ip(ip) {
            return Err(NetError::SsrfBlocked(format!(
                "Forbidden destination IP address '{}'",
                ip
            )));
        }
        return Ok(parsed);
    }

    // Resolve domain name to IP addresses
    let port = parsed.port_or_known_default().unwrap_or(80);
    let socket_addrs = tokio::net::lookup_host(format!("{}:{}", host_str, port))
        .await
        .map_err(|e| NetError::Other(format!("DNS resolution failed for '{}': {}", host_str, e)))?;

    let mut resolved_any = false;
    for addr in socket_addrs {
        resolved_any = true;
        if !is_safe_ip(addr.ip()) {
            return Err(NetError::SsrfBlocked(format!(
                "Domain '{}' resolved to non-public IP address '{}'",
                host_str,
                addr.ip()
            )));
        }
    }

    if !resolved_any {
        return Err(NetError::Other(format!(
            "No IP addresses resolved for domain '{}'",
            host_str
        )));
    }

    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_safe_ip_blocks_private_and_loopback() {
        assert!(!is_safe_ip("127.0.0.1".parse().unwrap()));
        assert!(!is_safe_ip("127.1.2.3".parse().unwrap()));
        assert!(!is_safe_ip("10.0.0.1".parse().unwrap()));
        assert!(!is_safe_ip("10.255.255.254".parse().unwrap()));
        assert!(!is_safe_ip("172.16.0.1".parse().unwrap()));
        assert!(!is_safe_ip("172.31.255.255".parse().unwrap()));
        assert!(!is_safe_ip("192.168.1.1".parse().unwrap()));
        assert!(!is_safe_ip("169.254.169.254".parse().unwrap())); // AWS metadata
        assert!(!is_safe_ip("0.0.0.0".parse().unwrap()));
        assert!(!is_safe_ip("255.255.255.255".parse().unwrap()));
        assert!(!is_safe_ip("::1".parse().unwrap()));
        assert!(!is_safe_ip("::".parse().unwrap()));
        assert!(!is_safe_ip("fe80::1".parse().unwrap()));
        assert!(!is_safe_ip("fc00::1".parse().unwrap()));
        assert!(!is_safe_ip("::ffff:127.0.0.1".parse().unwrap()));
        assert!(!is_safe_ip("::ffff:169.254.169.254".parse().unwrap()));
    }

    #[test]
    fn test_is_safe_ip_allows_public_ips() {
        assert!(is_safe_ip("8.8.8.8".parse().unwrap()));
        assert!(is_safe_ip("1.1.1.1".parse().unwrap()));
        assert!(is_safe_ip("142.250.180.206".parse().unwrap()));
        assert!(is_safe_ip("2606:4700:4700::1111".parse().unwrap()));
    }

    #[tokio::test]
    async fn test_validate_safe_url_blocks_attacks() {
        assert!(validate_safe_url("file:///etc/passwd").await.is_err());
        assert!(validate_safe_url("gopher://127.0.0.1/").await.is_err());
        assert!(validate_safe_url("http://127.0.0.1:8080/api")
            .await
            .is_err());
        assert!(validate_safe_url("http://localhost:8088/").await.is_err());
        assert!(validate_safe_url("http://test.localhost/").await.is_err());
        assert!(validate_safe_url("http://169.254.169.254/latest/meta-data")
            .await
            .is_err());
        assert!(validate_safe_url("http://10.0.0.5:9090").await.is_err());
        assert!(validate_safe_url("http://192.168.1.100").await.is_err());
    }

    #[tokio::test]
    async fn test_validate_safe_url_allows_valid() {
        assert!(validate_safe_url("https://1.1.1.1/dns-query").await.is_ok());
    }
}
