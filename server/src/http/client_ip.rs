//! 客户端真实来源判定（EN-14）。
//!
//! 反代 + 容器部署链（EdgeOne → Caddy → 端口映射）下，容器内看到的 TCP 对端是
//! 网桥网关——审计与登录限速的「来源 IP」会退化为全局桶。仅当 TCP 对端落在
//! **可信代理网段**内时，才取 `X-Forwarded-For` 首段（最外层客户端）；否则一律
//! 用 TCP 对端地址，伪造 XFF 不生效；未配置可信网段时行为与无反代部署完全一致。

use std::net::{IpAddr, SocketAddr};

/// 单条可信网段（v4/v6）。`a.b.c.d/nn` 或裸 IP（视作 /32、/128）。
#[derive(Debug, Clone, PartialEq)]
pub struct Cidr {
    addr: IpAddr,
    prefix: u8,
}

impl Cidr {
    pub fn parse(s: &str) -> Option<Self> {
        let (ip_part, len_part) = match s.split_once('/') {
            Some((ip, l)) => (ip, Some(l)),
            None => (s, None),
        };
        let addr: IpAddr = ip_part.parse().ok()?;
        let max = match addr {
            IpAddr::V4(_) => 32,
            IpAddr::V6(_) => 128,
        };
        let len = match len_part {
            Some(l) => l.parse::<u8>().ok().filter(|&l| l <= max)?,
            None => max, // 裸 IP = /32 或 /128
        };
        Some(Self { addr, prefix: len })
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.addr, ip) {
            (IpAddr::V4(base), IpAddr::V4(other)) => {
                mask_v4(base, self.prefix) == mask_v4(other, self.prefix)
            }
            (IpAddr::V6(base), IpAddr::V6(other)) => {
                mask_v6(base, self.prefix) == mask_v6(other, self.prefix)
            }
            _ => false, // v4 网段不匹配 v6 地址，反之亦然
        }
    }
}

fn mask_v4(ip: std::net::Ipv4Addr, prefix: u8) -> u32 {
    let prefix = prefix.min(32);
    let raw = u32::from(ip);
    if prefix == 0 {
        return 0;
    }
    raw & (u32::MAX << (32 - prefix))
}

fn mask_v6(ip: std::net::Ipv6Addr, prefix: u8) -> u128 {
    let prefix = prefix.min(128);
    let raw = u128::from(ip);
    if prefix == 0 {
        return 0;
    }
    raw & (u128::MAX << (128 - prefix))
}

/// 解析逗号分隔的 CIDR / 裸 IP 列表；非法条目跳过。
pub fn parse_cidr_list(s: &str) -> Vec<Cidr> {
    s.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .filter_map(Cidr::parse)
        .collect()
}

/// 客户端来源判定（EN-14）：`peer` 命中任一可信网段时取 XFF 首段，
/// 否则返回 TCP 对端地址。XFF 缺失 / 首段非法时退回对端地址。
pub fn client_ip(peer: SocketAddr, xff: Option<&str>, trusted: &[Cidr]) -> IpAddr {
    let peer_ip = peer.ip();
    if trusted.is_empty() || !trusted.iter().any(|c| c.contains(peer_ip)) {
        return peer_ip;
    }
    xff.and_then(|v| v.split(',').next())
        .map(str::trim)
        .and_then(|s| s.parse::<IpAddr>().ok())
        .unwrap_or(peer_ip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn sa(ip: [u8; 4]) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::from(ip), 12345))
    }

    fn v4_list() -> Vec<Cidr> {
        parse_cidr_list("172.21.0.0/16, 10.0.0.5")
    }

    #[test]
    fn parse_handles_cidr_bare_and_invalid() {
        assert_eq!(v4_list().len(), 2);
        assert!(Cidr::parse("10.0.0.0/33").is_none());
        assert!(Cidr::parse("not-an-ip").is_none());
        assert!(Cidr::parse("2001:db8::/32").is_some());
    }

    #[test]
    fn trusted_peer_takes_first_xff_entry() {
        let peer = sa([172, 21, 0, 1]); // 网桥网关，命中 172.21.0.0/16
        let ip = client_ip(peer, Some("203.0.113.9, 10.0.0.1"), &v4_list());
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(203, 0, 113, 9)));
    }

    #[test]
    fn untrusted_peer_forging_xff_is_ignored() {
        let peer = sa([8, 8, 8, 8]); // 不在任何可信网段
        let ip = client_ip(peer, Some("1.2.3.4"), &v4_list());
        assert_eq!(ip, peer.ip(), "伪造 XFF 不得生效");
    }

    #[test]
    fn empty_config_keeps_peer_ip() {
        let peer = sa([172, 21, 0, 1]);
        let ip = client_ip(peer, Some("1.2.3.4"), &[]);
        assert_eq!(ip, peer.ip(), "未配置可信网段时行为与旧版一致");
    }

    #[test]
    fn bad_or_missing_xff_falls_back_to_peer() {
        let peer = sa([172, 21, 0, 1]);
        assert_eq!(client_ip(peer, None, &v4_list()), peer.ip());
        assert_eq!(client_ip(peer, Some("garbage"), &v4_list()), peer.ip());
    }

    #[test]
    fn bare_ip_cidr_matches_exactly() {
        let trusted = parse_cidr_list("10.0.0.5");
        assert!(trusted[0].contains(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5))));
        assert!(!trusted[0].contains(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 6))));
    }
}
