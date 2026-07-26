#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) {
    if argc < 2 {
        sys_write(b"procinfo: procinfo <pid>\n");
        sys_exit_code(1);
    }

    let pid_str = unsafe { arg_at(argv, 1) };
    let mut pid: u32 = 0;
    for &b in pid_str.as_bytes() {
        if b >= b'0' && b <= b'9' {
            pid = pid * 10 + (b - b'0') as u32;
        }
    }

    let mut buf = [0u8; 1024];
    let n = sys_proc_info(pid, &mut buf);
    if n > 0 {
        sys_write(&buf[..n as usize]);
    } else {
        sys_write(b"procinfo: no info for PID ");
        print_u32(pid);
        sys_write(b"\n");
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
