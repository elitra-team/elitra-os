#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) {
    if argc < 2 {
        sys_write(b"xargs: xargs <command> [args...]\n");
        sys_write(b"  Reads lines from stdin and executes command for each\n");
        sys_exit_code(1);
    }

    let mut line_buf = [0u8; 1024];
    let mut line_len = 0;
    let mut stdin_buf = [0u8; 512];

    loop {
        let n = sys_read(0, &mut stdin_buf);
        if n <= 0 { break; }

        for i in 0..n as usize {
            if stdin_buf[i] == b'\n' {
                if line_len > 0 {
                    line_buf[line_len] = 0;
                    // Fork and exec the command with this line as last arg
                    let pid = sys_fork();
                    if pid == 0 {
                        // Child: build argv array
                        let mut child_argv: [*const u8; 16] = [core::ptr::null(); 16];
                        let mut ci = 0;
                        // Copy original argv (skip "xargs")
                        while ci + 1 < argc as usize && ci < 14 {
                            let ptr = unsafe { *argv.add(ci + 1) };
                            if ptr.is_null() { break; }
                            child_argv[ci] = ptr;
                            ci += 1;
                        }
                        // Append the line
                        child_argv[ci] = line_buf.as_ptr();
                        ci += 1;
                        child_argv[ci] = core::ptr::null();

                        let cmd = unsafe { arg_at(argv, 1) };
                        sys_execve(cmd, &child_argv);
                        sys_exit_code(1);
                    } else if pid > 0 {
                        let mut status = 0i32;
                        sys_waitpid(pid, &mut status);
                    }
                    line_len = 0;
                }
            } else if line_len < 1023 {
                line_buf[line_len] = stdin_buf[i];
                line_len += 1;
            }
        }
    }

    // Handle last line without newline
    if line_len > 0 {
        line_buf[line_len] = 0;
        let pid = sys_fork();
        if pid == 0 {
            let mut child_argv: [*const u8; 16] = [core::ptr::null(); 16];
            let mut ci = 0;
            while ci + 1 < argc as usize && ci < 14 {
                let ptr = unsafe { *argv.add(ci + 1) };
                if ptr.is_null() { break; }
                child_argv[ci] = ptr;
                ci += 1;
            }
            child_argv[ci] = line_buf.as_ptr();
            ci += 1;
            child_argv[ci] = core::ptr::null();
            let cmd = unsafe { arg_at(argv, 1) };
            sys_execve(cmd, &child_argv);
            sys_exit_code(1);
        } else if pid > 0 {
            let mut status = 0i32;
            sys_waitpid(pid, &mut status);
        }
    }
}
