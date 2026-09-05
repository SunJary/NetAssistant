// 客户端本地绑定地址解析
//
// TCP/UDP 客户端共用的本地绑定配置解析: 将 ClientConfig 中的
// local_address/local_port(留空=系统自动)解析为 SocketAddr。
// 仅接受 IP 字面量, 解析失败返回 Err 而非 panic。

use crate::config::connection::ClientConfig;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::str::FromStr;

/// 从客户端配置解析本地绑定地址。
///
/// - 地址与端口均未配置 → `Ok(None)`, 调用方保持系统默认行为(自动选网卡 + 临时端口);
/// - 地址留空时按远端地址族使用通配地址(IPv4 → 0.0.0.0, IPv6 → ::), 端口留空时为 0(自动分配);
/// - 本地地址族必须与远端一致, 否则报错(IPv4 socket 无法连接 IPv6 远端, 反之亦然)。
pub fn resolve_local_bind(
    config: &ClientConfig,
    remote: SocketAddr,
) -> Result<Option<SocketAddr>, String> {
    let addr_str = config
        .local_address
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let port = config.local_port;

    if addr_str.is_none() && port.is_none() {
        return Ok(None);
    }

    let ip = match addr_str {
        Some(s) => IpAddr::from_str(s)
            .map_err(|_| format!("无效的本地绑定地址 '{s}'(仅支持 IP, 不支持域名)"))?,
        // 未填本地地址时按远端地址族使用通配地址
        None if remote.is_ipv4() => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        None => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
    };

    if ip.is_ipv4() != remote.is_ipv4() {
        return Err(format!(
            "本地绑定地址 {ip} 与远端地址 {remote} 地址族不一致(IPv4/IPv6)"
        ));
    }

    Ok(Some(SocketAddr::new(ip, port.unwrap_or(0))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn client_config(local_address: Option<&str>, local_port: Option<u16>) -> ClientConfig {
        ClientConfig {
            local_address: local_address.map(str::to_string),
            local_port,
            ..Default::default()
        }
    }

    #[test]
    /// 未配置本地绑定时返回 None, 调用方保持系统默认行为
    fn test_unconfigured_returns_none() {
        let remote: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        assert_eq!(
            resolve_local_bind(&client_config(None, None), remote),
            Ok(None)
        );
    }

    #[test]
    /// 仅配置端口时按远端地址族使用通配地址
    fn test_port_only_uses_wildcard_by_remote_family() {
        let remote_v4: SocketAddr = "192.168.1.1:8080".parse().unwrap();
        assert_eq!(
            resolve_local_bind(&client_config(None, Some(50000)), remote_v4),
            Ok(Some("0.0.0.0:50000".parse().unwrap()))
        );

        let remote_v6: SocketAddr = "[::1]:8080".parse().unwrap();
        assert_eq!(
            resolve_local_bind(&client_config(None, Some(50000)), remote_v6),
            Ok(Some("[::]:50000".parse().unwrap()))
        );
    }

    #[test]
    /// 地址与端口同时配置时按原样解析; 空白地址等同未配置
    fn test_address_and_port() {
        let remote: SocketAddr = "10.0.0.1:8080".parse().unwrap();
        assert_eq!(
            resolve_local_bind(&client_config(Some("192.168.1.100"), Some(50000)), remote),
            Ok(Some("192.168.1.100:50000".parse().unwrap()))
        );
        // 端口留空 = 自动分配(0)
        assert_eq!(
            resolve_local_bind(&client_config(Some("192.168.1.100"), None), remote),
            Ok(Some("192.168.1.100:0".parse().unwrap()))
        );
        // 端口 0 = 自动分配, 允许
        assert_eq!(
            resolve_local_bind(&client_config(Some("127.0.0.1"), Some(0)), remote),
            Ok(Some("127.0.0.1:0".parse().unwrap()))
        );
        // 空白地址等同未配置
        assert_eq!(
            resolve_local_bind(&client_config(Some("  "), Some(50000)), remote),
            Ok(Some(
                format!("{}:50000", Ipv4Addr::UNSPECIFIED).parse().unwrap()
            ))
        );
    }

    #[test]
    /// 本地地址族与远端不一致时报错
    fn test_family_mismatch_rejected() {
        let remote_v4: SocketAddr = "10.0.0.1:8080".parse().unwrap();
        assert!(resolve_local_bind(&client_config(Some("::1"), Some(50000)), remote_v4).is_err());

        let remote_v6: SocketAddr = "[::1]:8080".parse().unwrap();
        assert!(
            resolve_local_bind(&client_config(Some("127.0.0.1"), Some(50000)), remote_v6).is_err()
        );
    }

    #[test]
    /// 域名等非 IP 字面量报错而非 panic
    fn test_hostname_rejected() {
        let remote: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        assert!(
            resolve_local_bind(&client_config(Some("localhost"), Some(50000)), remote).is_err()
        );
    }
}
