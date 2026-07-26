#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) {
    // umount <path> - just a stub for now
    if argc < 2 {
        sys_write(b"umount: umount <mountpoint>\n");
        sys_exit_code(1);
    }
    let path = unsafe { arg_at(argv, 1) };
    sys_write(b"umount: ");
    sys_write(path.as_bytes());
    sys_write(b" - not yet implemented (no umount syscall)\n");
    sys_exit_code(1);
}
