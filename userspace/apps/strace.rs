#![no_std]
#![no_main]

include!("../src/rt.rs");

// syscall name lookup (index = syscall number)
fn syscall_name(n: u32) -> &'static [u8] {
    match n {
        0 => b"write",
        1 => b"exit",
        2 => b"sleep",
        3 => b"yield",
        4 => b"open",
        5 => b"read",
        6 => b"close",
        7 => b"readdir",
        8 => b"write_file",
        9 => b"mkdir",
        10 => b"unlink",
        11 => b"rmdir",
        12 => b"rename",
        13 => b"stat",
        15 => b"pipe_create",
        16 => b"pipe_read",
        17 => b"pipe_write",
        18 => b"pipe_close",
        19 => b"write_fd",
        20 => b"clear",
        21 => b"system_info",
        22 => b"reboot",
        23 => b"poweroff",
        24 => b"open_write",
        25 => b"getcwd",
        26 => b"chdir",
        27 => b"gettime",
        28 => b"fork",
        29 => b"execve",
        30 => b"waitpid",
        31 => b"getpid",
        32 => b"kill",
        33 => b"sigaction",
        34 => b"sigreturn",
        35 => b"getppid",
        36 => b"mmap",
        37 => b"munmap",
        38 => b"brk",
        40 => b"lseek",
        41 => b"dup",
        42 => b"fcntl",
        45 => b"select",
        55 => b"socket",
        56 => b"connect",
        57 => b"sendto",
        58 => b"recvfrom",
        59 => b"bind",
        60 => b"close_socket",
        61 => b"dup2",
        62 => b"listen",
        63 => b"accept",
        64 => b"ioctl",
        65 => b"poll",
        66 => b"clock_gettime",
        67 => b"nanosleep",
        68 => b"getuid",
        69 => b"setuid",
        70 => b"getgid",
        71 => b"setgid",
        72 => b"geteuid",
        73 => b"getegid",
        74 => b"fchmod",
        75 => b"fchown",
        76 => b"chmod",
        77 => b"chown",
        78 => b"getdents",
        79 => b"symlink",
        80 => b"readlink",
        82 => b"setenv",
        83 => b"getenv",
        84 => b"ps",
        85 => b"free_info",
        86 => b"list_env",
        _ => b"???",
    }
}

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) {
    if argc < 2 {
        sys_write(b"strace: strace <command> [args...]\n");
        sys_write(b"  Traces syscall execution of a command\n");
        sys_exit_code(1);
    }

    let pid = sys_fork();
    if pid == 0 {
        // Child: execute the command (we can't actually trace yet,
        // so just run it normally)
        let mut child_argv: [*const u8; 16] = [core::ptr::null(); 16];
        let mut ci = 0;
        while (ci as u32) + 1 < argc && ci < 15 {
            let ptr = unsafe { *argv.add(ci + 1) };
            if ptr.is_null() { break; }
            child_argv[ci] = ptr;
            ci += 1;
        }
        child_argv[ci] = core::ptr::null();
        let cmd = unsafe { arg_at(argv, 1) };
        sys_execve(cmd, &child_argv);
        sys_exit_code(1);
    } else if pid > 0 {
        sys_write(b"strace: pid ");
        print_isize(pid);
        sys_write(b"\n");
        sys_write(b"strace: (syscall tracing requires ptrace -- not yet implemented)\n");
        sys_write(b"strace: running command without tracing...\n");
        let mut status = 0i32;
        sys_waitpid(pid, &mut status);
        sys_write(b"strace: child exited with status ");
        print_isize(status as isize);
        sys_write(b"\n");
    } else {
        sys_write(b"strace: fork failed\n");
        sys_exit_code(1);
    }
}

fn print_isize(val: isize) {
    if val < 0 {
        sys_write(b"-");
        print_u32((-val) as u32);
    } else {
        print_u32(val as u32);
    }
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
