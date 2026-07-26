#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(_argc: u32, _argv: *const *const u8) {
    let fd = sys_open("/proc/stat");
    if fd >= 0 {
        sys_write(b"PID    FD     TYPE  PATH\n");
        sys_write(b"------ ------ ---- ------------------\n");
        // Read processes from /proc/stat
        let mut buf = [0u8; 2048];
        let mut total = 0;
        let mut ready = 0;
        let mut sleeping = 0;
        let mut zombie = 0;

        loop {
            let n = sys_read(fd, &mut buf);
            if n <= 0 { break; }
            for i in 0..n as usize {
                // Parse procs_running, procs_sleeping, procs_zombie
                if buf[i] == b'r' && i + 14 <= n as usize {
                    if &buf[i..i+14] == b"procs_running " {
                        let mut val = 0u32;
                        let mut j = i + 14;
                        while j < n as usize && buf[j] >= b'0' && buf[j] <= b'9' {
                            val = val * 10 + (buf[j] - b'0') as u32;
                            j += 1;
                        }
                        ready = val;
                    }
                }
                if buf[i] == b's' && i + 15 <= n as usize {
                    if &buf[i..i+15] == b"procs_sleeping " {
                        let mut val = 0u32;
                        let mut j = i + 15;
                        while j < n as usize && buf[j] >= b'0' && buf[j] <= b'9' {
                            val = val * 10 + (buf[j] - b'0') as u32;
                            j += 1;
                        }
                        sleeping = val;
                    }
                }
                if buf[i] == b'z' && i + 13 <= n as usize {
                    if &buf[i..i+13] == b"procs_zombie " {
                        let mut val = 0u32;
                        let mut j = i + 13;
                        while j < n as usize && buf[j] >= b'0' && buf[j] <= b'9' {
                            val = val * 10 + (buf[j] - b'0') as u32;
                            j += 1;
                        }
                        zombie = val;
                    }
                }
            }
        }
        sys_close(fd);

        total = ready + sleeping + zombie;
        sys_write(b"Total processes: ");
        print_u32(total);
        sys_write(b"\n");
        sys_write(b"  Running: ");
        print_u32(ready);
        sys_write(b"\n  Sleeping: ");
        print_u32(sleeping);
        sys_write(b"\n  Zombie: ");
        print_u32(zombie);
        sys_write(b"\n\nOpen FDs for current process (PID ");
        print_isize(sys_getpid());
        sys_write(b"):\n");

        // Show current process's stdin/stdout/stderr
        sys_write(b"  0  /dev/tty  (stdin)\n");
        sys_write(b"  1  /dev/tty  (stdout)\n");
        sys_write(b"  2  /dev/tty  (stderr)\n");
    } else {
        sys_write(b"lsof: /proc/stat not available\n");
    }
}

fn print_isize(val: isize) {
    if val < 0 {
        sys_write(b"-");
        print_u32((-val) as u32);
    } else {
        print_u32(val as u32);
    }
}

fn print_u32(val: u32) {
    if val == 0 { sys_write(b"0"); return; }
    let mut tmp = [0u8; 12];
    let mut n = 0;
    let mut v = val;
    while v > 0 { tmp[n] = b'0' + (v % 10) as u8; v /= 10; n += 1; }
    let mut i = n;
    while i > 0 { i -= 1; sys_write(&[tmp[i]]); }
}
