//! Primary IPv4 address (`getifaddrs`) and Wi-Fi link state (`/proc/net/wireless`).

use std::ffi::CStr;
use std::fs;
use std::net::Ipv4Addr;
use std::process::{Command, Stdio};

/// Interface name prefixes of virtual networks, least preferred for the primary address.
const VIRTUAL_PREFIXES: [&str; 6] = ["docker", "veth", "br-", "virbr", "tun", "tailscale"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Wifi {
    pub(super) iface: String,
    /// Signal level in dBm, when the driver reports one.
    pub(super) dbm: Option<i32>,
    /// Network name, from `iwgetid`.
    pub(super) ssid: Option<String>,
}

impl Wifi {
    pub(super) fn text(&self) -> String {
        let name = self.ssid.as_deref().unwrap_or(&self.iface);
        format_wifi(name, self.dbm)
    }
}

/// `"HomeNet (-52 dBm)"`, or just the name when no signal level is known.
pub(super) fn format_wifi(name: &str, dbm: Option<i32>) -> String {
    match dbm {
        Some(dbm) => format!("{name} ({dbm} dBm)"),
        None => name.to_string(),
    }
}

/// `"192.168.1.5 (wlan0)"` for the best candidate interface.
pub(super) fn local_ip() -> Option<String> {
    let (iface, address) = choose_interface(&interfaces())?;
    Some(format!("{address} ({iface})"))
}

/// Physical-looking names first, then other interfaces, then virtual ones.
fn choose_interface(candidates: &[(String, Ipv4Addr)]) -> Option<(String, Ipv4Addr)> {
    candidates
        .iter()
        .min_by_key(|(name, _)| tier(name))
        .cloned()
}

fn tier(name: &str) -> u8 {
    if VIRTUAL_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
    {
        2
    } else if name.starts_with(['e', 'w']) {
        0
    } else {
        1
    }
}

/// Every up, non-loopback interface with an IPv4 address.
fn interfaces() -> Vec<(String, Ipv4Addr)> {
    let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: on success getifaddrs stores the head of a list in `head`. The list is
    // released exactly once, below.
    if unsafe { libc::getifaddrs(&mut head) } != 0 {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut cursor = head;
    // SAFETY: each node stays valid until freeifaddrs, and the loop ends at the null tail.
    while let Some(entry) = unsafe { cursor.as_ref() } {
        if let Some(item) = interface_entry(entry) {
            found.push(item);
        }
        cursor = entry.ifa_next;
    }
    // SAFETY: `head` is the list returned by the successful getifaddrs call and is not
    // used after this point.
    unsafe { libc::freeifaddrs(head) };
    found
}

fn interface_entry(entry: &libc::ifaddrs) -> Option<(String, Ipv4Addr)> {
    let flags = entry.ifa_flags;
    if flags & (libc::IFF_UP as u32) == 0 || flags & (libc::IFF_LOOPBACK as u32) != 0 {
        return None;
    }
    if entry.ifa_addr.is_null() || entry.ifa_name.is_null() {
        return None;
    }
    // SAFETY: the address pointer is non-null and points into the getifaddrs list.
    let family = unsafe { (*entry.ifa_addr).sa_family };
    if i32::from(family) != libc::AF_INET {
        return None;
    }
    // SAFETY: the family is AF_INET, so the storage is a sockaddr_in.
    let socket = unsafe { &*entry.ifa_addr.cast::<libc::sockaddr_in>() };
    // The address is stored in network byte order, so its in-memory bytes are the octets.
    let address = Ipv4Addr::from(socket.sin_addr.s_addr.to_ne_bytes());
    // SAFETY: getifaddrs gives every interface name as a NUL-terminated string.
    let name = unsafe { CStr::from_ptr(entry.ifa_name) }
        .to_string_lossy()
        .into_owned();
    Some((name, address))
}

/// Interface, signal level and network name of the first wireless interface.
pub(super) fn wifi() -> Option<Wifi> {
    let text = fs::read_to_string("/proc/net/wireless").ok()?;
    let (iface, dbm) = parse_wireless(&text)?;
    let ssid = current_ssid();
    Some(Wifi { iface, dbm, ssid })
}

/// Interface name and dBm level from `/proc/net/wireless`. Positive levels are link
/// quality rather than dBm and are reported as `None`.
fn parse_wireless(text: &str) -> Option<(String, Option<i32>)> {
    for line in text.lines().skip(2) {
        let Some((iface, rest)) = line.split_once(':') else {
            continue;
        };
        let iface = iface.trim();
        if iface.is_empty() {
            continue;
        }
        // Fields after the name: status, link quality, signal level, noise, ...
        let level = rest
            .split_whitespace()
            .nth(2)
            .and_then(|value| value.trim_end_matches('.').parse::<i32>().ok())
            .filter(|level| *level < 0);
        return Some((iface.to_string(), level));
    }
    None
}

/// Network name from `iwgetid -r`. Only called once a wireless interface exists.
fn current_ssid() -> Option<String> {
    let output = Command::new("iwgetid")
        .arg("-r")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let ssid = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!ssid.is_empty()).then_some(ssid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wireless_line_gives_interface_and_dbm() {
        let text = "\
Inter-| sta-|   Quality        Discarded packets               Missed | WE
 face | tus | link level noise |  nwid  crypt   frag  retry   misc | beacon | 22
wlan0: 0000   70.  -52.  -256        0      0      0      0      0        0
";
        assert_eq!(parse_wireless(text), Some(("wlan0".to_string(), Some(-52))));
    }

    #[test]
    fn wireless_quality_without_dbm_has_no_level() {
        let text = "h1\nh2\nwlp2s0: 0000   60.  70.  0  0 0 0 0 0 0\n";
        assert_eq!(parse_wireless(text), Some(("wlp2s0".to_string(), None)));
    }

    #[test]
    fn no_wireless_interface_is_none() {
        assert_eq!(parse_wireless("h1\nh2\n"), None);
        assert_eq!(parse_wireless(""), None);
    }

    #[test]
    fn wifi_text_prefers_the_network_name() {
        assert_eq!(format_wifi("HomeNet", Some(-52)), "HomeNet (-52 dBm)");
        assert_eq!(format_wifi("wlan0", Some(-52)), "wlan0 (-52 dBm)");
        assert_eq!(format_wifi("wlan0", None), "wlan0");
    }

    #[test]
    fn physical_interfaces_win_over_virtual_ones() {
        let candidates = vec![
            ("docker0".to_string(), Ipv4Addr::new(172, 17, 0, 1)),
            ("wlan0".to_string(), Ipv4Addr::new(192, 168, 1, 5)),
            ("tailscale0".to_string(), Ipv4Addr::new(100, 64, 0, 2)),
        ];
        assert_eq!(
            choose_interface(&candidates),
            Some(("wlan0".to_string(), Ipv4Addr::new(192, 168, 1, 5)))
        );
    }

    #[test]
    fn other_interfaces_beat_virtual_ones() {
        let candidates = vec![
            ("br-1a2b".to_string(), Ipv4Addr::new(10, 0, 0, 1)),
            ("usb0".to_string(), Ipv4Addr::new(10, 0, 1, 1)),
        ];
        assert_eq!(
            choose_interface(&candidates),
            Some(("usb0".to_string(), Ipv4Addr::new(10, 0, 1, 1)))
        );
        assert_eq!(choose_interface(&[]), None);
    }

    #[test]
    fn first_candidate_wins_within_a_tier() {
        let candidates = vec![
            ("enp1s0".to_string(), Ipv4Addr::new(10, 0, 0, 2)),
            ("eth0".to_string(), Ipv4Addr::new(10, 0, 0, 3)),
        ];
        assert_eq!(
            choose_interface(&candidates).map(|(name, _)| name),
            Some("enp1s0".to_string())
        );
    }
}
