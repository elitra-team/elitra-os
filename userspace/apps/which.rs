#![no_std]
#![no_main]

include!("../src/rt.rs");

fn path_exists(path: &[u8]) -> bool {
    let s = core::str::from_utf8(path).unwrap_or("");
    let mut st = FileStat { type_: 0, size: 0, name: [0u8; 64], uid: 0, gid: 0, mode: 0 };
    sys_stat(s, &mut st) == 0
}

fn concat_path<'a>(dir: &'a [u8], name: &str, buf: &'a mut [u8]) -> &'a [u8] {
    let mut pos = 0;
    for &b in dir {
        if b == 0 { break; }
        if pos >= buf.len() - 1 { break; }
        buf[pos] = b;
        pos += 1;
    }
    if pos > 0 && buf[pos - 1] != b'/' { buf[pos] = b'/'; pos += 1; }
    for &b in name.as_bytes() {
        if pos >= buf.len() - 1 { break; }
        buf[pos] = b;
        pos += 1;
    }
    buf[pos] = 0;
    &buf[..=pos]
}

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) -> ! {
    if argc < 2 {
        sys_write(b"Usage: which <program>...\n");
        sys_exit();
    }

    let dirs: &[&[u8]] = &[b"/bin\0", b"/mnt/bin\0"];
    let mut any_found = false;

    for i in 1..argc as usize {
        let ptr = unsafe { *argv.add(i) };
        if ptr.is_null() { break; }
        let name = unsafe { core::str::from_utf8_unchecked(
            core::slice::from_raw_parts(ptr, strlen(ptr))
        ) };

        let mut found = false;

        for dir in dirs {
            let mut full_buf = [0u8; 256];
            let path = concat_path(dir, name, &mut full_buf);
            if path_exists(path) {
                let s = core::str::from_utf8(path).unwrap_or("");
                sys_write(s.as_bytes());
                sys_write(b"\n");
                found = true;
                any_found = true;
                break;
            }
        }
        if !found {
            sys_write(name.as_bytes());
            sys_write(b": not found\n");
        }
    }

    if any_found { sys_exit() } else { sys_exit_code(1) }
}
