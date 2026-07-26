#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(_argc: u32, _argv: *const *const u8) {
    let mut buf = [0u8; 4096];
    let n = sys_list_env(&mut buf);
    if n > 0 {
        sys_write(&buf[..n as usize]);
    } else {
        // Fallback if no env vars set
        sys_write(b"HOME=/\n");
        sys_write(b"PATH=/bin:/mnt/bin\n");
        sys_write(b"USER=root\n");
    }
    sys_exit();
}
