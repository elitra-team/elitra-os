#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) {
    if argc < 3 {
        println!("Usage: ln <target> <linkpath>");
        sys_exit();
    }
    let target = unsafe { arg_at(argv, 1) };
    let linkpath = unsafe { arg_at(argv, 2) };

    if sys_symlink(target, linkpath) < 0 {
        println!("ln: failed to create symlink '{}' -> '{}'", linkpath, target);
    }
}
