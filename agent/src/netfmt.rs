//! 网络信息的**纯格式化**：TCP 状态码/接口类型码 → 标签，网络字节序 → IPv4/端口串。
//!
//! 抽自 `win_native/net.rs`（T4）：`win_native.rs` 顶部有 `#![cfg(windows)]`，使这些
//! 纯映射在 macOS 上完全不可测。抽到本模块（`#[cfg(any(windows, test))]`）后，
//! `cargo test -p helm-agent` 在任何平台都能覆盖它们。
//!
//! 生产路径不变：`win_native::net` 照旧调用本模块的函数。

/// Windows `MIB_TCP_STATE` → 可读标签（未知值返回空串，调用方自行兜底）。
pub fn tcp_state_label(state: u32) -> String {
    let label = match state {
        2 => "LISTEN",
        3 => "SYN_SENT",
        4 => "SYN_RCVD",
        5 => "ESTABLISHED",
        6 => "FIN_WAIT1",
        7 => "FIN_WAIT2",
        8 => "CLOSE_WAIT",
        9 => "CLOSING",
        10 => "LAST_ACK",
        11 => "TIME_WAIT",
        12 => "DELETE_TCB",
        1 => "CLOSED",
        _ => return String::new(),
    };
    label.to_string()
}

/// IANA `IfType` → 标签（只覆盖实际会遇到的几类，其余归 other）。
pub fn if_type_label(t: u32) -> &'static str {
    match t {
        6 => "ethernet",
        71 => "wifi",
        24 => "loopback",
        53 | 131 => "tunnel",
        23 => "ppp",
        _ => "other",
    }
}

/// 内存中的网络字节序 u32 → IPv4 点分串（本机字节序读入 u32 后按字节还原）。
pub fn ipv4_str(addr: u32) -> String {
    let b = addr.to_ne_bytes();
    std::net::Ipv4Addr::new(b[0], b[1], b[2], b[3]).to_string()
}

/// 端口字段（低 16 位，网络字节序）→ 主机序端口号字符串。
pub fn port_str(field: u32) -> String {
    u16::from_be((field & 0xFFFF) as u16).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tcp_states_map_to_labels() {
        assert_eq!(tcp_state_label(5), "ESTABLISHED");
        assert_eq!(tcp_state_label(2), "LISTEN");
        assert_eq!(tcp_state_label(11), "TIME_WAIT");
        assert_eq!(tcp_state_label(1), "CLOSED");
    }

    #[test]
    fn tcp_unknown_state_is_empty() {
        assert_eq!(tcp_state_label(0), "");
        assert_eq!(tcp_state_label(99), "");
    }

    #[test]
    fn if_types_map_to_labels() {
        assert_eq!(if_type_label(6), "ethernet");
        assert_eq!(if_type_label(71), "wifi");
        assert_eq!(if_type_label(24), "loopback");
        assert_eq!(if_type_label(53), "tunnel");
        assert_eq!(if_type_label(131), "tunnel");
        assert_eq!(if_type_label(23), "ppp");
    }

    #[test]
    fn if_unknown_type_is_other() {
        assert_eq!(if_type_label(0), "other");
        assert_eq!(if_type_label(9999), "other");
    }

    #[test]
    fn ipv4_formats_dotted_quad() {
        // 127.0.0.1 按本机字节序组 u32（与 win_native 的读法一致）
        let addr = u32::from_ne_bytes([127, 0, 0, 1]);
        assert_eq!(ipv4_str(addr), "127.0.0.1");
    }

    #[test]
    fn port_decodes_network_byte_order() {
        // 约定：调用方用 u32::from_le_bytes 读内存，故「端口 80」在内存里是 [00 50 00 00]，
        // 组出的 u32 = 0x0000_5000。port_str 取低 16 位再做 from_be 还原成主机序 => 80。
        assert_eq!(port_str(0x0000_5000), "80", "HTTP");
        assert_eq!(port_str(0x0000_BB01), "443", "HTTPS");
    }
}
