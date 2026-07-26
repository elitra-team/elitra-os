#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) {
    if argc < 2 {
        println!("Usage: grep <pattern> [file]");
        println!("  Without file, reads from stdin.");
        sys_exit();
    }

    let pattern = unsafe { arg_at(argv, 1) };
    let pb = pattern.as_bytes();

    let mut reading_stdin = true;
    let mut fd = -1isize;

    if argc >= 3 {
        let path = unsafe { arg_at(argv, 2) };
        fd = sys_open(path);
        if fd < 0 {
            println!("grep: cannot open '{}'", path);
            sys_exit();
        }
        reading_stdin = false;
    }

    let mut line_buf = [0u8; 4096];
    let mut line_len = 0usize;
    let mut byte_buf = [0u8; 1];

    loop {
        let n = if reading_stdin {
            sys_read(0, &mut byte_buf)
        } else {
            sys_read(fd, &mut byte_buf)
        };
        if n <= 0 { break; }

        if byte_buf[0] == b'\n' {
            // Process complete line
            if line_len > 0 && pattern_match(&line_buf[..line_len], pb) {
                sys_write(&line_buf[..line_len]);
                sys_write(b"\n");
            }
            line_len = 0;
        } else {
            if line_len < line_buf.len() {
                line_buf[line_len] = byte_buf[0];
                line_len += 1;
            }
        }
    }
    // Handle last line without newline
    if line_len > 0 && pattern_match(&line_buf[..line_len], pb) {
        sys_write(&line_buf[..line_len]);
        sys_write(b"\n");
    }

    if fd >= 0 { sys_close(fd); }
}

fn pattern_match(line: &[u8], pattern: &[u8]) -> bool {
    if pattern.is_empty() { return true; }
    if line.len() < pattern.len() { return false; }
    // Simple substring search
    for i in 0..=(line.len() - pattern.len()) {
        if &line[i..i + pattern.len()] == pattern {
            return true;
        }
    }
    false
}
