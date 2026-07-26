#![no_std]
#![no_main]

include!("../src/rt.rs");

fn cp_file(src: &str, dst: &str) -> isize {
    let fd_src = sys_open(src);
    if fd_src < 0 { return -1; }

    let fd_dst = sys_open_write(dst);
    if fd_dst < 0 {
        sys_close(fd_src);
        return -1;
    }

    let mut buf = [0u8; 4096];
    loop {
        let n = sys_read(fd_src, &mut buf);
        if n <= 0 { break; }
        let chunk = &buf[..n as usize];
        if sys_write_fd(fd_dst as i32, chunk) < 0 {
            sys_close(fd_src);
            sys_close(fd_dst);
            return -1;
        }
    }

    sys_close(fd_src);
    sys_close(fd_dst);
    0
}

fn is_dir(path: &str) -> bool {
    let mut st = FileStat { type_: 0, size: 0, name: [0u8; 64], uid: 0, gid: 0, mode: 0 };
    if sys_stat(path, &mut st) < 0 { return false; }
    st.type_ == 1 // NODE_DIR
}

fn mkdir_p(path: &str) {
    sys_mkdir(path);
}

fn cp_recursive(src: &str, dst: &str) -> isize {
    if !is_dir(src) {
        return cp_file(src, dst);
    }

    mkdir_p(dst);

    let mut buf = [0u8; 4096];
    let n = sys_readdir(src, &mut buf);
    if n <= 0 { return 0; }

    let mut i = 0;
    while (i as isize) < n {
        let start = i;
        while (i as isize) < n && buf[i] != b'\n' && buf[i] != b'/' { i += 1; }
        if i > start {
            let name = &buf[start..i];
            let name_str = unsafe { core::str::from_utf8_unchecked(name) };

            // Skip . and ..
            if name == b"." || name == b".." {
                i += 1;
                continue;
            }

            // Build src path: src/name
            let mut child_src = [0u8; 512];
            let mut pos = 0;
            for &b in src.as_bytes().iter() {
                if pos >= 500 { break; }
                child_src[pos] = b; pos += 1;
            }
            if pos > 0 && child_src[pos-1] != b'/' { child_src[pos] = b'/'; pos += 1; }
            for &b in name.iter() {
                if pos >= 500 { break; }
                child_src[pos] = b; pos += 1;
            }
            child_src[pos] = 0;

            // Build dst path: dst/name
            let mut child_dst = [0u8; 512];
            let mut pos = 0;
            for &b in dst.as_bytes().iter() {
                if pos >= 500 { break; }
                child_dst[pos] = b; pos += 1;
            }
            if pos > 0 && child_dst[pos-1] != b'/' { child_dst[pos] = b'/'; pos += 1; }
            for &b in name.iter() {
                if pos >= 500 { break; }
                child_dst[pos] = b; pos += 1;
            }
            child_dst[pos] = 0;

            let cs = unsafe { core::str::from_utf8_unchecked(&child_src[..pos]) };
            let cd = unsafe { core::str::from_utf8_unchecked(&child_dst[..pos]) };

            if is_dir(cs) {
                cp_recursive(cs, cd);
            } else {
                cp_file(cs, cd);
            }
        }
        i += 1;
    }
    0
}

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) {
    if argc < 3 {
        println!("Usage: cp [-r] <src> <dst>");
        sys_exit();
    }

    let mut arg_idx = 1;
    let mut recursive = false;

    // Check for -r flag
    let first_arg = unsafe { arg_at(argv, 1) };
    if first_arg == "-r" || first_arg == "-R" || first_arg == "--recursive" {
        recursive = true;
        arg_idx = 2;
    }

    if (arg_idx + 1) as u32 >= argc {
        println!("Usage: cp [-r] <src> <dst>");
        sys_exit();
    }

    let src = unsafe { arg_at(argv, arg_idx as usize) };
    let dst = unsafe { arg_at(argv, (arg_idx + 1) as usize) };

    let result = if recursive || is_dir(src) {
        cp_recursive(src, dst)
    } else {
        cp_file(src, dst)
    };

    if result < 0 {
        println!("cp: failed to copy '{}' -> '{}'", src, dst);
    }
}
