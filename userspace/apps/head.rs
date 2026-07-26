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
        println!("Usage: head [-n count] <file>");
        sys_exit();
    }

    let path = unsafe { arg_at(argv, path_arg_idx) };
    let fd = sys_open(path);
    if fd < 0 {
        println!("head: cannot open '{}'", path);
        sys_exit();
    }

    let mut buf = [0u8; 4096];
    let mut lines = 0usize;
    let mut total = 0usize;
    loop {
        let n = sys_read(fd, &mut buf[total..]);
        if n <= 0 { break; }
        total += n as usize;

        // Process complete lines in buffer
        let mut start = 0;
        for i in 0..total {
            if buf[i] == b'\n' {
                lines += 1;
                if lines <= num_lines {
                    sys_write(&buf[start..i + 1]);
                }
                start = i + 1;
                if lines >= num_lines { break; }
            }
        }
        if lines >= num_lines { break; }
        // Shift remaining data
        if start < total {
            let remaining = total - start;
            for i in 0..remaining {
                buf[i] = buf[start + i];
            }
            total = remaining;
        } else {
            total = 0;
        }
    }
    sys_close(fd);
}
