#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) {
    // test / [ ] — conditional evaluation
    if argc < 2 {
        sys_exit_code(1);
    }

    let op = unsafe { arg_at(argv, 1) };

    // test -e <file>
    if op == "-e" || op == "-f" || op == "-r" {
        if argc < 4 {
            sys_exit_code(1);
        }
        let path = unsafe { arg_at(argv, 2) };
        let mut st = FileStat { type_: 0, size: 0, name: [0u8; 64], uid: 0, gid: 0, mode: 0 };
        if sys_stat(path, &mut st) >= 0 {
            sys_exit_code(0);
        } else {
            sys_exit_code(1);
        }
    }
    // test -d <file>
    else if op == "-d" {
        if argc < 4 { sys_exit_code(1); }
        let path = unsafe { arg_at(argv, 2) };
        let mut st = FileStat { type_: 0, size: 0, name: [0u8; 64], uid: 0, gid: 0, mode: 0 };
        if sys_stat(path, &mut st) >= 0 && st.type_ == 2 {
            sys_exit_code(0);
        } else {
            sys_exit_code(1);
        }
    }
    // test -z <string>
    else if op == "-z" {
        if argc < 4 {
            sys_exit_code(0);
        }
        let s = unsafe { arg_at(argv, 2) };
        if s.is_empty() { sys_exit_code(0); } else { sys_exit_code(1); }
    }
    // test -n <string>
    else if op == "-n" {
        if argc < 4 {
            sys_exit_code(1);
        }
        let s = unsafe { arg_at(argv, 2) };
        if !s.is_empty() { sys_exit_code(0); } else { sys_exit_code(1); }
    }
    // test <s1> = <s2>
    else if argc >= 4 {
        let eq = unsafe { arg_at(argv, 2) };
        let right = unsafe { arg_at(argv, 3) };
        if eq == "=" {
            if op == right { sys_exit_code(0); } else { sys_exit_code(1); }
        } else if eq == "!=" {
            if op != right { sys_exit_code(0); } else { sys_exit_code(1); }
        } else {
            sys_exit_code(1);
        }
    } else {
        sys_exit_code(1);
    }
}
