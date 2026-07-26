#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(_argc: u32, _argv: *const *const u8) {
    println!("System halted.");
    sys_poweroff();
}
