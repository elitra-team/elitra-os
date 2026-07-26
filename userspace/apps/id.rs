#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(_argc: u32, _argv: *const *const u8) {
    let uid = sys_getuid();
    let gid = sys_getgid();
    let euid = sys_geteuid();
    let egid = sys_getegid();

    let mut buf = [0u8; 128];
    let mut pos = 0;

    // uid=N(uid=N) gid=N(gid=N)
    let uid_s = b"uid=";
    for &b in uid_s.iter() { buf[pos] = b; pos += 1; }
    pos += write_u32(&mut buf[pos..], uid as u32);
    buf[pos] = b'('; pos += 1;
    buf[pos] = b'e'; pos += 1;
    buf[pos] = b'u'; pos += 1;
    buf[pos] = b'i'; pos += 1;
    buf[pos] = b'd'; pos += 1;
    buf[pos] = b'='; pos += 1;
    pos += write_u32(&mut buf[pos..], euid as u32);
    buf[pos] = b')'; pos += 1;
    buf[pos] = b' '; pos += 1;

    let gid_s = b"gid=";
    for &b in gid_s.iter() { buf[pos] = b; pos += 1; }
    pos += write_u32(&mut buf[pos..], gid as u32);
    buf[pos] = b'('; pos += 1;
    buf[pos] = b'e'; pos += 1;
    buf[pos] = b'g'; pos += 1;
    buf[pos] = b'i'; pos += 1;
    buf[pos] = b'd'; pos += 1;
    buf[pos] = b'='; pos += 1;
    pos += write_u32(&mut buf[pos..], egid as u32);
    buf[pos] = b')'; pos += 1;
    buf[pos] = b'\n'; pos += 1;

    sys_write(&buf[..pos]);
}

fn write_u32(buf: &mut [u8], val: u32) -> usize {
    if val == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut tmp = [0u8; 12];
    let mut n = 0;
    let mut v = val;
    while v > 0 { tmp[n] = b'0' + (v % 10) as u8; v /= 10; n += 1; }
    let mut i = n;
    let mut written = 0;
    while i > 0 {
        i -= 1;
        buf[written] = tmp[i];
        written += 1;
    }
    written
}
