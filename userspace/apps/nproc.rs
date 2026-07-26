#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(_argc: u32, _argv: *const *const u8) {
    // Show number of CPUs (we know SMP exists but may be 1)
    sys_write(b"1\n");
}
