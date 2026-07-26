#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) {
    // mount reads /proc/mounts
    let fd = sys_open("/proc/mounts");
    if fd >= 0 {
        let mut buf = [0u8; 2048];
        loop {
            let n = sys_read(fd, &mut buf);
            if n <= 0 { break; }
            sys_write(&buf[..n as usize]);
        }
        sys_close(fd);
    } else {
        sys_write(b"mount: /proc/mounts not available\n");
        sys_write(b"Usage: mount        - list mounted filesystems\n");
        sys_write(b"       mount -t <type> <device> <mountpoint>\n");
    }
}
