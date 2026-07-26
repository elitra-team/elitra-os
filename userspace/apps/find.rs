#![no_std]
#![no_main]

include!("../src/rt.rs");

fn find_files(path: &str, pattern: &str) {
    let mut buf = [0u8; 4096];
    let n = sys_readdir(path, &mut buf);
    if n <= 0 { return; }

    let mut i = 0;
    while (i as isize) < n {
        let start = i;
        while (i as isize) < n && buf[i] != b'\n' && buf[i] != b'/' { i += 1; }
        if i > start {
            let name = &buf[start..i];
            if name == b"." || name == b".." { i += 1; continue; }
            let name_str = unsafe { core::str::from_utf8_unchecked(name) };

            // Build full path
            let mut full = [0u8; 512];
            let mut pos = 0;
            for &b in path.as_bytes().iter() {
                if pos >= 500 { break; }
                full[pos] = b; pos += 1;
            }
            if pos > 0 && full[pos-1] != b'/' { full[pos] = b'/'; pos += 1; }
            for &b in name.iter() {
                if pos >= 500 { break; }
                full[pos] = b; pos += 1;
            }
            full[pos] = 0;

            let full_str = unsafe { core::str::from_utf8_unchecked(&full[..pos]) };

            // Check pattern match
            if match_pattern(name_str, pattern) {
                sys_write(full_str.as_bytes());
                sys_write(b"\n");
            }

            // Recurse into directories
            let mut st = FileStat { type_: 0, size: 0, name: [0u8; 64], uid: 0, gid: 0, mode: 0 };
            if sys_stat(full_str, &mut st) >= 0 && st.type_ == 1 {
                find_files(full_str, pattern);
            }
        }
        i += 1;
    }
}

fn match_pattern(name: &str, pattern: &str) -> bool {
    if pattern == "*" { return true; }
    // Simple substring match
    let nb = name.as_bytes();
    let pb = pattern.as_bytes();
    if pb[0] == b'*' && pb.len() > 1 {
        // *suffix — check if name ends with suffix
        let suffix = &pb[1..];
        if nb.len() >= suffix.len() {
            return &nb[nb.len() - suffix.len()..] == suffix;
        }
        return false;
    }
    // prefix* — check if name starts with prefix
    if pb[pb.len() - 1] == b'*' && pb.len() > 1 {
        let prefix = &pb[..pb.len() - 1];
        if nb.len() >= prefix.len() {
            return &nb[..prefix.len()] == prefix;
        }
        return false;
    }
    // Exact match
    name == pattern
}

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) {
    if argc < 2 {
        println!("Usage: find <path> <pattern>");
        sys_exit();
    }

    let path = unsafe { arg_at(argv, 1) };
    let pattern = if argc >= 3 { unsafe { arg_at(argv, 2) } } else { "*" };

    find_files(path, pattern);
}
