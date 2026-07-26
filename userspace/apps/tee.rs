#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) -> ! {
    let mut out_fds = [0i32; 8];
    let mut out_cnt = 0usize;
    let mut append_mode = false;

    let mut i = 1;
    while (i as u32) < argc {
        let ptr = unsafe { *argv.add(i) };
        if ptr.is_null() { break; }
        let arg = unsafe { core::str::from_utf8_unchecked(
            core::slice::from_raw_parts(ptr, strlen(ptr))
        ) };
        if arg == "-a" || arg == "--append" {
            append_mode = true;
        } else if arg == "--" {
            i += 1;
            break;
        } else if arg.as_bytes().first() == Some(&b'-') && arg.len() > 1 {
            // skip other options
        } else {
            if out_cnt < out_fds.len() {
                let path = arg;
                if append_mode {
                    // Open for read to get size, then seek to end
                    let fd = sys_open(path);
                    if fd >= 0 {
                        // Seek to end for append
                        let mut st = FileStat { type_: 0, size: 0, name: [0u8; 64], uid: 0, gid: 0, mode: 0 };
                        if sys_stat(path, &mut st) >= 0 {
                            sys_lseek(fd, st.size as isize, 0); // SEEK_SET to size = end
                        }
                        out_fds[out_cnt] = fd as i32;
                        out_cnt += 1;
                    } else {
                        // File doesn't exist, create it
                        let fd = sys_open_write(path);
                        if fd < 0 {
                            sys_write(path.as_bytes());
                            sys_write(b": open failed\n");
                        } else {
                            out_fds[out_cnt] = fd as i32;
                            out_cnt += 1;
                        }
                    }
                } else {
                    let fd = sys_open_write(path);
                    if fd < 0 {
                        sys_write(path.as_bytes());
                        sys_write(b": open failed\n");
                    } else {
                        out_fds[out_cnt] = fd as i32;
                        out_cnt += 1;
                    }
                }
            }
        }
        i += 1;
    }

    loop {
        let mut buf = [0u8; 512];
        let n = sys_read(0, &mut buf);
        if n <= 0 { break; }
        let chunk = &buf[..n as usize];
        sys_write(chunk);
        for j in 0..out_cnt {
            sys_write_fd(out_fds[j], chunk);
        }
    }

    for j in 0..out_cnt {
        sys_close(out_fds[j] as isize);
    }
    sys_exit();
}
