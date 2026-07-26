#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) {
    if argc < 2 {
        sys_write(b"tr: tr <set1> <set2>\n");
        sys_exit_code(1);
    }

    let set1 = unsafe { arg_at(argv, 1) };
    let set2 = if argc > 2 { unsafe { arg_at(argv, 2) } } else { "" };

    let mut buf = [0u8; 512];
    loop {
        let n = sys_read(0, &mut buf);
        if n <= 0 { break; }
        for i in 0..n as usize {
            let c = buf[i];
            let mut found = false;
            let mut j = 0;
            let s1 = set1.as_bytes();
            while j < s1.len() {
                if s1[j] == c {
                    found = true;
                    let s2 = set2.as_bytes();
                    if j < s2.len() {
                        sys_write(&[s2[j]]);
                    } else {
                        sys_write(&[c]);
                    }
                    break;
                }
                j += 1;
            }
            if !found {
                sys_write(&[c]);
            }
        }
    }
}
