use crate::scheduler::{TaskState, VNode};

pub fn init_procfs() {
    unsafe {
        crate::vfs::krust_vfs_create_dir(b"/proc\0".as_ptr());
        create_proc_dev(b"/proc/cpuinfo\0".as_ptr(), read_cpuinfo);
        create_proc_dev(b"/proc/meminfo\0".as_ptr(), read_meminfo);
        create_proc_dev(b"/proc/uptime\0".as_ptr(), read_uptime);
        create_proc_dev(b"/proc/version\0".as_ptr(), read_version);
        create_proc_dev(b"/proc/stat\0".as_ptr(), read_stat);
        create_proc_dev(b"/proc/pci\0".as_ptr(), read_pci);
    }
}

unsafe fn create_proc_dev(path: *const u8, read_fn: extern "C" fn(*mut VNode, *mut u8, u32, u32) -> i32) {
    crate::vfs::krust_vfs_create_device(path, Some(read_fn), None);
}

extern "C" fn read_cpuinfo(_node: *mut VNode, buf: *mut u8, size: u32, _offset: u32) -> i32 {
    unsafe {
        let mut out = [0u8; 512];
        let mut pos = 0;

        append_str(&mut out, &mut pos, b"processor\t: 0\n\0");

        let vendor = crate::cpuid::vendor();
        append_str(&mut out, &mut pos, b"vendor_id\t: \0");
        let mut vi = 0;
        while vi < 12 && vendor[vi] != 0 {
            if pos < out.len() { out[pos] = vendor[vi]; pos += 1; }
            vi += 1;
        }
        append_str(&mut out, &mut pos, b"\n\0");

        let brand = crate::cpuid::brand();
        append_str(&mut out, &mut pos, b"model name\t: \0");
        let mut bi = 0;
        while bi < 48 && brand[bi] != 0 {
            if pos < out.len() { out[pos] = brand[bi]; pos += 1; }
            bi += 1;
        }
        append_str(&mut out, &mut pos, b"\n\0");

        append_str(&mut out, &mut pos, b"flags\t\t: \0");
        if crate::cpuid::has_sse() { append_str(&mut out, &mut pos, b"sse \0"); }
        if crate::cpuid::has_sse2() { append_str(&mut out, &mut pos, b"sse2 \0"); }
        if crate::cpuid::has_sse3() { append_str(&mut out, &mut pos, b"sse3 \0"); }
        if crate::cpuid::has_ssse3() { append_str(&mut out, &mut pos, b"ssse3 \0"); }
        if crate::cpuid::has_sse41() { append_str(&mut out, &mut pos, b"sse4.1 \0"); }
        if crate::cpuid::has_sse42() { append_str(&mut out, &mut pos, b"sse4.2 \0"); }
        if crate::cpuid::has_avx() { append_str(&mut out, &mut pos, b"avx \0"); }
        if crate::cpuid::has_avx2() { append_str(&mut out, &mut pos, b"avx2 \0"); }
        if crate::cpuid::has_rdrand() { append_str(&mut out, &mut pos, b"rdrand \0"); }
        if crate::cpuid::has_xsave() { append_str(&mut out, &mut pos, b"xsave \0"); }
        if crate::cpuid::has_fxsave() { append_str(&mut out, &mut pos, b"fxsave \0"); }
        append_str(&mut out, &mut pos, b"\n\0");

        let len = core::cmp::min(pos, size as usize);
        let mut i = 0;
        while i < len {
            core::ptr::write_volatile(buf.add(i), out[i]);
            i += 1;
        }
        len as i32
    }
}

extern "C" fn read_meminfo(_node: *mut VNode, buf: *mut u8, size: u32, _offset: u32) -> i32 {
    unsafe {
        let mut total_kb: u32 = 0;
        let mut free_kb: u32 = 0;
        crate::mm::mm::krust_mm_info(&mut total_kb, &mut free_kb);
        let mut out = [0u8; 256];
        let mut pos = 0;
        append_str(&mut out, &mut pos, b"MemTotal:     \0");
        append_u32(&mut out, &mut pos, total_kb);
        append_str(&mut out, &mut pos, b" kB\nMemFree:     \0");
        append_u32(&mut out, &mut pos, free_kb);
        append_str(&mut out, &mut pos, b" kB\nMemAvailable:\0");
        append_u32(&mut out, &mut pos, free_kb);
        append_str(&mut out, &mut pos, b" kB\n\0");
        let len = core::cmp::min(pos, size as usize);
        let mut i = 0;
        while i < len {
            core::ptr::write_volatile(buf.add(i), out[i]);
            i += 1;
        }
        len as i32
    }
}

extern "C" fn read_uptime(_node: *mut VNode, buf: *mut u8, size: u32, _offset: u32) -> i32 {
    unsafe {
        let ticks = crate::pittimer::krust_pittimer_get_ticks();
        let secs = ticks / 100;
        let mut out = [0u8; 32];
        let mut pos = 0;
        append_u32(&mut out, &mut pos, secs);
        append_str(&mut out, &mut pos, b".00\n\0");
        let len = core::cmp::min(pos, size as usize);
        let mut i = 0;
        while i < len {
            core::ptr::write_volatile(buf.add(i), out[i]);
            i += 1;
        }
        len as i32
    }
}

extern "C" fn read_version(_node: *mut VNode, buf: *mut u8, size: u32, _offset: u32) -> i32 {
    let mut out = [0u8; 64];
    let mut pos = 0;
    append_str(&mut out, &mut pos, crate::KERNEL_NAME.as_bytes());
    append_str(&mut out, &mut pos, b" v\0");
    append_str(&mut out, &mut pos, crate::KERNEL_VERSION.as_bytes());
    append_str(&mut out, &mut pos, b" \0");
    append_str(&mut out, &mut pos, crate::KERNEL_ARCH.as_bytes());
    append_str(&mut out, &mut pos, b"\n\0");
    let len = core::cmp::min(pos, size as usize);
    unsafe {
        let mut i = 0;
        while i < len {
            core::ptr::write_volatile(buf.add(i), out[i]);
            i += 1;
        }
    }
    len as i32
}

extern "C" fn read_stat(_node: *mut VNode, buf: *mut u8, size: u32, _offset: u32) -> i32 {
    unsafe {
        let next_id = crate::scheduler::krust_sched_get_next_id();
        let mut running = 0u32;
        let mut sleeping = 0u32;
        let mut zombie = 0u32;
        for i in 0..next_id {
            let t = crate::scheduler::krust_sched_get_task(i);
            if t.is_null() { continue; }
            match (*t).state {
                TaskState::RUNNING | TaskState::READY => running += 1,
                TaskState::BLOCKED | TaskState::WAITING => sleeping += 1,
                TaskState::EXITED => zombie += 1,
            }
        }
        let mut out = [0u8; 128];
        let mut pos = 0;
        append_str(&mut out, &mut pos, b"procs_running \0");
        append_u32(&mut out, &mut pos, running);
        append_str(&mut out, &mut pos, b"\nprocs_sleeping \0");
        append_u32(&mut out, &mut pos, sleeping);
        append_str(&mut out, &mut pos, b"\nprocs_zombie \0");
        append_u32(&mut out, &mut pos, zombie);
        append_str(&mut out, &mut pos, b"\n\0");
        let len = core::cmp::min(pos, size as usize);
        let mut i = 0;
        while i < len {
            core::ptr::write_volatile(buf.add(i), out[i]);
            i += 1;
        }
        len as i32
    }
}

extern "C" fn read_pci(_node: *mut VNode, buf: *mut u8, size: u32, _offset: u32) -> i32 {
    unsafe {
        let mut out = [0u8; 4096];
        let mut pos = 0;
        append_str(&mut out, &mut pos, b"Bus  Dev  Func  Vendor  Device  Class\n\0");

        for bus in 0..=255u8 {
            for dev in 0..32u8 {
                for func in 0..8u8 {
                    let vendor = crate::pci::PCI::config_read_word(bus, dev, func, 0x00);
                    if vendor == 0xFFFF { continue; }
                    let device = crate::pci::PCI::config_read_word(bus, dev, func, 0x02);
                    let class_reg = crate::pci::PCI::config_read_dword(bus, dev, func, 0x08);
                    let class = (class_reg >> 24) as u8;
                    let subclass = (class_reg >> 16) as u8;

                    append_u32_hex(&mut out, &mut pos, bus as u32);
                    append_str(&mut out, &mut pos, b"  \0");
                    append_u32_hex(&mut out, &mut pos, dev as u32);
                    append_str(&mut out, &mut pos, b"  \0");
                    append_u32_hex(&mut out, &mut pos, func as u32);
                    append_str(&mut out, &mut pos, b"     \0");
                    append_u32_hex(&mut out, &mut pos, vendor as u32);
                    append_str(&mut out, &mut pos, b"  \0");
                    append_u32_hex(&mut out, &mut pos, device as u32);
                    append_str(&mut out, &mut pos, b"  \0");
                    append_u32_hex(&mut out, &mut pos, ((class as u32) << 8) | (subclass as u32));
                    append_str(&mut out, &mut pos, b"\n\0");
                }
            }
        }
        let len = core::cmp::min(pos, size as usize);
        let mut i = 0;
        while i < len {
            core::ptr::write_volatile(buf.add(i), out[i]);
            i += 1;
        }
        len as i32
    }
}

pub fn proc_list_processes(buf: &mut [u8]) -> usize {
    unsafe {
        let mut pos = 0;
        let header = b"  PID  PPID STATE  CMD\n\0";
        let mut i = 0;
        while i < header.len() && pos < buf.len() {
            buf[pos] = header[i];
            pos += 1;
            i += 1;
        }
        let next_id = crate::scheduler::krust_sched_get_next_id();
        for pid in 0..next_id {
            let task = crate::scheduler::krust_sched_get_task(pid);
            if task.is_null() { continue; }
            let state_str = match (*task).state {
                TaskState::READY => b"READY \0",
                TaskState::RUNNING => b"RUN   \0",
                TaskState::BLOCKED => b"BLOCK \0",
                TaskState::WAITING => b"WAIT  \0",
                TaskState::EXITED => b"EXIT  \0",
            };
            pos += write_padded_u32(buf, pos, pid, 6);
            pos += write_padded_u32(buf, pos, (*task).ppid, 6);
            i = 0;
            while i < 6 && pos < buf.len() {
                buf[pos] = state_str[i];
                pos += 1;
                i += 1;
            }
            if (*task).cwd[0] != 0 {
                let mut j = 0;
                while j < 127 && (*task).cwd[j] != 0 && pos < buf.len() {
                    buf[pos] = (*task).cwd[j];
                    pos += 1;
                    j += 1;
                }
            } else {
                let shell = b"shell\0";
                i = 0;
                while i < shell.len() && pos < buf.len() {
                    buf[pos] = shell[i];
                    pos += 1;
                    i += 1;
                }
            }
            if pos < buf.len() {
                buf[pos] = b'\n';
                pos += 1;
            }
        }
        pos
    }
}

pub fn append_str(buf: &mut [u8], pos: &mut usize, s: &[u8]) {
    let mut i = 0;
    while i < s.len() && *pos < buf.len() {
        buf[*pos] = s[i];
        *pos += 1;
        i += 1;
    }
}

pub fn append_u32(buf: &mut [u8], pos: &mut usize, val: u32) {
    if val == 0 {
        if *pos < buf.len() { buf[*pos] = b'0'; *pos += 1; }
        return;
    }
    let mut tmp = [0u8; 12];
    let mut n = 0;
    let mut v = val;
    while v > 0 { tmp[n] = b'0' + (v % 10) as u8; v /= 10; n += 1; }
    let mut i = n;
    while i > 0 {
        i -= 1;
        if *pos < buf.len() { buf[*pos] = tmp[i]; *pos += 1; }
    }
}

fn write_padded_u32(buf: &mut [u8], start: usize, val: u32, width: usize) -> usize {
    let mut tmp = [0u8; 12];
    let mut n = 0;
    if val == 0 { tmp[0] = b'0'; n = 1; }
    else {
        let mut v = val;
        while v > 0 { tmp[n] = b'0' + (v % 10) as u8; v /= 10; n += 1; }
    }
    let mut written = 0;
    let padding = if width > n { width - n } else { 0 };
    let mut p = 0;
    while p < padding && start + written < buf.len() { buf[start + written] = b' '; written += 1; p += 1; }
    let mut i = n;
    while i > 0 {
        i -= 1;
        if start + written < buf.len() { buf[start + written] = tmp[i]; }
        written += 1;
    }
    written
}

fn append_u32_hex(buf: &mut [u8], pos: &mut usize, val: u32) {
    if val == 0 {
        if *pos + 2 <= buf.len() { buf[*pos] = b'0'; buf[*pos + 1] = b'0'; *pos += 2; }
        return;
    }
    let hex = b"0123456789abcdef";
    let mut tmp = [0u8; 8];
    let mut n = 0;
    let mut v = val;
    while v > 0 { tmp[n] = hex[(v & 0xF) as usize]; v >>= 4; n += 1; }
    // Pad to at least 2 digits
    if n < 2 { n = 2; }
    let mut i = n;
    while i > 0 {
        i -= 1;
        if *pos < buf.len() { buf[*pos] = tmp[i]; *pos += 1; }
    }
}
