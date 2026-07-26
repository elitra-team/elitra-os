#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(_argc: u32, _argv: *const *const u8) {
    let mut total: u32 = 0;
    let mut free: u32 = 0;
    if sys_free_info(&mut total, &mut free) >= 0 {
        let used = total - free;
        let pct = if total > 0 { used * 100 / total } else { 0 };
        sys_write(b"MemTotal:      ");
        print_u32(total);
        sys_write(b" kB\n");
        sys_write(b"MemFree:       ");
        print_u32(free);
        sys_write(b" kB\n");
        sys_write(b"MemAvailable:  ");
        print_u32(free);
        sys_write(b" kB\n");
        sys_write(b"Buffers:       0 kB\n");
        sys_write(b"Cached:        0 kB\n");
        sys_write(b"SwapTotal:     0 kB\n");
        sys_write(b"SwapFree:      0 kB\n");
    } else {
        sys_write(b"free: cannot read memory info\n");
        sys_exit_code(1);
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
