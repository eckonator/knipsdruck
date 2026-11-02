// SPDX-License-Identifier: MIT OR Apache-2.0
//! Auswahl der IPv4-Adressen, unter denen das Handy den Computer im lokalen Netz erreicht.
//!
//! Die Bewertung ist eine reine Funktion über Paare aus Schnittstellenname und Adresse, damit sie sich ohne
//! echte Netzwerkkarten testen lässt. Bevorzugt werden private Adressen auf WLAN- und LAN-Schnittstellen;
//! VPN-, Docker- und VM-Schnittstellen kommen ans Ende.

use std::net::{IpAddr, Ipv4Addr};

/// Eine Adresse, unter der der Computer erreichbar sein könnte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    pub ip: Ipv4Addr,
    /// Aus dem Schnittstellennamen geraten: „WLAN“, „LAN“, „Netzwerk“, „VPN“, „Docker“, „VM“ oder „Lokal“
    pub label: String,
    /// Private Adresse auf einer echten Netzwerkschnittstelle: die wahrscheinlich richtige Wahl.
    pub preferred: bool,
}

/// Art der Schnittstelle; die Reihenfolge ist zugleich die Rangfolge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Wlan,
    Lan,
    Network,
    Vpn,
    Docker,
    Vm,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Wlan => "WLAN",
            Kind::Lan => "LAN",
            Kind::Network => "Netzwerk",
            Kind::Vpn => "VPN",
            Kind::Docker => "Docker",
            Kind::Vm => "VM",
        }
    }

    fn is_virtual(self) -> bool {
        matches!(self, Kind::Vpn | Kind::Docker | Kind::Vm)
    }
}

const VPN_PREFIXES: &[&str] = &["utun", "tun", "tap", "wg", "ppp", "ipsec", "tailscale", "zt"];
const VPN_WORDS: &[&str] = &["vpn", "wireguard", "tailscale", "zerotier", "tap-windows", "anyconnect"];
const DOCKER_PREFIXES: &[&str] = &["docker", "br-", "veth", "cni", "flannel", "podman"];
const VM_PREFIXES: &[&str] = &["vboxnet", "vmnet", "bridge", "virbr", "lxc", "lxd", "vnic", "anpi"];
const VM_WORDS: &[&str] = &["vethernet", "virtualbox", "vmware", "hyper-v", "wsl", "parallels"];
const WLAN_PREFIXES: &[&str] = &["wl", "wi-fi", "wifi"];
const WLAN_WORDS: &[&str] = &["wlan", "wi-fi", "wireless", "drahtlos"];
const LAN_PREFIXES: &[&str] = &["eth", "enp", "eno", "ens", "enx"];
const LAN_WORDS: &[&str] = &["ethernet", "lan-verbindung", "local area connection"];

/// Rät die Art der Schnittstelle aus ihrem Namen (Linux/macOS: `en0`, `wlp2s0`; Windows: „WLAN“, „Ethernet 2“).
fn kind_of(name: &str) -> Kind {
    let n = name.to_lowercase();
    let starts = |list: &[&str]| list.iter().any(|p| n.starts_with(p));
    let contains = |list: &[&str]| list.iter().any(|w| n.contains(w));
    if starts(VM_PREFIXES) || contains(VM_WORDS) {
        Kind::Vm
    } else if starts(DOCKER_PREFIXES) || n.contains("docker") {
        Kind::Docker
    } else if starts(VPN_PREFIXES) || contains(VPN_WORDS) {
        Kind::Vpn
    } else if starts(WLAN_PREFIXES) || contains(WLAN_WORDS) {
        Kind::Wlan
    } else if starts(LAN_PREFIXES) || contains(LAN_WORDS) {
        Kind::Lan
    } else {
        // u. a. macOS `en0`/`en1`: WLAN oder Kabel lässt sich am Namen nicht unterscheiden
        Kind::Network
    }
}

/// Private IPv4-Bereiche 10/8, 172.16/12 und 192.168/16.
fn is_private_v4(ip: Ipv4Addr) -> bool {
    ip.is_private()
}

/// Adressen, die für das Handy grundsätzlich nicht taugen.
fn is_unusable(ip: Ipv4Addr) -> bool {
    ip.is_loopback() || ip.is_link_local() || ip.is_unspecified() || ip.is_multicast() || ip.is_broadcast()
}

/// Bewertet und sortiert die Kandidaten: bevorzugte zuerst, virtuelle Schnittstellen zuletzt.
/// Doppelte Adressen werden nur einmal aufgeführt; die ursprüngliche Reihenfolge bleibt bei Gleichstand erhalten.
pub fn rank(candidates: &[(String, Ipv4Addr)]) -> Vec<Address> {
    let mut seen = Vec::new();
    let mut scored: Vec<((bool, bool, Kind), Address)> = Vec::new();
    for (name, ip) in candidates {
        if is_unusable(*ip) || seen.contains(ip) {
            continue;
        }
        seen.push(*ip);
        let kind = kind_of(name);
        let private = is_private_v4(*ip);
        let preferred = private && !kind.is_virtual();
        let key = (kind.is_virtual(), !private, kind);
        scored.push((
            key,
            Address {
                ip: *ip,
                label: kind.label().into(),
                preferred,
            },
        ));
    }
    scored.sort_by_key(|(key, _)| *key);
    scored.into_iter().map(|(_, a)| a).collect()
}

/// Ersatz, wenn gar keine Netzwerkadresse gefunden wurde: nur auf diesem Computer erreichbar.
fn loopback() -> Address {
    Address {
        ip: Ipv4Addr::LOCALHOST,
        label: "Lokal".into(),
        preferred: false,
    }
}

/// Liest die Schnittstellen des Systems; liefert immer mindestens eine Adresse.
pub fn local_addresses() -> Vec<Address> {
    let interfaces = if_addrs::get_if_addrs().unwrap_or_else(|e| {
        log::warn!("Netzwerkschnittstellen nicht lesbar: {e}");
        Vec::new()
    });
    let candidates: Vec<(String, Ipv4Addr)> = interfaces
        .into_iter()
        .filter(|i| i.oper_status != if_addrs::IfOperStatus::Down)
        .filter_map(|i| match i.addr {
            if_addrs::IfAddr::V4(v4) => Some((i.name, v4.ip)),
            if_addrs::IfAddr::V6(_) => None,
        })
        .collect();
    let mut ranked = rank(&candidates);
    if ranked.is_empty() {
        ranked.push(loopback());
    }
    ranked
}

/// Darf diese Gegenstelle mit uns reden? Nur private Netze und der Computer selbst.
pub fn is_allowed_peer(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_loopback(),
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6
                    .to_ipv4_mapped()
                    .is_some_and(|v4| v4.is_private() || v4.is_loopback())
                || (v6.segments()[0] & 0xfe00) == 0xfc00
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> Ipv4Addr {
        s.parse().unwrap()
    }

    fn ranked(list: &[(&str, &str)]) -> Vec<Address> {
        let c: Vec<(String, Ipv4Addr)> = list.iter().map(|(n, a)| (n.to_string(), ip(a))).collect();
        rank(&c)
    }

    #[test]
    fn labels_from_names() {
        assert_eq!(kind_of("en0"), Kind::Network);
        assert_eq!(kind_of("wlan0"), Kind::Wlan);
        assert_eq!(kind_of("wlp2s0"), Kind::Wlan);
        assert_eq!(kind_of("WLAN"), Kind::Wlan);
        assert_eq!(kind_of("Wi-Fi"), Kind::Wlan);
        assert_eq!(kind_of("eth0"), Kind::Lan);
        assert_eq!(kind_of("enp3s0"), Kind::Lan);
        assert_eq!(kind_of("Ethernet 2"), Kind::Lan);
        assert_eq!(kind_of("utun3"), Kind::Vpn);
        assert_eq!(kind_of("tun0"), Kind::Vpn);
        assert_eq!(kind_of("wg0"), Kind::Vpn);
        assert_eq!(kind_of("ppp0"), Kind::Vpn);
        assert_eq!(kind_of("Firmen-VPN"), Kind::Vpn);
        assert_eq!(kind_of("docker0"), Kind::Docker);
        assert_eq!(kind_of("br-1a2b3c"), Kind::Docker);
        assert_eq!(kind_of("veth12ab"), Kind::Docker);
        assert_eq!(kind_of("vboxnet0"), Kind::Vm);
        assert_eq!(kind_of("vmnet8"), Kind::Vm);
        assert_eq!(kind_of("bridge100"), Kind::Vm);
        assert_eq!(kind_of("vEthernet (WSL)"), Kind::Vm);
    }

    #[test]
    fn prefers_private_physical() {
        let r = ranked(&[
            ("utun3", "10.8.0.2"),
            ("docker0", "172.17.0.1"),
            ("en0", "192.168.178.23"),
            ("en5", "84.10.1.2"),
        ]);
        let ips: Vec<String> = r.iter().map(|a| a.ip.to_string()).collect();
        assert_eq!(ips, ["192.168.178.23", "84.10.1.2", "10.8.0.2", "172.17.0.1"]);
        assert!(r[0].preferred);
        assert!(!r[1].preferred, "öffentliche Adresse ist nicht bevorzugt");
        assert_eq!(r[2].label, "VPN");
        assert!(!r[2].preferred);
        assert_eq!(r[3].label, "Docker");
    }

    #[test]
    fn wlan_before_lan_and_order_kept() {
        let r = ranked(&[("eth0", "10.0.0.5"), ("wlan0", "192.168.1.9"), ("eth1", "172.20.1.1")]);
        let ips: Vec<String> = r.iter().map(|a| a.ip.to_string()).collect();
        assert_eq!(ips, ["192.168.1.9", "10.0.0.5", "172.20.1.1"]);
        assert!(r.iter().all(|a| a.preferred));
        assert_eq!(r[0].label, "WLAN");
        assert_eq!(r[1].label, "LAN");
    }

    #[test]
    fn skips_loopback_link_local_and_duplicates() {
        let r = ranked(&[
            ("lo0", "127.0.0.1"),
            ("en0", "169.254.10.3"),
            ("en1", "192.168.0.4"),
            ("en2", "192.168.0.4"),
        ]);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].ip, ip("192.168.0.4"));
    }

    #[test]
    fn empty_stays_empty() {
        assert!(ranked(&[]).is_empty());
        assert_eq!(loopback().ip, Ipv4Addr::LOCALHOST);
    }

    #[test]
    fn private_peers() {
        let ok = [
            "192.168.1.2",
            "10.1.2.3",
            "172.16.0.1",
            "172.31.255.255",
            "127.0.0.1",
            "::1",
            "::ffff:192.168.1.2",
        ];
        for a in ok {
            assert!(is_allowed_peer(a.parse().unwrap()), "{a}");
        }
        let bad = [
            "8.8.8.8",
            "172.32.0.1",
            "100.64.0.1",
            "169.254.1.1",
            "2001:db8::1",
            "::ffff:8.8.8.8",
        ];
        for a in bad {
            assert!(!is_allowed_peer(a.parse().unwrap()), "{a}");
        }
        assert!(is_allowed_peer("fd00::1".parse().unwrap()));
    }
}
