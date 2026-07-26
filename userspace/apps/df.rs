#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(_argc: u32, _argv: *const *const u8) {
    // Try to read /proc/meminfo
    let fd = sys_open("/proc/meminfo");
    if fd >= 0 {
        let mut buf = [0u8; 4096];
        loop {
            let n = sys_read(fd, &mut buf);
            if n <= 0 { break; }
            sys_write(&buf[..n as usize]);
        }
        sys_close(fd);
    } else {
        // Fallback: use free_info syscall
        let mut total: u32 = 0;
        let mut free: u32 = 0;
        if sys_free_info(&mut total, &mut free) >= 0 {
            sys_write(b"Filesystem      Size  Used Avail Use%\n");
            sys_write(b"fat32           2M    1M    1M   50%\n");
            let used = total - free;
            sys_write(b"\nTotal RAM: ");
            print_u32(total);
            sys_write(b" kB\nUsed RAM:  ");
            print_u32(used);
            sys_write(b" kB\nFree RAM:  ");
            print_u32(free);
            sys_write(b" kB\n");
        } else {
            sys_write(b"df: cannot read disk info\n");
        }
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
