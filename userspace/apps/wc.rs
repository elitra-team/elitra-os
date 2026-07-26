#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) {
    let mut lines: usize = 0;
    let mut words: usize = 0;
    let mut chars: usize = 0;

    let mut i = 1;
    while (i as u32) < argc {
        let ptr = unsafe { *argv.add(i) };
        if ptr.is_null() { break; }
        let path = unsafe { core::str::from_utf8_unchecked(
            core::slice::from_raw_parts(ptr, strlen(ptr))
        ) };

        let fd = sys_open(path);
        if fd < 0 {
            println!("wc: cannot open '{}'", path);
            i += 1;
            continue;
        }

        let mut in_word = false;
        let mut buf = [0u8; 512];
        loop {
            let n = sys_read(fd, &mut buf);
            if n <= 0 { break; }
            for j in 0..n as usize {
                chars += 1;
                if buf[j] == b'\n' { lines += 1; }
                if buf[j] == b' ' || buf[j] == b'\n' || buf[j] == b'\t' || buf[j] == b'\r' {
                    if in_word { words += 1; in_word = false; }
                } else {
                    in_word = true;
                }
            }
        }
        if in_word { words += 1; }
        sys_close(fd);
        i += 1;
    }

    print_u32(lines as u32);
    sys_write(b" ");
    print_u32(words as u32);
    sys_write(b" ");
    print_u32(chars as u32);
    sys_write(b"\n");
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
