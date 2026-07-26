pub mod ethernet;
pub mod ipv4;
pub mod tcp;
pub mod udp;
pub mod arp;
pub mod e1000;
pub mod dhcp;
pub mod icmp;
pub mod dns;

use crate::klib::uint8_t;
use crate::socket::{Socket, SocketType, SocketState, SocketAddrIn};
use crate::spinlock::SpinLock;
use core::sync::atomic::{AtomicU16, Ordering};

pub const EWOULDBLOCK: i32 = -100;
pub const MAX_SOCKETS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetError {
    BufferTooSmall,
    InvalidArgument,
    NoRoute,
    NotConnected,
    WouldBlock,
    Unknown,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SocketAddr {
    pub ip: [uint8_t; 4],
    pub port: u16,
}

impl SocketAddr {
    pub const fn new(ip: [uint8_t; 4], port: u16) -> Self {
        Self { ip, port }
    }
}

pub trait NetDevice {
    fn send(&mut self, buf: &[u8]) -> Result<(), NetError>;
    fn receive(&mut self, buf: &mut [u8]) -> Result<usize, NetError>;
    fn mac_address(&self) -> [u8; 6];
    fn ip_address(&self) -> [u8; 4];
}

pub struct NetworkStack<T: NetDevice> {
    device: T,
    arp_cache: [Option<([u8; 4], [u8; 6])>; 64],
}

impl<T: NetDevice> NetworkStack<T> {
    pub fn new(device: T) -> Self {
        Self {
            device,
            arp_cache: [None; 64],
        }
    }

    pub fn device(&mut self) -> &mut T {
        &mut self.device
    }

    pub fn discover_dhcp(&mut self) -> Result<(), NetError> {
        let mut packet = dhcp::build_dhcp_discover();

        let mac = self.device.mac_address();
        for i in 0..6 {
            packet[28 + i] = mac[i];
        }

        if let Some(mut client) = dhcp::dhcp_client() {
            if let Some(ref mut c) = *client {
                let xid = c.xid.wrapping_add(1);
                c.xid = xid;
                packet[4] = (xid >> 24) as u8;
                packet[5] = (xid >> 16) as u8;
                packet[6] = (xid >> 8) as u8;
                packet[7] = xid as u8;
            }
        }

        self.device.send(&packet).map_err(|_| NetError::Unknown)
    }

    pub fn handle_dhcp_packet(&mut self, packet: &[u8]) -> Result<(), NetError> {
        if packet.len() < core::mem::size_of::<dhcp::DhcpHeader>() {
            return Err(NetError::InvalidArgument);
        }

        if let Some(mut client_guard) = dhcp::dhcp_client() {
            if let Some(ref mut client) = *client_guard {
                match client.handle_offer(packet) {
                    Ok(()) => {
                        let ip = client.ip_address;
                        e1000::set_ip(ip);
                    }
                    Err(_) => {
                        if let Ok(()) = client.handle_ack(packet) {
                            let ip = client.ip_address;
                            e1000::set_ip(ip);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn start_dhcp_discovery(&mut self) -> Result<(), NetError> {
        let mac = self.device.mac_address();
        let _ = dhcp::init_dhcp_client(mac, [0, 0, 0, 0]);
        self.discover_dhcp()
    }

    pub fn handle_arp_packet(&mut self, opcode: u16, sender_mac: [u8; 6], sender_ip: [u8; 4]) {
        if opcode == arp::OP_REPLY {
            self.update_arp_cache(sender_ip, sender_mac);
        }
    }

    pub fn send_ipv4(&mut self, dest_ip: [u8; 4], protocol: u8, payload: &[u8]) -> Result<(), NetError> {
        let dest_mac = self.resolve_mac(dest_ip)?;
        let mut ip_packet = [0u8; 1500];
        let ip_len = ipv4::build_ipv4_packet(
            &mut ip_packet,
            self.device.ip_address(),
            dest_ip,
            protocol,
            payload,
        );
        self.send_ethernet(0x0800, dest_mac, &ip_packet[..ip_len])
    }

    pub fn send_ethernet(&mut self, ethertype: u16, dest_mac: [u8; 6], payload: &[u8]) -> Result<(), NetError> {
        let mut frame = [0u8; 1514];
        let frame_len = ethernet::build_ethernet_frame(
            &mut frame,
            self.device.mac_address(),
            dest_mac,
            ethertype,
            payload,
        );
        if frame_len == 0 {
            return Err(NetError::BufferTooSmall);
        }
        self.device.send(&frame[..frame_len])
    }

    pub fn update_arp_cache(&mut self, ip: [u8; 4], mac: [u8; 6]) {
        for entry in self.arp_cache.iter_mut() {
            if let Some((cached_ip, _)) = entry {
                if *cached_ip == ip {
                    *entry = Some((ip, mac));
                    return;
                }
            }
        }
        for entry in self.arp_cache.iter_mut() {
            if entry.is_none() {
                *entry = Some((ip, mac));
                return;
            }
        }
    }

    pub fn resolve_mac(&mut self, ip: [u8; 4]) -> Result<[u8; 6], NetError> {
        for entry in self.arp_cache.iter() {
            if let Some((cached_ip, mac)) = entry {
                if *cached_ip == ip {
                    return Ok(*mac);
                }
            }
        }

        let mut arp_request = [0u8; 42];
        arp::build_arp_request(
            &mut arp_request,
            self.device.mac_address(),
            self.device.ip_address(),
            ip,
        );
        let _ = self.device.send(&arp_request);

        // Retry ARP resolution: poll up to 50 times with small delays
        for _ in 0..50 {
            let mut buf = [0u8; 1500];
            while let Ok(len) = self.device.receive(&mut buf) {
                process_packet_raw(&mut self.arp_cache, self.device.ip_address(), &buf[..len]);
            }
            for entry in self.arp_cache.iter() {
                if let Some((cached_ip, mac)) = entry {
                    if *cached_ip == ip {
                        return Ok(*mac);
                    }
                }
            }
            // Small delay
            for _ in 0..10000 {
                core::hint::spin_loop();
            }
        }

        Err(NetError::NotConnected)
    }
}

// --- Global state ---

#[repr(C)]
pub struct NetConfig {
    pub ip: [uint8_t; 4],
    pub netmask: [uint8_t; 4],
    pub gateway: [uint8_t; 4],
    pub mac: [uint8_t; 6],
}

static NET_INITIALIZED: SpinLock<bool> = SpinLock::new(false);
static NET_CONFIG: SpinLock<NetConfig> = SpinLock::new(NetConfig {
    ip: [0, 0, 0, 0],
    netmask: [255, 255, 255, 0],
    gateway: [0, 0, 0, 0],
    mac: [0, 0, 0, 0, 0, 0],
});

static TCP_CONNECTIONS: SpinLock<[Option<tcp::TcpConnection>; MAX_SOCKETS]> = SpinLock::new([
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
]);

static SOCKETS: SpinLock<[Option<Socket>; MAX_SOCKETS]> = SpinLock::new([
    None, None, None, None, None, None, None, None,
    None, None, None, None, None, None, None, None,
]);

static NEXT_EPHEMERAL_PORT: AtomicU16 = AtomicU16::new(49152);

pub fn allocate_ephemeral_port() -> u16 {
    let port = NEXT_EPHEMERAL_PORT.fetch_add(1, Ordering::Relaxed);
    if port < 49152 {
        NEXT_EPHEMERAL_PORT.store(49152, Ordering::Relaxed);
        49152
    } else {
        port
    }
}

pub fn sockets() -> crate::spinlock::SpinLockGuard<'static, [Option<Socket>; MAX_SOCKETS]> {
    SOCKETS.lock()
}

pub fn init_net_stack(device: e1000::E1000Device) {
    let _initialized = NET_INITIALIZED.lock();
    *NET_STACK.lock() = Some(NetworkStack::new(device));
}

fn tcp_connections() -> crate::spinlock::SpinLockGuard<'static, [Option<tcp::TcpConnection>; MAX_SOCKETS]> {
    TCP_CONNECTIONS.lock()
}

// --- Socket API (called from syscalls) ---

pub fn net_socket(socket_type: u32) -> i32 {
    let stype = match socket_type {
        1 => SocketType::Stream,
        2 => SocketType::Datagram,
        _ => return -1,
    };
    let mut socks = sockets();
    for (fd, slot) in socks.iter_mut().enumerate() {
        if slot.is_none() {
            *slot = Some(Socket::new(stype));
            return fd as i32;
        }
    }
    -1 // no free slots
}

pub fn net_bind(sockfd: i32, port: u16) -> i32 {
    let mut socks = sockets();
    if sockfd < 0 || sockfd as usize >= MAX_SOCKETS {
        return -1;
    }
    // Check port not already bound (avoid double borrow by checking in same slice)
    for (i, s) in socks.iter().enumerate() {
        if i == sockfd as usize { continue; }
        if let Some(ref other) = s {
            if let Some(ref addr) = other.local_addr {
                if addr.port_be() == port {
                    return crate::socket::EADDRINUSE;
                }
            }
        }
    }
    if let Some(ref mut sock) = socks[sockfd as usize] {
        let addr = SocketAddrIn::new([0, 0, 0, 0], port);
        sock.bind(addr)
    } else {
        -1
    }
}

pub fn net_listen(sockfd: i32, backlog: i32) -> i32 {
    let mut socks = sockets();
    if sockfd < 0 || sockfd as usize >= MAX_SOCKETS {
        return -1;
    }
    if let Some(ref mut sock) = socks[sockfd as usize] {
        sock.listen(backlog)
    } else {
        -1
    }
}

pub fn net_connect(sockfd: i32, ip: [u8; 4], port: u16) -> i32 {
    let mut socks = sockets();
    if sockfd < 0 || sockfd as usize >= MAX_SOCKETS {
        return -1;
    }
    if let Some(ref mut sock) = socks[sockfd as usize] {
        let addr = SocketAddrIn::new(ip, port);
        let result = sock.connect(addr);
        if result == 0 {
            let local_port = sock.local_addr.as_ref().unwrap().port_be();
            let local_ip = NET_CONFIG.lock().ip;
            let local_addr = SocketAddr::new(local_ip, local_port);
            let remote_addr = SocketAddr::new(ip, port);
            let mut conn = tcp::TcpConnection::new(local_addr, remote_addr);
            conn.socket_fd = sockfd;
            if let Some((_seq, syn_packet)) = conn.build_syn() {
                let mut conns = tcp_connections();
                for slot in conns.iter_mut() {
                    if slot.is_none() {
                        *slot = Some(conn);
                        break;
                    }
                }
                drop(conns);
                drop(socks);
                if let Some(ref mut stack) = *NET_STACK.lock() {
                    let _ = stack.send_ipv4(ip, ipv4::PROTOCOL_TCP, &syn_packet);
                }
            }
        }
        result
    } else {
        -1
    }
}

pub fn net_sendto(sockfd: i32, data: &[u8], ip: [u8; 4], port: u16) -> Result<usize, i32> {
    let mut socks = sockets();
    if sockfd < 0 || sockfd as usize >= MAX_SOCKETS {
        return Err(-1);
    }
    if let Some(ref mut sock) = socks[sockfd as usize] {
        if sock.local_addr.is_none() {
            let ephemeral = allocate_ephemeral_port();
            sock.local_addr = Some(SocketAddrIn::new([0, 0, 0, 0], ephemeral));
            if sock.state == SocketState::Closed {
                sock.state = SocketState::Bound;
            }
        }

        let local_port = sock.local_addr.as_ref().unwrap().port_be();
        let src_ip = NET_CONFIG.lock().ip;

        match sock.socket_type {
            SocketType::Datagram => {
                let mut udp_packet = [0u8; 1500];
                let len = udp::build_udp_packet(
                    &mut udp_packet,
                    local_port,
                    port,
                    data,
                    &src_ip,
                    &ip,
                );
                if len == 0 {
                    return Err(crate::socket::EMSGSIZE);
                }
                drop(socks);
                if let Some(ref mut stack) = *NET_STACK.lock() {
                    stack.send_ipv4(ip, ipv4::PROTOCOL_UDP, &udp_packet[..len])
                        .map_err(|_| -1)?;
                }
                Ok(data.len())
            }
            SocketType::Stream => {
                let mut conns = tcp_connections();
                let mut sent = 0;
                for conn_opt in conns.iter_mut() {
                    if let Some(ref mut conn) = conn_opt {
                        if conn.local_addr.port == local_port
                            && conn.remote_addr.port == port
                            && conn.state == tcp::TcpState::Established
                        {
                            if let Some(ref result) = conn.send_data(data) {
                                let (_seq, ref packet) = result;
                                drop(conns);
                                drop(socks);
                                if let Some(ref mut stack) = *NET_STACK.lock() {
                                    if stack.send_ipv4(ip, ipv4::PROTOCOL_TCP, packet).is_ok() {
                                        sent = data.len();
                                    }
                                }
                                break;
                            }
                        }
                    }
                }
                if sent > 0 {
                    Ok(sent)
                } else {
                    Err(-1)
                }
            }
        }
    } else {
        Err(-1)
    }
}

pub fn net_recvfrom(sockfd: i32, buf: &mut [u8]) -> Result<(usize, [u8; 4], u16), i32> {
    let mut socks = sockets();
    if sockfd < 0 || sockfd as usize >= MAX_SOCKETS {
        return Err(-1);
    }
    if let Some(ref mut sock) = socks[sockfd as usize] {
        if !sock.has_data() {
            return Err(EWOULDBLOCK);
        }
        let len = sock.recv(buf).map_err(|e| e)?;
        let src_ip = sock.remote_addr.as_ref().map(|a| a.sin_addr).unwrap_or([0; 4]);
        let src_port = sock.remote_addr.as_ref().map(|a| a.port_be()).unwrap_or(0);
        Ok((len, src_ip, src_port))
    } else {
        Err(-1)
    }
}

pub fn net_close_socket(sockfd: i32) -> i32 {
    let mut socks = sockets();
    if sockfd < 0 || sockfd as usize >= MAX_SOCKETS {
        return -1;
    }
    if let Some(ref mut sock) = socks[sockfd as usize] {
        sock.close();
        0
    } else {
        -1
    }
}

pub fn net_accept(sockfd: i32, client_ip: *mut u8, client_port: *mut u16) -> i32 {
    let mut socks = sockets();
    if sockfd < 0 || sockfd as usize >= MAX_SOCKETS {
        return -1;
    }
    if let Some(ref mut sock) = socks[sockfd as usize] {
        if sock.state != SocketState::Listen {
            return -1;
        }
    } else {
        return -1;
    }
    let listen_port = if let Some(ref sock) = socks[sockfd as usize] {
        match sock.local_addr {
            Some(ref local) => local.port_be(),
            None => return -1,
        }
    } else {
        return -1;
    };
    drop(socks);

    let mut conns = tcp_connections();
    for conn_opt in conns.iter_mut() {
        if let Some(ref mut conn) = conn_opt {
            if conn.state == tcp::TcpState::Established
                && u16::from_be(conn.local_addr.port) == listen_port
            {
                let c_ip = [conn.remote_addr.ip[0], conn.remote_addr.ip[1], conn.remote_addr.ip[2], conn.remote_addr.ip[3]];
                let c_port = u16::from_be(conn.remote_addr.port);
                let l_ip = [conn.local_addr.ip[0], conn.local_addr.ip[1], conn.local_addr.ip[2], conn.local_addr.ip[3]];
                let l_port = u16::from_be(conn.local_addr.port);
                let r_ip = c_ip;
                let r_port = c_port;

                unsafe {
                    if !client_ip.is_null() {
                        for i in 0..4u64 {
                            core::ptr::write_volatile(client_ip.add(i as usize), c_ip[i as usize]);
                        }
                    }
                    if !client_port.is_null() {
                        core::ptr::write_volatile(client_port, c_port);
                    }
                }

                // Remove connection from the table
                *conn_opt = None;
                drop(conns);

                // Create a new connected socket
                let mut socks2 = sockets();
                for slot in socks2.iter_mut() {
                    if slot.is_none() {
                        let mut new_sock = Socket::new(SocketType::Stream);
                        new_sock.state = SocketState::Connected;
                        new_sock.local_addr = Some(SocketAddrIn::new(l_ip, l_port));
                        new_sock.remote_addr = Some(SocketAddrIn::new(r_ip, r_port));
                        *slot = Some(new_sock);
                        return (socks2.iter().position(|s| s.is_some()).unwrap_or(0)) as i32;
                    }
                }
                // Find the fd for the slot we just wrote
                let mut found = -1i32;
                for (fd, slot) in socks2.iter_mut().enumerate() {
                    if slot.as_ref().map_or(false, |s| s.state == SocketState::Connected && s.remote_addr.map_or(false, |r| r.sin_port == r_port.to_be())) {
                        found = fd as i32;
                        break;
                    }
                }
                return found;
            }
        }
    }
    -1
}

pub fn net_socket_has_data(sockfd: i32) -> bool {
    let mut socks = sockets();
    if sockfd < 0 || sockfd as usize >= MAX_SOCKETS {
        return false;
    }
    if let Some(ref sock) = socks[sockfd as usize] {
        sock.has_data()
    } else {
        false
    }
}

// --- Packet processing ---

static NET_STACK: SpinLock<Option<NetworkStack<e1000::E1000Device>>> = SpinLock::new(None);

#[no_mangle]
pub unsafe extern "C" fn krust_net_init(
    mac: *const uint8_t,
    ip: *const uint8_t,
    netmask: *const uint8_t,
    gateway: *const uint8_t,
) -> i32 {
    let mut initialized = NET_INITIALIZED.lock();
    if *initialized {
        return 0;
    }

    let mut config = NET_CONFIG.lock();
    for i in 0..6 {
        config.mac[i] = *mac.add(i);
    }
    for i in 0..4 {
        config.ip[i] = *ip.add(i);
        config.netmask[i] = *netmask.add(i);
        config.gateway[i] = *gateway.add(i);
    }
    let mac = config.mac;
    let ip = config.ip;
    drop(config);
    e1000::set_ip(ip);
    e1000::set_mac(mac);

    *initialized = true;
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_net_poll() -> i32 {
    if !*NET_INITIALIZED.lock() {
        return -1;
    }

    let mut stack_guard = NET_STACK.lock();
    if let Some(ref mut stack) = *stack_guard {
        let _ = stack.device().poll();

        let mut buf = [0u8; 1500];
        while let Ok(len) = stack.device().receive(&mut buf) {
            process_packet(stack, &buf[..len]);
        }
    }
    drop(stack_guard);

    tcp::tcp_retransmit_check();

    0
}

unsafe fn process_packet(stack: &mut NetworkStack<e1000::E1000Device>, data: &[u8]) {
    if let Some((ethertype, payload)) = ethernet::parse_ethernet_frame(data) {
        match ethertype {
            ethernet::ETHERTYPE_IPV4 => {
                if let Some((protocol, ip_payload)) = ipv4::parse_ipv4_packet(payload) {
                    match protocol {
                        ipv4::PROTOCOL_UDP => {
                            if let Some((_src_port, dst_port, udp_payload)) = udp::parse_udp_packet(ip_payload) {
                                if dst_port == 67 || dst_port == 68 {
                                    let _ = stack.handle_dhcp_packet(udp_payload);
                                } else {
                                    // Extract src_ip from IPv4 header for UDP delivery
                                    let src_ip = if payload.len() >= 20 {
                                        let ihl = ((payload[0] & 0x0F) as usize) * 4;
                                        if payload.len() >= ihl + 4 {
                                            [payload[12], payload[13], payload[14], payload[15]]
                                        } else { [0; 4] }
                                    } else { [0; 4] };
                                    deliver_udp_packet(dst_port, udp_payload, src_ip);
                                }
                            }
                        }
                        ipv4::PROTOCOL_TCP => {
                            if let Some((tcp_header, tcp_payload)) = tcp::parse_tcp_packet(ip_payload) {
                                let src_ip = if payload.len() >= 20 {
                                    [payload[12], payload[13], payload[14], payload[15]]
                                } else { [0; 4] };
                                let dst_ip = if payload.len() >= 20 {
                                    [payload[16], payload[17], payload[18], payload[19]]
                                } else { [0; 4] };
                                process_tcp_packet(stack, tcp_header, tcp_payload, src_ip, dst_ip);
                            }
                        }
                        ipv4::PROTOCOL_ICMP => {
                            let src_ip = if payload.len() >= 20 {
                                [payload[12], payload[13], payload[14], payload[15]]
                            } else { [0; 4] };
                            let dst_ip = if payload.len() >= 20 {
                                [payload[16], payload[17], payload[18], payload[19]]
                            } else { [0; 4] };
                            process_icmp_packet(stack, ip_payload, src_ip, dst_ip);
                        }
                        _ => {}
                    }
                }
            }
            ethernet::ETHERTYPE_ARP => {
                if let Some((opcode, sender_mac, sender_ip)) = arp::parse_arp_packet(payload) {
                    match opcode {
                        arp::OP_REQUEST => {
                            let mut reply = [0u8; 42];
                            let config = NET_CONFIG.lock();
                            arp::build_arp_reply(
                                &mut reply,
                                config.mac,
                                config.ip,
                                sender_mac,
                                sender_ip,
                            );
                            drop(config);
                            let _ = stack.device().send(&reply);
                        }
                        arp::OP_REPLY => {
                            stack.handle_arp_packet(opcode, sender_mac, sender_ip);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

// Raw packet processing for ARP cache updates during resolve_mac
fn process_packet_raw(
    arp_cache: &mut [Option<([u8; 4], [u8; 6])>; 64],
    _my_ip: [u8; 4],
    data: &[u8],
) {
    if let Some((ethertype, payload)) = ethernet::parse_ethernet_frame(data) {
        if ethertype == ethernet::ETHERTYPE_ARP {
            if let Some((opcode, sender_mac, sender_ip)) = arp::parse_arp_packet(payload) {
                if opcode == arp::OP_REPLY {
                    for entry in arp_cache.iter_mut() {
                        if let Some((cached_ip, _)) = entry {
                            if *cached_ip == sender_ip {
                                *entry = Some((sender_ip, sender_mac));
                                return;
                            }
                        }
                    }
                    for entry in arp_cache.iter_mut() {
                        if entry.is_none() {
                            *entry = Some((sender_ip, sender_mac));
                            return;
                        }
                    }
                }
            }
        }
    }
}

fn deliver_udp_packet(dst_port: u16, payload: &[u8], src_ip: [u8; 4]) {
    let mut socks = sockets();
    for sock_opt in socks.iter_mut() {
        if let Some(ref mut sock) = sock_opt {
            if sock.socket_type != SocketType::Datagram { continue; }
            if let Some(ref local) = sock.local_addr {
                if local.port_be() == dst_port {
                    let space = sock.recv_buffer.len() - sock.recv_pos;
                    let copy_len = core::cmp::min(payload.len(), space);
                    if copy_len > 0 {
                        sock.recv_buffer[sock.recv_pos..sock.recv_pos + copy_len]
                            .copy_from_slice(&payload[..copy_len]);
                        sock.recv_pos += copy_len;
                        sock.remote_addr = Some(SocketAddrIn::new(src_ip, dst_port.to_be()));
                    }
                    return;
                }
            }
        }
    }
}

fn process_tcp_packet(
    stack: &mut NetworkStack<e1000::E1000Device>,
    header: tcp::TcpHeader,
    payload: &[u8],
    src_ip: [u8; 4],
    _dst_ip: [u8; 4],
) {
    let dst_port = u16::from_be(header.dest_port);
    let src_port = u16::from_be(header.src_port);
    let flags = u16::from_be(header.data_offset_flags) & 0x3F;
    let seq_num = u32::from_be(header.seq_num);
    let ack_num = u32::from_be(header.ack_num);

    let mut conns = tcp_connections();
    for conn_opt in conns.iter_mut() {
        if let Some(ref mut conn) = conn_opt {
            if conn.local_addr.port == dst_port
                && (conn.remote_addr.port == 0 || conn.remote_addr.port == src_port)
            {
                if let Some((_ack, response)) = conn.process_segment(flags, seq_num, ack_num, u16::from_be(header.window_size), payload) {
                    let _ = stack.send_ipv4(src_ip, ipv4::PROTOCOL_TCP, &response[..]);
                }
                return;
            }
        }
    }

    if flags & tcp::FLAG_SYN != 0 && (flags & tcp::FLAG_ACK) == 0 {
        let mut socks = sockets();
        for sock_opt in socks.iter_mut() {
            if let Some(ref mut sock) = sock_opt {
                if sock.state == SocketState::Listen && sock.socket_type == SocketType::Stream {
                    if let Some(ref local) = sock.local_addr {
                        if local.port_be() == dst_port {
                            // Enforce backlog: count existing connections for this port
                            let backlog = sock.backlog;
                            if backlog > 0 {
                                let mut count = 0i32;
                                let conns_ref = tcp_connections();
                                for conn_opt in conns_ref.iter() {
                                    if let Some(ref conn) = conn_opt {
                                        if u16::from_be(conn.local_addr.port) == dst_port
                                            && (conn.state == tcp::TcpState::SynReceived
                                                || conn.state == tcp::TcpState::Established)
                                        {
                                            count += 1;
                                        }
                                    }
                                }
                                drop(conns_ref);
                                if count >= backlog {
                                    // Backlog full: silently drop SYN
                                    return;
                                }
                            }
                            drop(socks);

                            let my_ip = NET_CONFIG.lock().ip;
                            let local_addr = SocketAddr::new(my_ip, dst_port);
                            let remote_addr = SocketAddr::new(src_ip, src_port);
                            let mut conn = tcp::TcpConnection::new(local_addr, remote_addr);
                            conn.recv_seq = seq_num.wrapping_add(1);
                            conn.state = tcp::TcpState::SynReceived;
                            conn.update_activity();
                            conn.send_seq = conn.send_seq.wrapping_add(1);
                            if let Some((_ack, response)) = conn.process_segment(
                                tcp::FLAG_SYN | tcp::FLAG_ACK,
                                seq_num, 0, tcp::DEFAULT_WINDOW, &[],
                            ) {
                                let _ = stack.send_ipv4(src_ip, ipv4::PROTOCOL_TCP, &response[..]);
                            }
                            let mut conns = tcp_connections();
                            for slot in conns.iter_mut() {
                                if slot.is_none() {
                                    *slot = Some(conn);
                                    break;
                                }
                            }
                            return;
                        }
                    }
                }
            }
        }
    }
}

fn process_icmp_packet(
    stack: &mut NetworkStack<e1000::E1000Device>,
    data: &[u8],
    src_ip: [u8; 4],
    _dst_ip: [u8; 4],
) {
    if let Some((icmp_type, icmp_id, icmp_seq, icmp_payload)) = icmp::parse_icmp_packet(data) {
        match icmp_type {
            icmp::ICMP_ECHO_REQUEST => {
                let _my_ip = NET_CONFIG.lock().ip;
                let mut reply_buf = [0u8; 1500];
                let len = icmp::build_icmp_reply(&mut reply_buf, icmp_id, icmp_seq, icmp_payload);
                if len > 0 {
                    let _ = stack.send_ipv4(src_ip, ipv4::PROTOCOL_ICMP, &reply_buf[..len]);
                }
            }
            icmp::ICMP_ECHO_REPLY => {
                // Could notify waiting socket — for now just ignore
            }
            _ => {}
        }
    }
}
