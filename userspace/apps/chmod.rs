#![no_std]
#![no_main]

include!("../src/rt.rs");

fn parse_mode(s: &str) -> Option<u16> {
    let bytes = s.as_bytes();
    if bytes.len() < 2 || bytes.len() > 4 { return None; }
    // Check if it's octal (starts with 0)
    if bytes[0] == b'0' {
        let mut mode: u16 = 0;
        for &b in &bytes[1..] {
            if b < b'0' || b > b'7' { return None; }
            mode = mode * 8 + (b - b'0') as u16;
        }
        return Some(mode);
    }
    // Symbolic mode: u+x, g+w, etc.
    let mut mode: u16 = 0;
    let mut i = 0;
    while i < bytes.len() {
        let who = bytes[i];
        i += 1;
        if i >= bytes.len() { return None; }
        let op = bytes[i];
        i += 1;
        while i < bytes.len() && bytes[i] != b',' {
            let perm = bytes[i];
            let bit = match perm {
                b'r' => 4,
                b'w' => 2,
                b'x' => 1,
                _ => return None,
            };
            match who {
                b'u' | b'a' => mode |= bit << 6,
                b'g' | b'a' => mode |= bit << 3,
                b'o' | b'a' => mode |= bit,
                _ => {}
            }
            i += 1;
        }
        if i < bytes.len() { i += 1; } // skip comma
    }
    Some(mode)
}

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) {
    if argc < 3 {
        println!("Usage: chmod <mode> <path>");
        println!("  mode: octal (0755) or symbolic (u+x,g-w)");
        sys_exit();
    }
    let mode_str = unsafe { arg_at(argv, 1) };
    let path = unsafe { arg_at(argv, 2) };

    let mode = match parse_mode(mode_str) {
        Some(m) => m,
        None => {
            println!("chmod: invalid mode '{}'", mode_str);
            sys_exit();
        }
    };

    if sys_chmod(path, mode) < 0 {
        println!("chmod: failed to change mode of '{}'", path);
    }
}
