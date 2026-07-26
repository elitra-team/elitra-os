#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) {
    // kill <pid> [signal]
    if argc < 2 {
        sys_write(b"kill: kill <pid> [signal]\n");
        sys_exit_code(1);
    }

    let pid_str = unsafe { arg_at(argv, 1) };
    let pid = parse_isize(pid_str);
    let sig = if argc > 2 {
        let sig_str = unsafe { arg_at(argv, 2) };
        parse_isize(sig_str)
    } else {
        15 // SIGTERM
    };

    let r = sys_kill(pid, sig as i32);
    if r < 0 {
        sys_write(b"kill: failed to send signal\n");
        sys_exit_code(1);
    }
}

fn parse_isize(s: &str) -> isize {
    let bytes = s.as_bytes();
    let mut neg = false;
    let mut start = 0;
    if bytes.len() > 0 && bytes[0] == b'-' {
        neg = true;
        start = 1;
    }
    let mut val: isize = 0;
    let mut i = start;
    while i < bytes.len() {
        if bytes[i] >= b'0' && bytes[i] <= b'9' {
            val = val * 10 + (bytes[i] - b'0') as isize;
        }
        i += 1;
    }
    if neg { -val } else { val }
}
