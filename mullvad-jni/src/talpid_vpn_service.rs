use ipnetwork::IpNetwork;
use jnix::{
    jni::{
        objects::JObject,
        sys::{jboolean, jint, JNI_FALSE},
        JNIEnv,
    },
    IntoJava, JnixEnv,
};
use nix::sys::{
    select::{pselect, FdSet},
    time::{TimeSpec, TimeValLike},
};
use rand::{thread_rng, Rng};
use std::{
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket},
    os::unix::io::RawFd,
    time::{Duration, Instant},
};
use talpid_tunnel::tun_provider::TunConfig;
use talpid_types::ErrorExt;

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("Failed to verify the tunnel device")]
    VerifyTunDevice(#[from] SendRandomDataError),

    #[error("Failed to select() on tunnel device")]
    Select(#[from] nix::Error),

    #[error("Timed out while waiting for tunnel device to receive data")]
    TunnelDeviceTimeout,
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_net_mullvad_talpid_TalpidVpnService_defaultTunConfig<'env>(
    env: JNIEnv<'env>,
    _this: JObject<'_>,
) -> JObject<'env> {
    let env = JnixEnv::from(env);

    TunConfig::default().into_java(&env).forget()
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_net_mullvad_talpid_TalpidVpnService_waitForTunnelUp(
    _: JNIEnv<'_>,
    _this: JObject<'_>,
    tunFd: jint,
    isIpv6Enabled: jboolean,
) {
    let tun_fd = tunFd as RawFd;
    let is_ipv6_enabled = isIpv6Enabled != JNI_FALSE;

    if let Err(error) = wait_for_tunnel_up(tun_fd, is_ipv6_enabled) {
        log::error!(
            "{}",
            error.display_chain_with_msg("Failed to wait for tunnel device to be usable")
        );
    }
}

fn wait_for_tunnel_up(tun_fd: RawFd, is_ipv6_enabled: bool) -> Result<(), Error> {
    let mut fd_set = FdSet::new();
    fd_set.insert(tun_fd);
    let timeout = TimeSpec::microseconds(300);
    const TIMEOUT: Duration = Duration::from_secs(60);
    let start = Instant::now();
    while start.elapsed() < TIMEOUT {
        // if tunnel device is ready to be read from, traffic is being routed through it
        if pselect(None, Some(&mut fd_set), None, None, Some(&timeout), None)? > 0 {
            return Ok(());
        }
        // have to add tun_fd back into the bitset
        fd_set.insert(tun_fd);
        try_sending_random_udp(is_ipv6_enabled)?;
    }

    Err(Error::TunnelDeviceTimeout)
}

#[derive(Debug, thiserror::Error)]
enum SendRandomDataError {
    #[error("Failed to bind an UDP socket")]
    BindUdpSocket(#[source] io::Error),

    #[error("Failed to send random data through UDP socket")]
    SendToUdpSocket(#[source] io::Error),
}

fn try_sending_random_udp(is_ipv6_enabled: bool) -> Result<(), SendRandomDataError> {
    let mut tried_ipv6 = false;
    const TIMEOUT: Duration = Duration::from_millis(300);
    let start = Instant::now();

    while start.elapsed() < TIMEOUT {
        // TODO: if we are to allow LAN on Android by changing the routes that are stuffed in
        // TunConfig, then this should be revisited to be fair between IPv4 and IPv6
        let should_generate_ipv4 = !is_ipv6_enabled || tried_ipv6 || thread_rng().gen();
        let (bound_addr, random_public_addr) = random_socket_addrs(should_generate_ipv4);

        tried_ipv6 |= random_public_addr.ip().is_ipv6();

        let socket = UdpSocket::bind(bound_addr).map_err(SendRandomDataError::BindUdpSocket)?;
        match socket.send_to(&random_data(), random_public_addr) {
            Ok(_) => return Ok(()),
            // Always retry on IPv6 errors
            Err(_) if random_public_addr.ip().is_ipv6() => continue,
            Err(_err) if matches!(_err.raw_os_error(), Some(22) | Some(101)) => {
                // Error code 101 - specified network is unreachable
                // Error code 22 - specified address is not usable
                continue;
            }
            Err(err) => return Err(SendRandomDataError::SendToUdpSocket(err)),
        }
    }
    Ok(())
}

fn random_data() -> Vec<u8> {
    let mut buf = vec![0u8; thread_rng().gen_range(17..214)];
    thread_rng().fill(buf.as_mut_slice());
    buf
}

/// Returns a random local and public destination socket address.
/// If `ipv4` is true, then IPv4 addresses will be returned. Otherwise, IPv6 addresses will be
/// returned.
fn random_socket_addrs(ipv4: bool) -> (SocketAddr, SocketAddr) {
    loop {
        let rand_port = thread_rng().gen();
        let (local_addr, rand_dest_addr) = if ipv4 {
            let mut ipv4_bytes = [0u8; 4];
            thread_rng().fill(&mut ipv4_bytes);
            (
                SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0),
                SocketAddr::new(IpAddr::from(ipv4_bytes), rand_port),
            )
        } else {
            let mut ipv6_bytes = [0u8; 16];
            thread_rng().fill(&mut ipv6_bytes);
            (
                SocketAddr::new(Ipv6Addr::UNSPECIFIED.into(), 0),
                SocketAddr::new(IpAddr::from(ipv6_bytes), rand_port),
            )
        };

        // TODO: once https://github.com/rust-lang/rust/issues/27709 is resolved, please use
        // `is_global()` to check if a new address should be attempted.
        if !is_public_ip(rand_dest_addr.ip()) {
            continue;
        }

        return (local_addr, rand_dest_addr);
    }
}

fn is_public_ip(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V4(ipv4) => {
            // 0.x.x.x is not a publicly routable address
            if ipv4.octets()[0] == 0u8 {
                return false;
            }
        }
        IpAddr::V6(ipv6) => {
            if ipv6.segments()[0] == 0u16 {
                return false;
            }
        }
    }
    // A non-exhaustive list of non-public subnets
    let publicly_unroutable_subnets: Vec<IpNetwork> = vec![
        // IPv4 local networks
        "10.0.0.0/8".parse().unwrap(),
        "172.16.0.0/12".parse().unwrap(),
        "192.168.0.0/16".parse().unwrap(),
        // IPv4 non-forwardable network
        "169.254.0.0/16".parse().unwrap(),
        "192.0.0.0/8".parse().unwrap(),
        // Documentation networks
        "192.0.2.0/24".parse().unwrap(),
        "198.51.100.0/24".parse().unwrap(),
        "203.0.113.0/24".parse().unwrap(),
        // IPv6 publicly unroutable networks
        "fc00::/7".parse().unwrap(),
        "fe80::/10".parse().unwrap(),
    ];

    !publicly_unroutable_subnets
        .iter()
        .any(|net| net.contains(addr))
}
