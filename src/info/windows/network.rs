//! Primary IPv4 address from the adapter table.

use std::net::Ipv4Addr;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_SUCCESS};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST,
    IF_TYPE_ETHERNET_CSMACD, IF_TYPE_IEEE80211, IF_TYPE_SOFTWARE_LOOPBACK, IF_TYPE_TUNNEL,
    IP_ADAPTER_ADDRESSES_LH, IP_ADAPTER_UNICAST_ADDRESS_LH,
};
use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows_sys::Win32::Networking::WinSock::{AF_INET, SOCKADDR, SOCKADDR_IN};

/// Upper bound on the length of an adapter's friendly name, in UTF-16 units.
const MAX_NAME_UNITS: usize = 1024;

/// An up adapter with an IPv4 address.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Candidate {
    /// Friendly name such as `Wi-Fi` or `Ethernet`. Empty when unknown.
    name: String,
    address: Ipv4Addr,
    if_type: u32,
}

/// `"192.168.1.5 (Wi-Fi)"` for the best adapter.
pub(super) fn local_ip() -> Option<String> {
    let best = choose(adapters())?;
    Some(format_ip(best.address, &best.name))
}

fn format_ip(address: Ipv4Addr, name: &str) -> String {
    if name.is_empty() {
        address.to_string()
    } else {
        format!("{address} ({name})")
    }
}

/// Ethernet and Wi-Fi adapters win over every other kind. Within a tier the first
/// adapter in the table wins.
fn choose(candidates: Vec<Candidate>) -> Option<Candidate> {
    candidates
        .into_iter()
        .min_by_key(|candidate| tier(candidate.if_type))
}

fn tier(if_type: u32) -> u8 {
    if matches!(if_type, IF_TYPE_ETHERNET_CSMACD | IF_TYPE_IEEE80211) {
        0
    } else {
        1
    }
}

/// Every up adapter that has an IPv4 unicast address and is neither loopback nor tunnel.
fn adapters() -> Vec<Candidate> {
    let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    let family = u32::from(AF_INET);
    let mut size: u32 = 0;
    // SAFETY: a null table with size 0 asks only for the size the table needs.
    let status = unsafe { GetAdaptersAddresses(family, flags, null(), null_mut(), &mut size) };
    if status != ERROR_BUFFER_OVERFLOW || size == 0 {
        return Vec::new();
    }
    // u64 storage keeps the buffer aligned for the structures written into it.
    let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
    let first = buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
    // SAFETY: `first` points to `size` writable bytes, and the call fills them in.
    let status = unsafe { GetAdaptersAddresses(family, flags, null(), first, &mut size) };
    if status != ERROR_SUCCESS {
        return Vec::new();
    }

    let mut found = Vec::new();
    let mut current = first;
    // SAFETY: a successful call links the adapter records inside `buffer` with valid
    // pointers, and `buffer` lives until the end of this function.
    while let Some(adapter) = unsafe { current.as_ref() } {
        if let Some(candidate) = candidate(adapter) {
            found.push(candidate);
        }
        current = adapter.Next;
    }
    found
}

fn candidate(adapter: &IP_ADAPTER_ADDRESSES_LH) -> Option<Candidate> {
    if adapter.OperStatus != IfOperStatusUp
        || matches!(adapter.IfType, IF_TYPE_SOFTWARE_LOOPBACK | IF_TYPE_TUNNEL)
    {
        return None;
    }
    // SAFETY: the unicast list belongs to the adapter table that `adapter` points into.
    let address = unsafe { first_ipv4(adapter.FirstUnicastAddress) }?;
    // SAFETY: a friendly name is a NUL-terminated UTF-16 string in the table, or null.
    let name = unsafe { wide_string(adapter.FriendlyName.cast_const()) };
    Some(Candidate {
        name,
        address,
        if_type: adapter.IfType,
    })
}

/// The first IPv4 address of a unicast address list.
///
/// # Safety
///
/// `unicast` must be null or the head of a list of records that stay valid for the call.
unsafe fn first_ipv4(mut unicast: *mut IP_ADAPTER_UNICAST_ADDRESS_LH) -> Option<Ipv4Addr> {
    // SAFETY: the caller guarantees each node is valid; the list ends with a null pointer.
    while let Some(node) = unsafe { unicast.as_ref() } {
        // SAFETY: lpSockaddr is null or points to a socket address owned by the table.
        if let Some(socket) = unsafe { node.Address.lpSockaddr.as_ref() } {
            if socket.sa_family == AF_INET {
                // SAFETY: the family is AF_INET, so the storage holds a SOCKADDR_IN. The
                // record may be only 2-byte aligned, hence the unaligned read.
                let inet = unsafe {
                    (socket as *const SOCKADDR)
                        .cast::<SOCKADDR_IN>()
                        .read_unaligned()
                };
                // SAFETY: S_addr is the union member that holds the address.
                let raw = unsafe { inet.sin_addr.S_un.S_addr };
                // The address is in network byte order, so its in-memory bytes are the octets.
                let address = Ipv4Addr::from(raw.to_ne_bytes());
                if is_usable(address) {
                    return Some(address);
                }
            }
        }
        unicast = node.Next;
    }
    None
}

/// Link-local (169.254.0.0/16) addresses are self-assigned when DHCP fails and do not
/// identify the machine on the network.
fn is_usable(address: Ipv4Addr) -> bool {
    !address.is_link_local()
}

/// Text of a NUL-terminated UTF-16 string, or empty for a null pointer.
///
/// # Safety
///
/// `ptr` must be null or point to a readable string that has a NUL within
/// [`MAX_NAME_UNITS`] units, or that is at least that long.
unsafe fn wide_string(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut length = 0;
    // SAFETY: the caller guarantees the units up to the terminator, or the length cap, are
    // readable. Each unit is read only before the cap.
    while length < MAX_NAME_UNITS && unsafe { *ptr.add(length) } != 0 {
        length += 1;
    }
    // SAFETY: the first `length` units were read above.
    let units = unsafe { std::slice::from_raw_parts(ptr, length) };
    String::from_utf16_lossy(units)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str, address: [u8; 4], if_type: u32) -> Candidate {
        Candidate {
            name: name.to_string(),
            address: Ipv4Addr::from(address),
            if_type,
        }
    }

    #[test]
    fn ethernet_and_wifi_win_over_other_adapters() {
        let candidates = vec![
            fixture("vEthernet (Default Switch)", [172, 20, 0, 1], 53),
            fixture("Wi-Fi", [192, 168, 1, 5], IF_TYPE_IEEE80211),
            fixture("Ethernet", [10, 0, 0, 2], IF_TYPE_ETHERNET_CSMACD),
        ];
        assert_eq!(
            choose(candidates),
            Some(fixture("Wi-Fi", [192, 168, 1, 5], IF_TYPE_IEEE80211))
        );
    }

    #[test]
    fn first_adapter_wins_within_a_tier() {
        let candidates = vec![
            fixture("Ethernet 2", [10, 0, 0, 2], IF_TYPE_ETHERNET_CSMACD),
            fixture("Ethernet", [10, 0, 0, 3], IF_TYPE_ETHERNET_CSMACD),
        ];
        assert_eq!(
            choose(candidates).map(|best| best.name),
            Some("Ethernet 2".to_string())
        );
        assert_eq!(choose(Vec::new()), None);
    }

    #[test]
    fn address_text_names_the_adapter_when_known() {
        assert_eq!(
            format_ip(Ipv4Addr::new(192, 168, 1, 5), "Wi-Fi"),
            "192.168.1.5 (Wi-Fi)"
        );
        assert_eq!(format_ip(Ipv4Addr::new(10, 0, 0, 2), ""), "10.0.0.2");
    }

    #[test]
    fn link_local_addresses_are_not_reported() {
        assert!(!is_usable(Ipv4Addr::new(169, 254, 10, 20)));
        assert!(!is_usable(Ipv4Addr::new(169, 254, 0, 1)));
        assert!(is_usable(Ipv4Addr::new(192, 168, 1, 5)));
        assert!(is_usable(Ipv4Addr::new(169, 255, 0, 1)));
    }

    #[test]
    fn adapter_lookup_does_not_panic() {
        let _ = local_ip();
    }
}
