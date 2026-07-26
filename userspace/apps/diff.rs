#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) {
    if argc < 3 {
        sys_write(b"diff: diff <file1> <file2>\n");
        sys_exit_code(1);
    }

    let path1 = unsafe { arg_at(argv, 1) };
    let path2 = unsafe { arg_at(argv, 2) };

    let fd1 = sys_open(path1);
    if fd1 < 0 {
        sys_write(b"diff: cannot open ");
        sys_write(path1.as_bytes());
        sys_write(b"\n");
        sys_exit_code(1);
    }

    let fd2 = sys_open(path2);
    if fd2 < 0 {
        sys_write(b"diff: cannot open ");
        sys_write(path2.as_bytes());
        sys_write(b"\n");
        sys_close(fd1);
        sys_exit_code(1);
    }

    let mut buf1 = [0u8; 512];
    let mut buf2 = [0u8; 512];
    let mut line: u32 = 1;
    let mut diff_found = false;

    loop {
        let n1 = sys_read(fd1, &mut buf1);
        let n2 = sys_read(fd2, &mut buf2);

        if n1 <= 0 && n2 <= 0 { break; }
        if n1 != n2 {
            diff_found = true;
            print_u32(line);
            sys_write(b": files differ in length\n");
            break;
        }

        for i in 0..n1 as usize {
            if buf1[i] == b'\n' { line += 1; }
            if buf1[i] != buf2[i] {
                diff_found = true;
                sys_write(b"< ");
                sys_write(&buf1[i..core::cmp::min(i + 40, n1 as usize)]);
                if n1 as usize > i + 40 { sys_write(b"..."); }
                sys_write(b"\n> ");
                sys_write(&buf2[i..core::cmp::min(i + 40, n2 as usize)]);
                if n2 as usize > i + 40 { sys_write(b"..."); }
                sys_write(b"\n");
                break;
            }
        }
        if diff_found { break; }
    }

    sys_close(fd1);
    sys_close(fd2);

    if diff_found { sys_exit_code(1); }
    sys_exit_code(0);
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
