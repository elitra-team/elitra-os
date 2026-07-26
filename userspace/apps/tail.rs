#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) {
    let mut num_lines: usize = 10;
    let mut path_arg_idx: usize = 1;

    // Parse -n <count>
    if argc >= 3 {
        let flag = unsafe { arg_at(argv, 1) };
        if flag == "-n" {
            let val = unsafe { arg_at(argv, 2) };
            let mut n: usize = 0;
            for &b in val.as_bytes() {
                if b >= b'0' && b <= b'9' {
                    n = n * 10 + (b - b'0') as usize;
                }
            }
            if n > 0 { num_lines = n; }
            path_arg_idx = 3;
        }
    }

    if path_arg_idx as u32 >= argc {
        println!("Usage: tail [-n count] <file>");
        sys_exit();
    }

    let path = unsafe { arg_at(argv, path_arg_idx) };
    let fd = sys_open(path);
    if fd < 0 {
        println!("tail: cannot open '{}'", path);
        sys_exit();
    }

    // Read entire file, then output last N lines
    let mut all = [0u8; 65536];
    let mut total = 0usize;
    loop {
        if total >= all.len() { break; }
        let n = sys_read(fd, &mut all[total..]);
        if n <= 0 { break; }
        total += n as usize;
    }
    sys_close(fd);

    // Count lines
    let mut line_count = 0usize;
    for i in 0..total {
        if all[i] == b'\n' { line_count += 1; }
    }

    // Find start of last num_lines lines
    let skip = if line_count > num_lines { line_count - num_lines } else { 0 };
    let mut lines_seen = 0usize;
    let mut start = 0;
    for i in 0..total {
        if lines_seen == skip {
            start = i;
            break;
        }
        if all[i] == b'\n' { lines_seen += 1; }
    }

    sys_write(&all[start..total]);
}
