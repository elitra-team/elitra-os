#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) {
    if argc < 2 {
        println!("Usage: ping <ip>");
        println!("Example: ping 10.0.2.2");
        return;
    }

    let ip_str = unsafe { arg_at(argv, 1) };
    let ip = parse_ip(ip_str);
    if ip.is_none() {
        println!("ping: invalid IP address: {}", ip_str);
        return;
    }
    let ip = ip.unwrap();

    let sock = sys_socket(2); // SOCK_DGRAM
    if sock < 0 {
        println!("ping: failed to create socket");
        return;
    }

    println!("PING {} ({}.{}.{}.{}) 56 bytes of data",
        ip_str, ip[0], ip[1], ip[2], ip[3]);

    let mut seq: u32 = 0;
    let mut received: u32 = 0;
    let mut sent: u32 = 0;

    for _ in 0..4 {
        seq += 1;
        sent += 1;

        // Build ICMP echo request (type 8, code 0)
        let mut icmp = [0u8; 64];
        icmp[0] = 8; // type: echo request
        icmp[1] = 0; // code
        icmp[4] = (seq >> 8) as u8; // id high
        icmp[5] = seq as u8;        // id low
        icmp[6] = (seq >> 8) as u8; // seq high
        icmp[7] = seq as u8;        // seq low

        // Fill with pattern data
        for i in 8..64 {
            icmp[i] = (i & 0xFF) as u8;
        }

        // Compute ICMP checksum
        let cksum = icmp_checksum(&icmp);
        icmp[2] = (cksum >> 8) as u8;
        icmp[3] = cksum as u8;

        let result = sys_sendto(sock, &icmp, &ip, 0);
        if result < 0 {
            println!("ping: send failed");
            continue;
        }

        sys_sleep(500);

        // Try to receive
        let mut recv_buf = [0u8; 128];
        match sys_recvfrom(sock, &mut recv_buf) {
            Ok((n, _info)) => {
                if n >= 8 && recv_buf[0] == 0 { // echo reply
                    received += 1;
                    println!("64 bytes from {}: icmp_seq={} ttl=64 time=1ms",
                        ip_str, seq);
                }
            }
            Err(_) => {
                println!("Request timeout for icmp_seq {}", seq);
            }
        }

        sys_sleep(500);
    }

    println!("--- {} ping statistics ---", ip_str);
    println!("{} packets transmitted, {} received, {}% packet loss",
        sent, received, if sent > 0 { ((sent - received) * 100 / sent) } else { 0 });

    sys_close_socket(sock);
}

fn parse_ip(s: &str) -> Option<[u8; 4]> {
    let bytes = s.as_bytes();
    let mut parts = [0u8; 4];
    let mut part_idx = 0;
    let mut current = 0u16;
    let mut has_digit = false;

    for &b in bytes {
        if b >= b'0' && b <= b'9' {
            current = current * 10 + (b - b'0') as u16;
            has_digit = true;
            if current > 255 { return None; }
        } else if b == b'.' {
            if !has_digit { return None; }
            if part_idx >= 3 { return None; }
            parts[part_idx] = current as u8;
            part_idx += 1;
            current = 0;
            has_digit = false;
        } else {
            return None;
        }
    }
    if !has_digit || part_idx != 3 { return None; }
    parts[part_idx] = current as u8;
    Some(parts)
}

fn icmp_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < data.len() {
        sum += ((data[i] as u32) << 8) | (data[i + 1] as u32);
        i += 2;
    }
    if i < data.len() {
        sum += (data[i] as u32) << 8;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !sum as u16
}
