#![no_std]
#![no_main]

include!("../src/rt.rs");

use core::str;

const MAX_CMD: usize = 512;
const MAX_ARGS: usize = 32;
const MAX_PATH: usize = 256;

fn read_line(buf: &mut [u8]) -> usize {
    let mut i = 0;
    loop {
        let c = sys_getchar();
        if c == b'\n' || c == b'\r' {
            if i > 0 {
                sys_write(b"\n");
            }
            buf[i] = 0;
            return i;
        }
        if c == b'\x08' || c == 0x7F {
            if i > 0 {
                i -= 1;
                sys_write(b"\x08 \x08");
            }
            continue;
        }
        if i < buf.len() - 1 {
            buf[i] = c;
            i += 1;
            sys_write(&[c]);
        }
    }
}

fn parse_line(line: &[u8], argv: &mut [*const u8]) -> usize {
    let mut argc = 0;
    let mut i = 0;
    while i < line.len() && line[i] == b' ' { i += 1; }

    while i < line.len() && argc < MAX_ARGS - 1 {
        let start = i;
        if line[i] == b'"' {
            i += 1;
            let qstart = i;
            while i < line.len() && line[i] != b'"' { i += 1; }
            if qstart < i {
                let mut arg = [0u8; 256];
                let len = i - qstart;
                if len < arg.len() {
                    arg[..len].copy_from_slice(&line[qstart..i]);
                    arg[len] = 0;
                    let ptr = unsafe { crate::malloc(len + 1) };
                    if !ptr.is_null() {
                        unsafe { core::ptr::copy_nonoverlapping(arg.as_ptr(), ptr, len + 1); }
                        argv[argc] = ptr;
                        argc += 1;
                    }
                }
            }
            if i < line.len() { i += 1; }
        } else {
            while i < line.len() && line[i] != b' ' { i += 1; }
            let mut arg = [0u8; 256];
            let len = i - start;
            if len < arg.len() {
                arg[..len].copy_from_slice(&line[start..i]);
                arg[len] = 0;
                let ptr = unsafe { crate::malloc(len + 1) };
                if !ptr.is_null() {
                    unsafe { core::ptr::copy_nonoverlapping(arg.as_ptr(), ptr, len + 1); }
                    argv[argc] = ptr;
                    argc += 1;
                }
            }
        }
        while i < line.len() && line[i] == b' ' { i += 1; }
    }
    argv[argc] = core::ptr::null();
    argc
}

fn str_cmp(a: &[u8], b: &str) -> bool {
    let b = b.as_bytes();
    if a.len() != b.len() { return false; }
    for i in 0..a.len() {
        if a[i] != b[i] { return false; }
    }
    true
}

fn path_join(dir: &[u8], name: &[u8], out: &mut [u8]) {
    let mut pos = 0;
    if dir.len() > 0 && out.len() > 0 {
        if dir[0] != b'/' {
            let cwd = sys_cwd();
            for &b in cwd.iter() {
                if pos >= out.len() - 1 { break; }
                out[pos] = b;
                pos += 1;
            }
            if pos > 0 && out[pos-1] != b'/' { out[pos] = b'/'; pos += 1; }
        }
        for &b in dir.iter() {
            if pos >= out.len() - 1 { break; }
            out[pos] = b;
            pos += 1;
        }
    }
    out[pos] = b'/'; pos += 1;
    for &b in name.iter() {
        if pos >= out.len() - 1 { break; }
        out[pos] = b;
        pos += 1;
    }
    if pos < out.len() { out[pos] = 0; }
}

fn sys_cwd() -> [u8; 256] {
    let mut buf = [0u8; 256];
    sys_getcwd(&mut buf);
    buf
}

// ──── BUILT-IN COMMANDS ─────────────────────────────────────────

fn builtin_help() {
    let help = b"Elitra OS Shell Commands:\n\
  help          - Show this help\n\
  clear         - Clear screen\n\
  echo <text>   - Print text\n\
  ls [path]     - List directory\n\
  cat <file>    - Print file\n\
  cd <path>     - Change directory\n\
  pwd           - Print working directory\n\
  mkdir <path>  - Create directory\n\
  touch <path>  - Create file\n\
  rm <path>     - Remove file\n\
  rmdir <path>  - Remove directory\n\
  mv <src> <dst>- Move/rename\n\
  cp <src> <dst>- Copy file\n\
  uname         - System info\n\
  uptime        - Show uptime\n\
  exec <file>   - Run program\n\
  reboot        - Reboot system\n\
  poweroff      - Power off\n\
  kill <pid>    - Send SIGTERM\n\
  ps            - List processes\n\
  tee <file>    - Copy stdin to file\n\
  free          - Memory info\n\
  lspci         - PCI devices\n\
  lsmem         - Memory map\n\
  top           - Process table\n\
  setenv k v    - Set environment\n\
  unsetenv k    - Unset environment\n\
  env           - Show environment\n\
  export k=v    - Set environment\n\
  Source: cmd1 && cmd2 (run if ok)\n\
  Source: cmd1 || cmd2 (run if fail)\n\
  Source: cmd1 ; cmd2 (run both)\n\
  Source: cmd > file  (output redirect)\n\
  Source: cmd < file  (input redirect)\n\
  Source: cmd1 | cmd2 (pipe)\n";
    sys_write(help);
}

fn builtin_clear() {
    sys_clear();
}

fn builtin_pwd() {
    let cwd = sys_cwd();
    let mut len = 0;
    while len < cwd.len() && cwd[len] != 0 { len += 1; }
    sys_write(&cwd[..len]);
    sys_write(b"\n");
}

fn builtin_echo(args: &[*const u8]) {
    let mut i = 1;
    while i < args.len() && !args[i].is_null() {
        let s = unsafe { cstr_to_slice(args[i]) };
        sys_write(s);
        i += 1;
        if i < args.len() && !args[i].is_null() {
            sys_write(b" ");
        }
    }
    sys_write(b"\n");
}

unsafe fn cstr_to_slice(ptr: *const u8) -> &'static [u8] {
    let mut len = 0;
    while *ptr.add(len) != 0 { len += 1; }
    core::slice::from_raw_parts(ptr, len)
}

fn builtin_ls(args: &[*const u8]) {
    let path = if !args[1].is_null() {
        unsafe { cstr_to_slice(args[1]) }
    } else {
        b"."
    };
    let mut buf = [0u8; 4096];
    let n = sys_readdir(
        unsafe { core::str::from_utf8_unchecked(path) },
        &mut buf
    );
    if n <= 0 {
        let msg = b"ls: failed\n";
        sys_write(msg);
        return;
    }
    let mut i = 0;
    while (i as isize) < n {
        let start = i;
        while (i as isize) < n && buf[i] != b'\n' && buf[i] != b'/' {
            i += 1;
        }
        if i > start {
            sys_write(&buf[start..i]);
            if (i as isize) < n && buf[i] == b'/' { sys_write(b"/"); }
            sys_write(b"  ");
        }
        i += 1;
    }
    sys_write(b"\n");
}

fn builtin_cat(args: &[*const u8]) {
    if args[1].is_null() {
        sys_write(b"Usage: cat <file>\n");
        return;
    }
    let path = unsafe { cstr_to_slice(args[1]) };
    let path_str = unsafe { core::str::from_utf8_unchecked(path) };
    let mut st = FileStat { type_: 0, size: 0, name: [0u8; 64], uid: 0, gid: 0, mode: 0 };
    if sys_stat(path_str, &mut st) < 0 {
        sys_write(b"cat: not found\n");
        return;
    }
    let fd = sys_open(path_str);
    if fd < 0 {
        sys_write(b"cat: open failed\n");
        return;
    }
    let mut buf = [0u8; 512];
    loop {
        let n = sys_read(fd, &mut buf);
        if n <= 0 { break; }
        sys_write(&buf[..n as usize]);
    }
    sys_close(fd);
    sys_write(b"\n");
}

fn builtin_cd(args: &[*const u8]) {
    let path = if !args[1].is_null() {
        unsafe { cstr_to_slice(args[1]) }
    } else {
        b"/"
    };
    let path_str = unsafe { core::str::from_utf8_unchecked(path) };
    if sys_chdir(path_str) < 0 {
        sys_write(b"cd: failed\n");
    }
}

fn builtin_touch(args: &[*const u8]) {
    if args[1].is_null() {
        sys_write(b"Usage: touch <path>\n");
        return;
    }
    let path = unsafe { core::str::from_utf8_unchecked(cstr_to_slice(args[1])) };
    if sys_write_file(path, b"") < 0 {
        sys_write(b"touch: failed\n");
    }
}

fn builtin_mkdir(args: &[*const u8]) {
    if args[1].is_null() {
        sys_write(b"Usage: mkdir <path>\n");
        return;
    }
    let path = unsafe { core::str::from_utf8_unchecked(cstr_to_slice(args[1])) };
    if sys_mkdir(path) < 0 {
        sys_write(b"mkdir: failed\n");
    }
}

fn builtin_rm(args: &[*const u8]) {
    if args[1].is_null() {
        sys_write(b"Usage: rm <path>\n");
        return;
    }
    let path = unsafe { core::str::from_utf8_unchecked(cstr_to_slice(args[1])) };
    if sys_unlink(path) < 0 {
        sys_write(b"rm: failed\n");
    }
}

fn builtin_rmdir(args: &[*const u8]) {
    if args[1].is_null() {
        sys_write(b"Usage: rmdir <path>\n");
        return;
    }
    let path = unsafe { core::str::from_utf8_unchecked(cstr_to_slice(args[1])) };
    if sys_rmdir(path) < 0 {
        sys_write(b"rmdir: failed\n");
    }
}

fn builtin_mv(args: &[*const u8]) {
    if args[1].is_null() || args[2].is_null() {
        sys_write(b"Usage: mv <src> <dst>\n");
        return;
    }
    let old = unsafe { core::str::from_utf8_unchecked(cstr_to_slice(args[1])) };
    let new = unsafe { core::str::from_utf8_unchecked(cstr_to_slice(args[2])) };
    if sys_rename(old, new) < 0 {
        sys_write(b"mv: failed\n");
    }
}

fn builtin_cp(args: &[*const u8]) {
    if args[1].is_null() || args[2].is_null() {
        sys_write(b"Usage: cp <src> <dst>\n");
        return;
    }
    let src = unsafe { core::str::from_utf8_unchecked(cstr_to_slice(args[1])) };
    let dst = unsafe { core::str::from_utf8_unchecked(cstr_to_slice(args[2])) };
    let fd = sys_open(src);
    if fd < 0 { sys_write(b"cp: source not found\n"); return; }
    let mut st = FileStat { type_: 0, size: 0, name: [0u8; 64], uid: 0, gid: 0, mode: 0 };
    if sys_stat(src, &mut st) < 0 { sys_close(fd); return; }
    let mut buf = [0u8; 4096];
    let mut total = 0u32;
    loop {
        let n = sys_read(fd, &mut buf);
        if n <= 0 { break; }
        total += n as u32;
    }
    sys_close(fd);
    /* Re-read the file content */
    let fd = sys_open(src);
    if fd < 0 { return; }
    let mut content = [0u8; 4096];
    let mut pos = 0;
    loop {
        if pos >= content.len() { break; }
        let n = sys_read(fd, &mut content[pos..]);
        if n <= 0 { break; }
        pos += n as usize;
    }
    sys_close(fd);
    sys_write_file(dst, &content[..pos]);
}

fn builtin_uname() {
    let mut buf = [0u8; 325];
    let ticks = sys_system_info(&mut buf);
    let mut len = 0;
    while len < buf.len() && buf[len] != 0 { len += 1; }
    if len > 0 { sys_write(&buf[..len]); sys_write(b"\n"); }
    sys_write(b"Uptime: ");
    let secs = ticks / 100;
    let mins = secs / 60;
    let hrs = mins / 60;
    let s = secs % 60;
    let m = mins % 60;
    let h = hrs % 60;
    let mut nbuf = [0u8; 20];
    let mut pos = 0;
    if h >= 10 { nbuf[pos] = b'0' + (h / 10) as u8; pos += 1; }
    nbuf[pos] = b'0' + (h % 10) as u8; pos += 1;
    nbuf[pos] = b':'; pos += 1;
    if m >= 10 { nbuf[pos] = b'0' + (m / 10) as u8; pos += 1; }
    nbuf[pos] = b'0' + (m % 10) as u8; pos += 1;
    nbuf[pos] = b':'; pos += 1;
    if s >= 10 { nbuf[pos] = b'0' + (s / 10) as u8; pos += 1; }
    nbuf[pos] = b'0' + (s % 10) as u8; pos += 1;
    sys_write(&nbuf[..pos]);
    sys_write(b"\n");
}

fn builtin_reboot() {
    sys_reboot();
}

fn builtin_poweroff() {
    sys_poweroff();
}

fn builtin_kill(args: &[*const u8]) {
    if args[1].is_null() {
        sys_write(b"Usage: kill <pid>\n");
        return;
    }
    let s = unsafe { cstr_to_slice(args[1]) };
    let mut pid: isize = 0;
    for &b in s.iter() {
        if b < b'0' || b > b'9' { sys_write(b"kill: invalid pid\n"); return; }
        pid = pid * 10 + (b - b'0') as isize;
    }
    if sys_kill(pid, 15) < 0 {
        sys_write(b"kill: failed\n");
    }
}

fn builtin_ps() {
    let mut buf = [0u8; 4096];
    let n = sys_ps(&mut buf);
    if n > 0 {
        sys_write(&buf[..n as usize]);
    } else {
        sys_write(b"ps: failed\n");
    }
}

fn builtin_tee(args: &[*const u8]) {
    if args[1].is_null() {
        let mut buf = [0u8; 256];
        loop {
            let n = sys_read(0, &mut buf);
            if n <= 0 { break; }
            sys_write(&buf[..n as usize]);
        }
        return;
    }
    let path = unsafe { core::str::from_utf8_unchecked(cstr_to_slice(args[1])) };
    let mut buf = [0u8; 256];
    let mut accum = [0u8; 4096];
    let mut total = 0usize;
    loop {
        let n = sys_read(0, &mut buf);
        if n <= 0 { break; }
        if total + n as usize <= accum.len() {
            accum[total..total + n as usize].copy_from_slice(&buf[..n as usize]);
            total += n as usize;
        }
        sys_write(&buf[..n as usize]);
    }
    if total > 0 {
        sys_write_file(path, &accum[..total]);
    }
}

fn builtin_env() {
    let mut buf = [0u8; 4096];
    let n = sys_list_env(&mut buf);
    if n > 0 {
        sys_write(&buf[..n as usize]);
    } else {
        sys_write(b"HOME=/\nPATH=/bin:/mnt/bin\nUSER=root\n");
    }
}

fn builtin_setenv(args: &[*const u8]) {
    if args[1].is_null() || args[2].is_null() {
        sys_write(b"Usage: setenv <key> <value>\n");
        return;
    }
    let key = unsafe { core::str::from_utf8_unchecked(cstr_to_slice(args[1])) };
    let val = unsafe { core::str::from_utf8_unchecked(cstr_to_slice(args[2])) };
    if sys_setenv(key, val) < 0 {
        sys_write(b"setenv: failed\n");
    }
}

fn builtin_unsetenv(args: &[*const u8]) {
    if args[1].is_null() {
        sys_write(b"Usage: unsetenv <key>\n");
        return;
    }
    let key = unsafe { core::str::from_utf8_unchecked(cstr_to_slice(args[1])) };
    if sys_setenv(key, "") < 0 {
        sys_write(b"unsetenv: failed\n");
    }
}

fn builtin_export(args: &[*const u8]) {
    if args[1].is_null() {
        sys_write(b"Usage: export KEY=VALUE\n");
        return;
    }
    let arg = unsafe { core::str::from_utf8_unchecked(cstr_to_slice(args[1])) };
    let bytes = arg.as_bytes();
    let mut eq_pos = bytes.len();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'=' { eq_pos = i; break; }
    }
    if eq_pos == bytes.len() {
        sys_write(b"export: syntax: KEY=VALUE\n");
        return;
    }
    let key = &arg[..eq_pos];
    let val = &arg[eq_pos + 1..];
    if sys_setenv(key, val) < 0 {
        sys_write(b"export: failed\n");
    }
}

fn builtin_free() {
    let mut total: u32 = 0;
    let mut free: u32 = 0;
    if sys_free_info(&mut total, &mut free) < 0 {
        sys_write(b"free: failed\n");
        return;
    }
    let used = total - free;
    sys_write(b"MemTotal:     ");
    print_u32(total);
    sys_write(b" kB\nMemFree:      ");
    print_u32(free);
    sys_write(b" kB\nMemUsed:      ");
    print_u32(used);
    sys_write(b" kB\n");
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

fn builtin_lspci() {
    let path_str = "/proc/pci";
    let fd = sys_open(path_str);
    if fd < 0 {
        sys_write(b"lspci: /proc/pci not available\n");
        return;
    }
    let mut buf = [0u8; 4096];
    loop {
        let n = sys_read(fd, &mut buf);
        if n <= 0 { break; }
        sys_write(&buf[..n as usize]);
    }
    sys_close(fd);
}

fn builtin_dmesg() {
    let path_str = "/dev/kmsg";
    let fd = sys_open(path_str);
    if fd < 0 {
        sys_write(b"dmesg: /dev/kmsg not available\n");
        return;
    }
    let mut buf = [0u8; 4096];
    loop {
        let n = sys_read(fd, &mut buf);
        if n <= 0 { break; }
        sys_write(&buf[..n as usize]);
    }
    sys_close(fd);
}

fn builtin_top() {
    let mut iteration = 0u32;
    loop {
        if iteration > 0 {
            sys_sleep(2000);
        }
        iteration += 1;
        sys_clear();
        let mut buf = [0u8; 4096];
        let n = sys_ps(&mut buf);
        if n > 0 {
            sys_write(&buf[..n as usize]);
        }
        let mut total: u32 = 0;
        let mut free: u32 = 0;
        if sys_free_info(&mut total, &mut free) >= 0 {
            sys_write(b"\nMem: ");
            print_u32(total - free);
            sys_write(b" / ");
            print_u32(total);
            sys_write(b" kB\n");
        }
        // Check for 'q' key to exit
        // Non-blocking: just check if there's input available
        let mut pfd = [PollFd { fd: 0, events: POLLIN, revents: 0 }];
        sys_poll(&mut pfd, 0);
        if pfd[0].revents & POLLIN != 0 {
            let c = sys_getchar();
            if c == b'q' || c == b'Q' { break; }
        }
    }
}

// ──── COMMAND EXECUTION ─────────────────────────────────────────

fn find_exec(path: &[u8]) -> bool {
    let path_str = unsafe { core::str::from_utf8_unchecked(path) };
    let mut st = FileStat { type_: 0, size: 0, name: [0u8; 64], uid: 0, gid: 0, mode: 0 };
    sys_stat(path_str, &mut st) >= 0
}

fn try_exec(cmd: &[u8], argv: &[*const u8]) -> bool {
    /* Try as given */
    if find_exec(cmd) {
        return true;
    }
    /* Try /bin/<cmd>.elf */
    let mut elf_path = [0u8; 256];
    elf_path[..5].copy_from_slice(b"/bin/");
    let mut pos = 5;
    for &b in cmd.iter() {
        if pos >= 250 { return false; }
        elf_path[pos] = b;
        pos += 1;
    }
    if pos + 4 > 256 { return false; }
    elf_path[pos] = b'.'; pos += 1;
    elf_path[pos] = b'e'; pos += 1;
    elf_path[pos] = b'l'; pos += 1;
    elf_path[pos] = b'f'; pos += 1;
    if pos < 256 { elf_path[pos] = 0; }
    find_exec(&elf_path)
}

fn run_command(cmd_line: &[u8]) {
    let mut args: [*const u8; MAX_ARGS] = [core::ptr::null(); MAX_ARGS];
    let argc = parse_line(cmd_line, &mut args);

    if argc == 0 { return; }

    /* Check for ; command chaining */
    for (i, &b) in cmd_line.iter().enumerate() {
        if b == b';' {
            let mut left = [0u8; MAX_CMD];
            let mut right = [0u8; MAX_CMD];
            let mut lpos = 0;
            for &x in cmd_line[..i].iter() {
                if lpos >= left.len() - 1 { break; }
                left[lpos] = x;
                lpos += 1;
            }
            left[lpos] = 0;
            while lpos > 0 && left[lpos-1] == b' ' { lpos -= 1; left[lpos] = 0; }

            let mut rpos = 0;
            for &x in cmd_line[i+1..].iter() {
                if rpos >= right.len() - 1 { break; }
                right[rpos] = x;
                rpos += 1;
            }
            right[rpos] = 0;
            while rpos > 0 && right[rpos-1] == b' ' { rpos -= 1; right[rpos] = 0; }

            if lpos > 0 { run_command(&left[..lpos]); }
            if rpos > 0 { run_command(&right[..rpos]); }
            return;
        }
    }

    /* Check for && command chaining */
    for (i, &b) in cmd_line.iter().enumerate() {
        if i + 1 < cmd_line.len() && b == b'&' && cmd_line[i+1] == b'&' {
            let mut left = [0u8; MAX_CMD];
            let mut right = [0u8; MAX_CMD];
            let mut lpos = 0;
            for &x in cmd_line[..i].iter() {
                if lpos >= left.len() - 1 { break; }
                left[lpos] = x;
                lpos += 1;
            }
            left[lpos] = 0;
            while lpos > 0 && left[lpos-1] == b' ' { lpos -= 1; left[lpos] = 0; }

            let mut rpos = 0;
            for &x in cmd_line[i+2..].iter() {
                if rpos >= right.len() - 1 { break; }
                right[rpos] = x;
                rpos += 1;
            }
            right[rpos] = 0;
            while rpos > 0 && right[rpos-1] == b' ' { rpos -= 1; right[rpos] = 0; }

            if lpos > 0 {
                let pid = sys_fork();
                if pid == 0 {
                    run_command(&left[..lpos]);
                    sys_exit();
                } else if pid > 0 {
                    let mut stat = 0i32;
                    sys_waitpid(pid, &mut stat);
                    if stat == 0 && rpos > 0 {
                        run_command(&right[..rpos]);
                    } else if stat != 0 {
                        return;
                    }
                }
            } else if rpos > 0 {
                run_command(&right[..rpos]);
            }
            return;
        }
    }

    let cmd = unsafe { cstr_to_slice(args[0]) };

    let mut output_redir: Option<[u8; 256]> = None;
    let mut input_redir: Option<[u8; 256]> = None;
    let mut append_redir: bool = false;
    let mut cmd_end = cmd_line.len();
    let mut pipe_idx = cmd_line.len();
    let mut i = 0;
    while i < cmd_line.len() {
        if cmd_line[i] == b'>' {
            if i + 1 < cmd_line.len() && cmd_line[i+1] == b'>' {
                append_redir = true;
                let mut path = [0u8; 256];
                let mut j = i + 2;
                while j < cmd_line.len() && cmd_line[j] == b' ' { j += 1; }
                let mut k = 0;
                while j < cmd_line.len() && cmd_line[j] != b' ' && cmd_line[j] != b';' && cmd_line[j] != b'|' && k < 255 {
                    path[k] = cmd_line[j]; k += 1; j += 1;
                }
                path[k] = 0;
                output_redir = Some(path);
                if cmd_end > i { cmd_end = i; }
                break;
            } else {
                let mut path = [0u8; 256];
                let mut j = i + 1;
                while j < cmd_line.len() && cmd_line[j] == b' ' { j += 1; }
                let mut k = 0;
                while j < cmd_line.len() && cmd_line[j] != b' ' && cmd_line[j] != b';' && cmd_line[j] != b'|' && k < 255 {
                    path[k] = cmd_line[j]; k += 1; j += 1;
                }
                path[k] = 0;
                output_redir = Some(path);
                if cmd_end > i { cmd_end = i; }
                break;
            }
        } else if cmd_line[i] == b'<' {
            let mut path = [0u8; 256];
            let mut j = i + 1;
            while j < cmd_line.len() && cmd_line[j] == b' ' { j += 1; }
            let mut k = 0;
            while j < cmd_line.len() && cmd_line[j] != b' ' && cmd_line[j] != b';' && cmd_line[j] != b'|' && k < 255 {
                path[k] = cmd_line[j]; k += 1; j += 1;
            }
            path[k] = 0;
            input_redir = Some(path);
            if cmd_end > i { cmd_end = i; }
            break;
        } else if cmd_line[i] == b'|' {
            if pipe_idx == cmd_line.len() {
                pipe_idx = i;
            }
        }
        i += 1;
    }

    let trimmed_cmd_line = &cmd_line[..cmd_end];

    if pipe_idx < trimmed_cmd_line.len() {
        let mut left = [0u8; MAX_CMD];
        let mut right = [0u8; MAX_CMD];
        let (l, r) = trimmed_cmd_line.split_at(pipe_idx);
        let r = &r[1..];

        let mut lpos = 0;
        for &b in l.iter() {
            if lpos >= left.len() - 1 { break; }
            left[lpos] = b;
            lpos += 1;
        }
        left[lpos] = 0;

        let mut rpos = 0;
        for &b in r.iter() {
            if rpos >= right.len() - 1 { break; }
            right[rpos] = b;
            rpos += 1;
        }
        right[rpos] = 0;

        while lpos > 0 && left[lpos-1] == b' ' { lpos -= 1; left[lpos] = 0; }
        let mut rstart = 0;
        while rstart < rpos && right[rstart] == b' ' { rstart += 1; }

        if lpos == 0 || rstart >= rpos {
            sys_write(b"Usage: cmd1 | cmd2\n");
            return;
        }

        let left_cmd = &left[..lpos];
        let right_cmd = &right[rstart..rpos];

        let mut fds = [0i32; 2];
        if sys_pipe_create(&mut fds) < 0 {
            sys_write(b"pipe: creation failed\n");
            return;
        }

        let l_argv: [*const u8; 2] = [left_cmd.as_ptr(), core::ptr::null()];
        let r_argv: [*const u8; 2] = [right_cmd.as_ptr(), core::ptr::null()];

        let left_pid = sys_fork();
        if left_pid < 0 {
            sys_write(b"pipe: fork failed\n");
            sys_pipe_close(fds[0]);
            sys_pipe_close(fds[1]);
            return;
        }

        if left_pid == 0 {
            sys_pipe_close(fds[0]);
            sys_dup2(fds[1], 1);
            sys_pipe_close(fds[1]);
            run_command(left_cmd);
            sys_exit();
        }

        if left_pid > 0 {
            sys_pipe_close(fds[1]);
            let mut status = 0i32;
            let right_pid = sys_fork();
            if right_pid == 0 {
                sys_dup2(fds[0], 0);
                sys_pipe_close(fds[0]);
                run_command(right_cmd);
                sys_exit();
            }
            sys_pipe_close(fds[0]);
            if left_pid > 0 { sys_waitpid(left_pid, &mut status); }
            if right_pid > 0 { sys_waitpid(right_pid, &mut status); }
        }
        return;
    }

    if !output_redir.is_none() || !input_redir.is_none() {
        let mut new_args: [*const u8; MAX_ARGS] = [core::ptr::null(); MAX_ARGS];
        let new_argc = parse_line(trimmed_cmd_line, &mut new_args);
        if new_argc == 0 { return; }
        let new_cmd = unsafe { cstr_to_slice(new_args[0]) };
        let path = if !output_redir.is_none() { output_redir.as_ref().unwrap() } else { input_redir.as_ref().unwrap() };
        let is_output = !output_redir.is_none();

        let path_str = unsafe { core::str::from_utf8_unchecked(path) };
        let fd = sys_open(path_str);
        if fd < 0 {
            sys_write(b"redir: cannot open\n");
            return;
        }
        let fd = fd as i32;
        let pid = sys_fork();
        if pid == 0 {
            if is_output {
                sys_dup2(fd, 1);
                sys_dup2(fd, 2);
            } else {
                sys_dup2(fd, 0);
            }
            if !try_exec(new_cmd, &new_args) {
                sys_write(b"init: command not found: ");
                let cmd_str = unsafe { core::str::from_utf8_unchecked(new_cmd) };
                sys_write(cmd_str.as_bytes());
                sys_write(b"\n");
            }
            sys_exit();
        } else if pid > 0 {
            let mut stat = 0i32;
            sys_waitpid(pid, &mut stat);
        }
        return;
    }

    /* Check for built-in commands */
    let cmd_str = cmd;
    if str_cmp(cmd_str, "help") || str_cmp(cmd_str, "?") {
        builtin_help();
        return;
    }
    if str_cmp(cmd_str, "clear") || str_cmp(cmd_str, "clr") {
        builtin_clear();
        return;
    }
    if str_cmp(cmd_str, "pwd") {
        builtin_pwd();
        return;
    }
    if str_cmp(cmd_str, "echo") || str_cmp(cmd_str, "say") {
        builtin_echo(&args);
        return;
    }
    if str_cmp(cmd_str, "cd") {
        builtin_cd(&args);
        return;
    }
    if str_cmp(cmd_str, "exit") || str_cmp(cmd_str, "quit") {
        sys_exit();
    }
    if str_cmp(cmd_str, "ls") || str_cmp(cmd_str, "list") {
        builtin_ls(&args);
        return;
    }
    if str_cmp(cmd_str, "cat") || str_cmp(cmd_str, "dump") {
        builtin_cat(&args);
        return;
    }
    if str_cmp(cmd_str, "touch") || str_cmp(cmd_str, "create") {
        builtin_touch(&args);
        return;
    }
    if str_cmp(cmd_str, "mkdir") || str_cmp(cmd_str, "md") {
        builtin_mkdir(&args);
        return;
    }
    if str_cmp(cmd_str, "rm") || str_cmp(cmd_str, "del") {
        builtin_rm(&args);
        return;
    }
    if str_cmp(cmd_str, "rmdir") {
        builtin_rmdir(&args);
        return;
    }
    if str_cmp(cmd_str, "mv") {
        builtin_mv(&args);
        return;
    }
    if str_cmp(cmd_str, "cp") {
        builtin_cp(&args);
        return;
    }
    if str_cmp(cmd_str, "uname") || str_cmp(cmd_str, "ver") {
        builtin_uname();
        return;
    }
    if str_cmp(cmd_str, "reboot") || str_cmp(cmd_str, "rst") {
        builtin_reboot();
        return;
    }
    if str_cmp(cmd_str, "poweroff") || str_cmp(cmd_str, "off") {
        builtin_poweroff();
        return;
    }
    if str_cmp(cmd_str, "kill") {
        builtin_kill(&args);
        return;
    }
    if str_cmp(cmd_str, "ps") || str_cmp(cmd_str, "jobs") {
        builtin_ps();
        return;
    }
    if str_cmp(cmd_str, "tee") {
        builtin_tee(&args);
        return;
    }
    if str_cmp(cmd_str, "env") {
        builtin_env();
        return;
    }
    if str_cmp(cmd_str, "setenv") {
        builtin_setenv(&args);
        return;
    }
    if str_cmp(cmd_str, "unsetenv") {
        builtin_unsetenv(&args);
        return;
    }
    if str_cmp(cmd_str, "export") {
        builtin_export(&args);
        return;
    }
    if str_cmp(cmd_str, "free") {
        builtin_free();
        return;
    }
    if str_cmp(cmd_str, "lspci") {
        builtin_lspci();
        return;
    }
    if str_cmp(cmd_str, "dmesg") {
        builtin_dmesg();
        return;
    }
    if str_cmp(cmd_str, "top") {
        builtin_top();
        return;
    }
    if str_cmp(cmd_str, "uptime") {
        builtin_uname();
        return;
    }

    /* Try to execute as external command */
    let mut argc = 0;
    while argc < MAX_ARGS && !args[argc].is_null() { argc += 1; }

    /* Build elf path variant - try /mnt/bin/<name>.elf first */
    let mut elf_path_buf = [0u8; MAX_PATH];
    elf_path_buf[..10].copy_from_slice(b"/mnt/bin/");
    let mut epos = 10;
    for &b in cmd_str.iter() {
        if epos >= MAX_PATH - 5 { break; }
        elf_path_buf[epos] = b;
        epos += 1;
    }
    elf_path_buf[epos] = b'.'; epos += 1;
    elf_path_buf[epos] = b'e'; epos += 1;
    elf_path_buf[epos] = b'l'; epos += 1;
    elf_path_buf[epos] = b'f'; epos += 1;
    if epos < MAX_PATH { elf_path_buf[epos] = 0; }

    /* Also try /bin/ as fallback */
    let mut elf_path_bin = [0u8; MAX_PATH];
    elf_path_bin[..5].copy_from_slice(b"/bin/");
    let mut bpos = 5;
    for &b in cmd_str.iter() {
        if bpos >= MAX_PATH - 5 { break; }
        elf_path_bin[bpos] = b;
        bpos += 1;
    }
    elf_path_bin[bpos] = b'.'; bpos += 1;
    elf_path_bin[bpos] = b'e'; bpos += 1;
    elf_path_bin[bpos] = b'l'; bpos += 1;
    elf_path_bin[bpos] = b'f'; bpos += 1;

    let elf_path = if epos < MAX_PATH { elf_path_buf[epos] = 0; epos } else { 0 };

    /* Check if we can exec the command directly or as .elf */
    let try_direct = find_exec(cmd_str);
    let try_mnt_elf = epos <= MAX_PATH && find_exec(&elf_path_buf[..epos]);
    let try_bin_elf = bpos <= MAX_PATH && find_exec(&elf_path_bin[..bpos]);

    if !try_direct && !try_mnt_elf && !try_bin_elf {
        sys_write(cmd_str);
        sys_write(b": command not found\n");
        return;
    }

    let fork_pid = sys_fork();
    if fork_pid < 0 {
        sys_write(b"fork failed\n");
        return;
    }

    if fork_pid == 0 {
        /* Child process */
        let path_str = if try_direct {
            unsafe { core::str::from_utf8_unchecked(cmd_str) }
        } else if try_mnt_elf {
            unsafe { core::str::from_utf8_unchecked(&elf_path_buf[..epos]) }
        } else {
            unsafe { core::str::from_utf8_unchecked(&elf_path_bin[..bpos]) }
        };

        /* Build argv for execve */
        let mut new_argv: [*const u8; MAX_ARGS] = [core::ptr::null(); MAX_ARGS];
        let mut new_argc = 0;
        let cmd_name = cmd_str;
        let name_ptr = unsafe { crate::malloc(cmd_name.len() + 1) };
        if !name_ptr.is_null() {
            unsafe {
                core::ptr::copy_nonoverlapping(cmd_name.as_ptr(), name_ptr, cmd_name.len());
                *name_ptr.add(cmd_name.len()) = 0;
            }
            new_argv[0] = name_ptr;
            new_argc = 1;
        }
        for i in 1..argc {
            new_argv[new_argc] = args[i];
            new_argc += 1;
        }

        let result = sys_execve(path_str, &new_argv[..new_argc + 1]);
        if result < 0 {
            sys_write(b"exec failed\n");
        }
        sys_exit();
    }

    /* Parent: wait for child */
    let mut status = 0i32;
    sys_waitpid(fork_pid, &mut status);
}

// ──── MAIN ──────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn rust_main(_argc: u32, _argv: *const *const u8) {
    let banner = b"\n  ===== Elitra OS v0.2.0 =====\n  Type 'help' for commands\n\n";
    sys_write(banner);

    let mut line = [0u8; MAX_CMD];

    loop {
        /* Print prompt */
        let cwd = sys_cwd();
        let mut clen = 0;
        while clen < cwd.len() && cwd[clen] != 0 { clen += 1; }
        sys_write(b"elitra:");
        if clen > 0 { sys_write(&cwd[..clen]); }
        sys_write(b"$ ");

        let n = read_line(&mut line);
        if n == 0 { continue; }

        run_command(&line[..n]);
    }
}
