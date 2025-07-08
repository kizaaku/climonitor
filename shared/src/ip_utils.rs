use std::net::IpAddr;
use std::str::FromStr;

/// IP許可リストによるチェック
pub fn is_ip_allowed_by_list(peer_ip: &IpAddr, allowed_ips: &[String]) -> bool {
    // 許可リストが空の場合は全て許可
    if allowed_ips.is_empty() {
        return true;
    }

    for allowed_pattern in allowed_ips {
        if is_ip_match(peer_ip, allowed_pattern) {
            return true;
        }
    }

    false
}

/// IP許可チェック関数
fn is_ip_match(peer_ip: &IpAddr, pattern: &str) -> bool {
    // 特別なパターンを処理
    match pattern {
        "any" => return true,
        "localhost" => {
            return matches!(peer_ip, IpAddr::V4(ipv4) if ipv4.is_loopback())
                || matches!(peer_ip, IpAddr::V6(ipv6) if ipv6.is_loopback());
        }
        _ => {}
    }

    // 直接IPマッチを試行
    if let Ok(allowed_ip) = IpAddr::from_str(pattern) {
        return *peer_ip == allowed_ip;
    }

    // CIDR記法のチェック
    if let Some((network_str, prefix_len_str)) = pattern.split_once('/') {
        if let (Ok(network_ip), Ok(prefix_len)) = 
            (IpAddr::from_str(network_str), prefix_len_str.parse::<u8>()) 
        {
            return is_ip_in_cidr(peer_ip, &network_ip, prefix_len);
        }
    }

    false
}

/// CIDR記法でのIPチェック
fn is_ip_in_cidr(peer_ip: &IpAddr, network_ip: &IpAddr, prefix_len: u8) -> bool {
    match (peer_ip, network_ip) {
        (IpAddr::V4(peer), IpAddr::V4(network)) => {
            if prefix_len > 32 {
                return false;
            }
            let mask = !((1u32 << (32 - prefix_len)) - 1);
            u32::from_be_bytes(peer.octets()) & mask == u32::from_be_bytes(network.octets()) & mask
        }
        (IpAddr::V6(peer), IpAddr::V6(network)) => {
            if prefix_len > 128 {
                return false;
            }
            let peer_bytes = peer.octets();
            let network_bytes = network.octets();
            
            let full_bytes = prefix_len / 8;
            let remaining_bits = prefix_len % 8;
            
            // 完全バイトの比較
            if peer_bytes[..full_bytes as usize] != network_bytes[..full_bytes as usize] {
                return false;
            }
            
            // 残りビットの比較
            if remaining_bits > 0 && full_bytes < 16 {
                let mask = !((1u8 << (8 - remaining_bits)) - 1);
                if peer_bytes[full_bytes as usize] & mask != network_bytes[full_bytes as usize] & mask {
                    return false;
                }
            }
            
            true
        }
        _ => false, // IPv4とIPv6の混在は不一致
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ip_match_exact() {
        let ip = IpAddr::from_str("192.168.1.100").unwrap();
        assert!(is_ip_match(&ip, "192.168.1.100"));
        assert!(!is_ip_match(&ip, "192.168.1.101"));
    }

    #[test]
    fn test_ip_match_localhost() {
        let ip = IpAddr::from_str("127.0.0.1").unwrap();
        assert!(is_ip_match(&ip, "localhost"));
        
        let ip6 = IpAddr::from_str("::1").unwrap();
        assert!(is_ip_match(&ip6, "localhost"));
    }

    #[test]
    fn test_ip_match_cidr() {
        let ip = IpAddr::from_str("192.168.1.100").unwrap();
        assert!(is_ip_match(&ip, "192.168.1.0/24"));
        assert!(!is_ip_match(&ip, "192.168.2.0/24"));
    }

    #[test]
    fn test_ip_match_any() {
        let ip = IpAddr::from_str("192.168.1.100").unwrap();
        assert!(is_ip_match(&ip, "any"));
    }

    #[test]
    fn test_allowed_by_list() {
        let ip = IpAddr::from_str("192.168.1.100").unwrap();
        let allowed = vec!["192.168.1.0/24".to_string(), "localhost".to_string()];
        assert!(is_ip_allowed_by_list(&ip, &allowed));

        let forbidden = vec!["192.168.2.0/24".to_string()];
        assert!(!is_ip_allowed_by_list(&ip, &forbidden));

        // 空のリストは全て許可
        assert!(is_ip_allowed_by_list(&ip, &[]));
    }
}