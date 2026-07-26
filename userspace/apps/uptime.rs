#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(_argc: u32, _argv: *const *const u8) {
    let mut ts = Timespec { tv_sec: 0, tv_nsec: 0 };
    if sys_clock_gettime(CLOCK_MONOTONIC, &mut ts) >= 0 {
        let secs = ts.tv_sec;
        let days = secs / 86400;
        let h = (secs % 86400) / 3600;
        let m = (secs % 3600) / 60;
        let s = secs % 60;
        sys_write(b" ");
        print_u64(days);
        sys_write(b" days, ");
        print_u64(h);
        sys_write(b":");
        if m < 10 { sys_write(b"0"); }
        print_u64(m);
        sys_write(b":");
        if s < 10 { sys_write(b"0"); }
        print_u64(s);
        sys_write(b"\n");
    } else {
        sys_write(b"uptime: cannot read clock\n");
    }
}

fn print_u64(val: u64) {
    if val == 0 { sys_write(b"0"); return; }
    let mut tmp = [0u8; 20];
    let mut n = 0;
    let mut v = val;
    while v > 0 { tmp[n] = b'0' + (v % 10) as u8; v /= 10; n += 1; }
    let mut i = n;
    while i > 0 { i -= 1; sys_write(&[tmp[i]]); }
}
