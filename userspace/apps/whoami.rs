#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(_argc: u32, _argv: *const *const u8) {
    let mut name_buf = [0u8; 256];
    let n = sys_getenv("USER", &mut name_buf);
    if n > 0 {
        let name = unsafe { core::str::from_utf8_unchecked(&name_buf[..n as usize]) };
        println!("{}", name);
    } else {
        println!("root");
    }
}
