#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) {
    let mut path_buf = [0u8; 256];
    let mut path_len = 0;

    if argc > 1 {
        let p = unsafe { *argv.add(1) };
        if !p.is_null() {
            path_len = unsafe { strlen(p) };
            if path_len > 255 { path_len = 255; }
            unsafe { core::ptr::copy_nonoverlapping(p, path_buf.as_mut_ptr(), path_len); }
            path_buf[path_len] = 0;
        }
    } else {
        path_buf[0] = b'.'; path_buf[1] = 0; path_len = 1;
    }

    let blocks = du_recursive(&path_buf[..path_len + 1]);
    print_u32(blocks);
    sys_write(b"\t");
    print_path(&path_buf[..path_len]);
    sys_write(b"\n");
}

fn du_recursive(path: &[u8]) -> u32 {
    let mut total_blocks = 0u32;

    // Try to stat the path first
    let mut st = FileStat { type_: 0, size: 0, name: [0u8; 64], uid: 0, gid: 0, mode: 0 };

    // Build a proper string from the byte slice (exclude trailing null)
    let path_str = unsafe { core::str::from_utf8_unchecked(&path[..path.len().saturating_sub(1)]) };

    if sys_stat(path_str, &mut st) < 0 {
        return 0;
    }

    let size_blocks = (st.size + 511) / 512;
    total_blocks += size_blocks;

    // If it's a directory, recurse
    if st.type_ == 2 {
        let mut dir_buf = [0u8; 2048];
        let n = sys_readdir(path_str, &mut dir_buf);
        if n > 0 {
            let mut pos = 0;
            while pos < n as usize {
                // Each entry is name_len(2) + type(1) + data
                if pos + 3 > n as usize { break; }
                let name_len = dir_buf[pos] as usize | ((dir_buf[pos + 1] as usize) << 8);
                if name_len == 0 || pos + 3 + name_len > n as usize { break; }
                let entry_type = dir_buf[pos + 2];
                let name = &dir_buf[pos + 3..pos + 3 + name_len];

                // Skip . and ..
                if name_len == 1 && name[0] == b'.' { pos += 3 + name_len; continue; }
                if name_len == 2 && name[0] == b'.' && name[1] == b'.' { pos += 3 + name_len; continue; }

                if entry_type == 1 {
                    // Build full path: path/name\0
                    let mut child = [0u8; 256];
                    let mut ci = 0;
                    // Copy parent (without trailing null)
                    for &b in path.iter() {
                        if b == 0 { break; }
                        if ci < 255 { child[ci] = b; ci += 1; }
                    }
                    if ci < 255 { child[ci] = b'/'; ci += 1; }
                    for &b in name.iter() {
                        if ci < 255 { child[ci] = b; ci += 1; }
                    }
                    child[ci] = 0;
                    total_blocks += du_recursive(&child[..ci + 1]);
                }

                pos += 3 + name_len;
            }
        }
    }

    total_blocks
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

fn print_path(path: &[u8]) {
    let mut i = 0;
    while i < path.len() && path[i] != 0 {
        sys_write(&[path[i]]);
        i += 1;
    }
}
