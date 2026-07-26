use crate::scheduler::Registers;
use crate::scheduler::MAX_FDS;
use crate::vfs::fd_ref;
use core::ptr;

const USER_ADDR_MAX: u64 = 0x00007FFFFFFFFFFF;
const PROT_READ: u64 = 1;
const PROT_WRITE: u64 = 2;
const PROT_EXEC: u64 = 4;
const VMA_TYPE_ANON: u64 = 0x100;
const VMA_TYPE_HEAP: u64 = 0x800;
const MMAP_VADDR: u64 = 0x40000000;
const BRK_INITIAL: u64 = 0x10000000;
const USTACK_VADDR: u64 = 0xC0000000;
const PAGE_SIZE: u64 = 4096;
const NODE_FILE: u8 = 0;
const NODE_DIR: u8 = 1;
const NSIG: usize = 32;

extern "C" {
    fn syscall_entry();
    fn krust_ps2kbd_getchar() -> u8;
}

fn is_user_range(ptr: *const u8, size: u64) -> bool {
    let addr = ptr as u64;
    if addr.checked_add(size).is_none() {
        return false;
    }
    if addr + size > USER_ADDR_MAX {
        return false;
    }
    if addr < 0x1000 {
        return false;
    }
    true
}

fn copy_string_from_user(dst: *mut u8, src: *const u8, max: usize) -> i32 {
    if !is_user_range(src, max as u64) {
        return -1;
    }
    unsafe {
        core::arch::asm!("stac");
        for i in 0..max {
            let c = ptr::read_volatile(src.add(i));
            ptr::write_volatile(dst.add(i), c);
            if c == 0 {
                core::arch::asm!("clac");
                return i as i32;
            }
        }
        ptr::write_volatile(dst.add(max - 1), 0);
        core::arch::asm!("clac");
        (max - 1) as i32
    }
}

fn copy_to_user(dst: *mut u8, src: *const u8, len: u64) -> i32 {
    if !is_user_range(dst, len) {
        return -1;
    }
    unsafe {
        core::arch::asm!("stac");
        for i in 0..len as usize {
            ptr::write_volatile(dst.add(i), ptr::read_volatile(src.add(i)));
        }
        core::arch::asm!("clac");
    }
    0
}

fn copy_from_user(dst: *mut u8, src: *const u8, len: u64) -> i32 {
    if !is_user_range(src, len) {
        return -1;
    }
    unsafe {
        core::arch::asm!("stac");
        for i in 0..len as usize {
            ptr::write_volatile(dst.add(i), ptr::read_volatile(src.add(i)));
        }
        core::arch::asm!("clac");
    }
    0
}

fn copy_cstr_to_user(dst: *mut u8, src: &[u8]) {
    let len = src.len().min(64);
    if !is_user_range(dst, len as u64 + 1) {
        return;
    }
    unsafe {
        core::arch::asm!("stac");
    }
    let mut i = 0;
    while i < src.len() && i < 64 && src[i] != 0 {
        unsafe { ptr::write_volatile(dst.add(i), src[i]); }
        i += 1;
    }
    unsafe {
        ptr::write_volatile(dst.add(i), 0);
        core::arch::asm!("clac");
    }
}

fn append_cstr(buf: &mut [u8], pos: &mut usize, s: &[u8]) {
    let mut i = 0;
    while i < s.len() && s[i] != 0 && *pos < buf.len() {
        buf[*pos] = s[i];
        *pos += 1;
        i += 1;
    }
}

#[no_mangle]
pub unsafe extern "C" fn syscall_init() {
    crate::idt::set_gate(0x80, syscall_entry as u64, 0x08, 0xEE);

    let star = (0x08u64 << 32) | (0x08u64 << 48);
    crate::irq::wrmsr(0xC0000081, star);
    crate::irq::wrmsr(0xC0000082, syscall_entry as u64);
    crate::irq::wrmsr(0xC0000084, 0x47700);

    crate::vga::krust_vga_writestring_color(
        b"Syscall int 0x80 & MSR registered\n\0" as *const u8,
        0x02,
    );
}

#[no_mangle]
pub unsafe extern "C" fn syscall_handler_c(r: *mut Registers) {
    if r.is_null() {
        crate::vga::krust_vga_writestring(
            b"[ERROR] Null registers pointer in syscall handler\n\0" as *const u8,
        );
        return;
    }

    crate::scheduler::krust_sched_deliver_signals(r);

    let num = (*r).rax;
    let arg1 = (*r).rbx;
    let arg2 = (*r).rcx;
    let arg3 = (*r).rdx;
    let arg4 = (*r).r10;
    let arg5 = (*r).r8;
    let _arg6 = (*r).r9;

    if num > 83 {
        (*r).rax = 0xFFFFFFFF;
        return;
    }

    if num == 3 {
        (*r).rax = 0;
    }

    match num {
        0 => {
            if !is_user_range(arg1 as *const u8, arg2) {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let str_ptr = arg1 as *const u8;
                let current = crate::scheduler::krust_sched_current();
                if !current.is_null() && (*current).stdout_fd >= 0 {
                    crate::vfs::krust_vfs_pipe_write((*current).stdout_fd, str_ptr, arg2 as u32);
                } else {
                    crate::vga::krust_vga_write(str_ptr, arg2 as usize);
                    crate::ns16550::krust_ns16550_write_buf(str_ptr, arg2 as usize);
                }
            }
        }
        1 => {
            crate::scheduler::krust_sched_exit(arg1 as u32);
        }
        2 => {
            crate::scheduler::krust_sched_sleep_ticks(arg1 as u32);
        }
        3 => {
            crate::scheduler::krust_sched_yield();
        }
        4 => {
            let mut path = [0u8; 256];
            if copy_string_from_user(path.as_mut_ptr(), arg1 as *const u8, 256) < 0 {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let fd = crate::vfs::krust_vfs_open(path.as_ptr());
                (*r).rax = if fd >= 0 { fd as u64 } else { 0xFFFFFFFF };
            }
        }
        5 => {
            let fd = arg1 as i32;
            if !is_user_range(arg2 as *const u8, arg3) {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let result = crate::vfs::krust_vfs_read(fd, arg2 as *mut u8, arg3 as u32);
                (*r).rax = if result >= 0 { result as u64 } else { 0xFFFFFFFF };
            }
        }
        6 => {
            let fd = arg1 as i32;
            (*r).rax = if crate::vfs::krust_vfs_close(fd) >= 0 {
                0
            } else {
                0xFFFFFFFF
            };
        }
        7 => {
            let mut path = [0u8; 256];
            if copy_string_from_user(path.as_mut_ptr(), arg1 as *const u8, 256) < 0 {
                (*r).rax = 0xFFFFFFFF;
            } else if !is_user_range(arg2 as *const u8, arg3) {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let buf = arg2 as *mut u8;
                let buf_len = arg3 as u32;
                let dir = crate::vfs::krust_vfs_resolve(path.as_ptr());
                if dir.is_null() || (*dir).type_ != NODE_DIR {
                    (*r).rax = 0xFFFFFFFF;
                } else {
                    core::arch::asm!("stac");
                    let mut total: u32 = 0;
                    let mut child = (*dir).children;
                    while !child.is_null() {
                        let mut name_len: u32 = 0;
                        while (*child).name[name_len as usize] != 0 {
                            name_len += 1;
                        }
                        if total + name_len + 2 > buf_len {
                            break;
                        }
                        for i in 0..name_len {
                            ptr::write_volatile(
                                buf.add(total as usize),
                                (*child).name[i as usize],
                            );
                            total += 1;
                        }
                        ptr::write_volatile(
                            buf.add(total as usize),
                            if (*child).type_ == NODE_DIR {
                                b'/'
                            } else {
                                b'\n'
                            },
                        );
                        total += 1;
                        child = (*child).next;
                    }
                    if total < buf_len {
                        ptr::write_volatile(buf.add(total as usize), 0);
                    }
                    core::arch::asm!("clac");
                    (*r).rax = total as u64;
                }
            }
        }
        8 => {
            let mut path = [0u8; 256];
            if copy_string_from_user(path.as_mut_ptr(), arg1 as *const u8, 256) < 0 {
                (*r).rax = 0xFFFFFFFF;
            } else if !is_user_range(arg2 as *const u8, arg3) {
                (*r).rax = 0xFFFFFFFF;
            } else {
                (*r).rax = if crate::vfs::krust_vfs_write_file(
                    path.as_ptr(),
                    arg2 as *const u8,
                    arg3 as u32,
                ) == 0 {
                    0
                } else {
                    0xFFFFFFFF
                };
            }
        }
        9 => {
            let mut path = [0u8; 256];
            if copy_string_from_user(path.as_mut_ptr(), arg1 as *const u8, 256) < 0 {
                (*r).rax = 0xFFFFFFFF;
            } else {
                (*r).rax = if crate::vfs::krust_vfs_mkdir(path.as_ptr()) == 0 {
                    0
                } else {
                    0xFFFFFFFF
                };
            }
        }
        10 => {
            let mut path = [0u8; 256];
            if copy_string_from_user(path.as_mut_ptr(), arg1 as *const u8, 256) < 0 {
                (*r).rax = 0xFFFFFFFF;
            } else {
                (*r).rax = if crate::vfs::krust_vfs_unlink(path.as_ptr()) == 0 {
                    0
                } else {
                    0xFFFFFFFF
                };
            }
        }
        11 => {
            let mut path = [0u8; 256];
            if copy_string_from_user(path.as_mut_ptr(), arg1 as *const u8, 256) < 0 {
                (*r).rax = 0xFFFFFFFF;
            } else {
                (*r).rax = if crate::vfs::krust_vfs_rmdir(path.as_ptr()) == 0 {
                    0
                } else {
                    0xFFFFFFFF
                };
            }
        }
        12 => {
            let mut old_path = [0u8; 256];
            let mut new_path = [0u8; 256];
            if copy_string_from_user(old_path.as_mut_ptr(), arg1 as *const u8, 256) < 0 {
                (*r).rax = 0xFFFFFFFF;
            } else if copy_string_from_user(new_path.as_mut_ptr(), arg2 as *const u8, 256) < 0 {
                (*r).rax = 0xFFFFFFFF;
            } else {
                (*r).rax = if crate::vfs::krust_vfs_rename(
                    old_path.as_ptr(),
                    new_path.as_ptr(),
                ) == 0 {
                    0
                } else {
                    0xFFFFFFFF
                };
            }
        }
        13 => {
            let mut path = [0u8; 256];
            if copy_string_from_user(path.as_mut_ptr(), arg1 as *const u8, 256) < 0 {
                (*r).rax = 0xFFFFFFFF;
            } else if !is_user_range(
                arg2 as *const u8,
                core::mem::size_of::<crate::vfs::FileStat>() as u64,
            ) {
                (*r).rax = 0xFFFFFFFF;
            } else {
                (*r).rax = if crate::vfs::krust_vfs_stat(
                    path.as_ptr(),
                    arg2 as *mut crate::vfs::FileStat,
                ) == 0 {
                    0
                } else {
                    0xFFFFFFFF
                };
            }
        }
        14 => {
            let current = crate::scheduler::krust_sched_current();
            if !current.is_null() && (*current).stdin_fd >= 0 {
                let mut c: u8 = 0;
                let n =
                    crate::vfs::krust_vfs_pipe_read((*current).stdin_fd, &mut c as *mut u8, 1);
                (*r).rax = if n > 0 { c as u64 } else { 0xFFFFFFFF };
            } else {
                let c = krust_ps2kbd_getchar();
                (*r).rax = c as u64;
            }
        }
        15 => {
            if !is_user_range(arg1 as *const u8, 2 * core::mem::size_of::<i32>() as u64) {
                (*r).rax = 0xFFFFFFFF;
            } else {
                (*r).rax = if crate::vfs::krust_vfs_pipe_create(arg1 as *mut i32) == 0 {
                    0
                } else {
                    0xFFFFFFFF
                };
            }
        }
        16 => {
            let fd = arg1 as i32;
            if !is_user_range(arg2 as *const u8, arg3) {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let result = crate::vfs::krust_vfs_pipe_read(fd, arg2 as *mut u8, arg3 as u32);
                (*r).rax = if result >= 0 { result as u64 } else { 0xFFFFFFFF };
            }
        }
        17 => {
            let fd = arg1 as i32;
            if !is_user_range(arg2 as *const u8, arg3) {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let result = crate::vfs::krust_vfs_pipe_write(fd, arg2 as *const u8, arg3 as u32);
                (*r).rax = if result >= 0 { result as u64 } else { 0xFFFFFFFF };
            }
        }
        18 => {
            crate::vfs::krust_vfs_pipe_close(arg1 as i32);
            (*r).rax = 0;
        }
        19 => {
            let fd = arg1 as i32;
            if !is_user_range(arg2 as *const u8, arg3) {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let result = crate::vfs::krust_vfs_write_fd(fd, arg2 as *const u8, arg3 as u32);
                (*r).rax = if result >= 0 { result as u64 } else { 0xFFFFFFFF };
            }
        }
        20 => {
            crate::vga::krust_vga_clear();
            (*r).rax = 0;
        }
        21 => {
            (*r).rax = crate::pittimer::krust_pittimer_get_ticks() as u64;
            if arg1 != 0 && arg2 > 0 {
                let buf = arg1 as *mut u8;
                // utsname layout: sysname(65) nodename(65) release(65) version(65) machine(65)
                let field_len = 65usize;
                // sysname
                copy_cstr_to_user(buf, crate::KERNEL_NAME.as_bytes());
                // nodename
                let nodename = buf.add(field_len);
                copy_cstr_to_user(nodename, b"elitra\0");
                // release
                let release = buf.add(field_len * 2);
                copy_cstr_to_user(release, crate::KERNEL_VERSION.as_bytes());
                // version
                let version = buf.add(field_len * 3);
                let mut vbuf = [0u8; 65];
                let mut vpos = 0;
                append_cstr(&mut vbuf, &mut vpos, crate::KERNEL_NAME.as_bytes());
                append_cstr(&mut vbuf, &mut vpos, b" v\0");
                append_cstr(&mut vbuf, &mut vpos, crate::KERNEL_VERSION.as_bytes());
                append_cstr(&mut vbuf, &mut vpos, b" \0");
                append_cstr(&mut vbuf, &mut vpos, crate::KERNEL_ARCH.as_bytes());
                copy_cstr_to_user(version, &vbuf);
                // machine
                let machine = buf.add(field_len * 4);
                copy_cstr_to_user(machine, crate::KERNEL_ARCH.as_bytes());
            }
        }
        22 => {
            crate::vga::krust_vga_clear();
            crate::vga::krust_vga_writestring(b"Rebooting...\n\0" as *const u8);
            if crate::acpi::krust_acpi_is_available() != 0 {
                crate::acpi::krust_acpi_reboot();
            } else {
                crate::irq::outb(0x64, 0xFE);
            }
            (*r).rax = 0;
        }
        23 => {
            crate::vga::krust_vga_clear();
            crate::vga::krust_vga_writestring(b"Power off...\n\0" as *const u8);
            if crate::acpi::krust_acpi_is_available() != 0 {
                crate::acpi::krust_acpi_poweroff();
            } else {
                crate::irq::outw(0x604, 0x2000);
                crate::irq::outw(0xB004, 0x2000);
            }
            (*r).rax = 0;
        }
        24 => {
            let mut path = [0u8; 256];
            if copy_string_from_user(path.as_mut_ptr(), arg1 as *const u8, 256) < 0 {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let fd = crate::vfs::krust_vfs_open_write(path.as_ptr());
                (*r).rax = if fd >= 0 { fd as u64 } else { 0xFFFFFFFF };
            }
        }
        25 => {
            if !is_user_range(arg1 as *const u8, arg2) {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let current = crate::scheduler::krust_sched_current();
                if !current.is_null() && arg2 > 0 {
                    let buf = arg1 as *mut u8;
                    let mut i: u64 = 0;
                    core::arch::asm!("stac");
                    while i < arg2 - 1 && (*current).cwd[i as usize] != 0 {
                        ptr::write_volatile(buf.add(i as usize), (*current).cwd[i as usize]);
                        i += 1;
                    }
                    ptr::write_volatile(buf.add(i as usize), 0);
                    core::arch::asm!("clac");
                    (*r).rax = 0;
                } else {
                    (*r).rax = 0xFFFFFFFF;
                }
            }
        }
        26 => {
            let mut path = [0u8; 256];
            if copy_string_from_user(path.as_mut_ptr(), arg1 as *const u8, 256) < 0 {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let current = crate::scheduler::krust_sched_current();
                if !current.is_null() {
                    let node = crate::vfs::krust_vfs_resolve(path.as_ptr());
                    if !node.is_null() && (*node).type_ == NODE_DIR {
                        let mut i = 0;
                        while i < 127 && path[i] != 0 {
                            (*current).cwd[i] = path[i];
                            i += 1;
                        }
                        (*current).cwd[i] = 0;
                        (*r).rax = 0;
                    } else {
                        (*r).rax = 0xFFFFFFFF;
                    }
                } else {
                    (*r).rax = 0xFFFFFFFF;
                }
            }
        }
        27 => {
            if !is_user_range(
                arg1 as *const u8,
                core::mem::size_of::<crate::cmos_rtc::RTCInfo>() as u64,
            ) {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let info = crate::cmos_rtc::krust_cmos_read_time();
                if copy_to_user(
                    arg1 as *mut u8,
                    &info as *const crate::cmos_rtc::RTCInfo as *const u8,
                    core::mem::size_of::<crate::cmos_rtc::RTCInfo>() as u64,
                ) == 0
                {
                    (*r).rax = 0;
                } else {
                    (*r).rax = 0xFFFFFFFF;
                }
            }
        }
        28 => {
            let pid = crate::scheduler::krust_sched_fork(r);
            (*r).rax = if pid >= 0 { pid as u64 } else { 0xFFFFFFFF };
        }
        29 => {
            let exit_val: u64;

            let mut path = [0u8; 256];
            if copy_string_from_user(path.as_mut_ptr(), arg1 as *const u8, 256) < 0 {
                exit_val = 0xFFFFFFFF;
            } else {
                let argc = arg2 as i32;
                let argv = arg3 as *const *const u8;
                let mut valid = true;
                if argc > 0 && !argv.is_null() {
                    if !is_user_range(
                        argv as *const u8,
                        (argc as u64 + 1) * core::mem::size_of::<*const u8>() as u64,
                    ) {
                        valid = false;
                    }
                    if valid {
                        for i in 0..argc {
                            if !is_user_range(*argv.add(i as usize) as *const u8, 1) {
                                valid = false;
                                break;
                            }
                        }
                    }
                }
                if !valid {
                    exit_val = 0xFFFFFFFF;
                } else {
                    let result =
                        crate::scheduler::krust_sched_execve(r, path.as_ptr(), argc, argv);
                    exit_val = if result == 0 { 0 } else { 0xFFFFFFFF };
                }
            }

            (*r).rax = exit_val;
        }
        30 => {
            let pid = arg1 as i32;
            let mut status: i32 = 0;
            let flags = arg3 as u32;
            let result =
                crate::scheduler::krust_sched_waitpid_flags(pid, &mut status as *mut i32, flags);
            if result >= 0 {
                if arg2 != 0
                    && is_user_range(
                        arg2 as *const u8,
                        core::mem::size_of::<i32>() as u64,
                    )
                {
                    copy_to_user(
                        arg2 as *mut u8,
                        &status as *const i32 as *const u8,
                        core::mem::size_of::<i32>() as u64,
                    );
                }
                (*r).rax = result as u64;
            } else {
                (*r).rax = 0xFFFFFFFF;
            }
        }
        31 => {
            (*r).rax = crate::scheduler::krust_sched_get_pid() as u64;
        }
        32 => {
            let pid = arg1 as i32;
            let sig = arg2 as i32;
            (*r).rax = if crate::scheduler::krust_sched_kill(pid, sig) == 0 {
                0
            } else {
                0xFFFFFFFF
            };
        }
        33 => {
            let sig = arg1 as i32;
            let handler = arg2;
            let old_handler = if arg3 != 0 {
                arg3 as *mut u64
            } else {
                ptr::null_mut()
            };
            (*r).rax = if crate::scheduler::krust_sched_sigaction(sig, handler, old_handler) == 0 {
                0
            } else {
                0xFFFFFFFF
            };
        }
        34 => {
            crate::scheduler::krust_sched_sigreturn(r);
        }
        35 => {
            (*r).rax = crate::scheduler::krust_sched_get_ppid() as u64;
        }
        36 => {
            let mut addr = arg1;
            let length = arg2;
            let prot = arg3;
            let flags = arg4;

            if length == 0 {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let length_aligned = (length + 0xFFF) & !0xFFF;
                let map_fixed = (flags & 0x10) != 0;

                let current = crate::scheduler::krust_sched_current();
                if current.is_null() {
                    (*r).rax = 0xFFFFFFFF;
                } else {
                    if map_fixed && addr != 0 {
                        crate::vma::krust_vmm_remove(
                            &mut (*current).vma_list,
                            addr,
                            addr + length_aligned,
                        );
                    } else if addr == 0 {
                        addr = MMAP_VADDR;
                        while crate::vma::krust_vmm_has_overlap(
                            (*current).vma_list,
                            addr,
                            addr + length_aligned,
                        ) != 0
                        {
                            addr += 0x100000;
                        }
                    }

                    let vma = crate::vma::krust_vmm_add(
                        &mut (*current).vma_list,
                        addr,
                        addr + length_aligned,
                        prot | VMA_TYPE_ANON,
                    );
                    if vma.is_null() {
                        (*r).rax = 0xFFFFFFFF;
                    } else {
                        (*r).rax = addr;
                    }
                }
            }
        }
        37 => {
            let addr = arg1;
            let length = arg2;
            if length == 0 {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let end = ((addr + length) + 0xFFF) & !0xFFF;
                let aligned_addr = addr & !0xFFF;

                let mut page = aligned_addr;
                while page < end {
                    let phys = crate::paging::krust_paging_get_phys(page);
                    if phys != !0u64 {
                        crate::paging::krust_paging_unmap_page(page);
                        crate::pmm::krust_pmm_free_frame((phys / 4096) as usize);
                    }
                    page += 0x1000;
                }

                let current = crate::scheduler::krust_sched_current();
                if !current.is_null() {
                    crate::vma::krust_vmm_remove(
                        &mut (*current).vma_list,
                        aligned_addr,
                        end,
                    );
                }
                (*r).rax = 0;
            }
        }
        38 => {
            let new_brk = arg1;
            let current = crate::scheduler::krust_sched_current();
            if current.is_null() {
                (*r).rax = 0xFFFFFFFF;
            } else if new_brk == 0 {
                (*r).rax = (*current).program_brk;
            } else if new_brk < BRK_INITIAL {
                (*r).rax = (*current).program_brk;
            } else if new_brk > USTACK_VADDR - 0x100000 {
                (*r).rax = (*current).program_brk;
            } else {
                let old_brk = (*current).program_brk;
                if new_brk > old_brk {
                    let start = old_brk;
                    let end = (new_brk + 0xFFF) & !0xFFF;
                    crate::vma::krust_vmm_add(
                        &mut (*current).vma_list,
                        start,
                        end,
                        PROT_READ | PROT_WRITE | VMA_TYPE_HEAP,
                    );
                    (*current).program_brk = new_brk;
                } else {
                    let start = (new_brk + 0xFFF) & !0xFFF;
                    let end = (old_brk + 0xFFF) & !0xFFF;
                    let mut page = start;
                    while page < end {
                        let phys = crate::paging::krust_paging_get_phys(page);
                        if phys != !0u64 {
                            crate::paging::krust_paging_unmap_page(page);
                            crate::pmm::krust_pmm_free_frame((phys / 4096) as usize);
                        }
                        page += 0x1000;
                    }
                    crate::vma::krust_vmm_remove(&mut (*current).vma_list, start, end);
                    (*current).program_brk = new_brk;
                }
                (*r).rax = (*current).program_brk;
            }
        }
        39 => {
            let addr = arg1;
            let len = arg2;
            let prot = arg3;

            if len == 0 {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let end_addr = ((addr + len) + 0xFFF) & !0xFFF;
                let aligned_addr = addr & !0xFFF;

                let current = crate::scheduler::krust_sched_current();
                if current.is_null() {
                    (*r).rax = 0xFFFFFFFF;
                } else {
                    let vma =
                        crate::vma::krust_vmm_find((*current).vma_list, aligned_addr);
                    if vma.is_null() || (*vma).end <= aligned_addr {
                        (*r).rax = 0xFFFFFFFF;
                    } else {
                        crate::vma::krust_vmm_remove(
                            &mut (*current).vma_list,
                            aligned_addr,
                            end_addr,
                        );
                        crate::vma::krust_vmm_add(
                            &mut (*current).vma_list,
                            aligned_addr,
                            end_addr,
                            prot | VMA_TYPE_ANON,
                        );

                        let mut page = aligned_addr;
                        while page < end_addr {
                            let phys = crate::paging::krust_paging_get_phys(page);
                            if phys != !0u64 {
                                let mut pte_flags: u64 = 0x1 | 0x4;
                                if prot & PROT_WRITE != 0 {
                                    pte_flags |= 0x2;
                                }
                                if prot & PROT_EXEC == 0 {
                                    pte_flags |= 0x8000000000000000;
                                }
                                crate::paging::krust_paging_map_page(page, phys, pte_flags);
                            }
                            page += 0x1000;
                        }
                        (*r).rax = 0;
                    }
                }
            }
        }
        40 => {
            let fd = arg1 as i32;
            let offset = arg2 as i64;
            let whence = arg3 as i32;
            let result = crate::vfs::krust_vfs_lseek(fd, offset, whence);
            (*r).rax = if result >= 0 { result as u64 } else { 0xFFFFFFFF };
        }
        41 => {
            let oldfd = arg1 as i32;
            let result = crate::vfs::krust_vfs_dup(oldfd);
            (*r).rax = if result >= 0 { result as u64 } else { 0xFFFFFFFF };
        }
        42 => {
            let fd = arg1 as i32;
            let cmd = arg2 as i32;
            let arg = arg3;
            let result = crate::vfs::krust_vfs_fcntl(fd, cmd, arg);
            (*r).rax = result as u64;
        }
        43 => {
            let fd = arg1 as i32;
            let request = arg2 as u32;
            let arg = arg3;
            let result = crate::vfs::krust_vfs_ioctl(fd, request, arg);
            (*r).rax = result as u64;
        }
        44 => {
            let fds = arg1 as *mut u8;
            let nfds = arg2;
            let timeout = arg3 as i32;
            if nfds == 0 || nfds > 1024 || !is_user_range(fds as *const u8, nfds * 8) {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let result = crate::vfs::krust_vfs_poll(fds as *mut crate::vfs::PollFdEntry, nfds, timeout);
                (*r).rax = result as u64;
            }
        }
        45 => {
            let nfds = arg1 as i32;
            let readfds = arg2 as *mut u8;
            let writefds = arg3 as *mut u8;
            let exceptfds = arg4 as *mut u8;
            let timeout = arg5 as *mut u8;
            let bitmap_size = ((nfds as u32 + 7) / 8) as u64;
            if nfds <= 0 || nfds > 1024
                || (!readfds.is_null() && !is_user_range(readfds, bitmap_size))
                || (!writefds.is_null() && !is_user_range(writefds, bitmap_size))
                || (!exceptfds.is_null() && !is_user_range(exceptfds, bitmap_size))
                || (!timeout.is_null() && !is_user_range(timeout as *const u8, 8))
            {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let result = crate::vfs::krust_vfs_select(nfds, readfds, writefds, exceptfds, timeout);
                (*r).rax = result as u64;
            }
        }
        46 => {
            let flags = arg1;
            let stack = arg2;
            let ptid = arg3 as *mut u32;
            let tls = arg4;
            let ctid = arg5 as *mut u32;
            let result = crate::scheduler::krust_sched_clone(r, flags, stack, ptid, tls, ctid);
            (*r).rax = if result >= 0 { result as u64 } else { 0xFFFFFFFF };
        }
        47 => {
            let entry: extern "C" fn(u64) = core::mem::transmute(arg1);
            let thread_arg = arg2;
            let stack = arg3;
            let flags = arg4;
            let result =
                crate::scheduler::krust_sched_thread_create(entry, thread_arg, stack, flags);
            (*r).rax = if result >= 0 { result as u64 } else { 0xFFFFFFFF };
        }
        48 => {
            let tid = arg1 as u32;
            let result = crate::scheduler::krust_sched_thread_join(tid);
            (*r).rax = if result >= 0 { result as u64 } else { 0xFFFFFFFF };
        }
        49 => {
            let tid = arg1 as u32;
            let result = crate::scheduler::krust_sched_thread_detach(tid);
            (*r).rax = if result >= 0 { result as u64 } else { 0xFFFFFFFF };
        }
        50 => {
            let current = crate::scheduler::krust_sched_current();
            if !current.is_null() {
                (*r).rax = (*current).uid as u64;
            } else {
                (*r).rax = 0;
            }
        }
        51 => {
            let current = crate::scheduler::krust_sched_current();
            if !current.is_null() {
                (*r).rax = (*current).gid as u64;
            } else {
                (*r).rax = 0;
            }
        }
        52 => {
            if arg2 >= core::mem::size_of::<u64>() as u64
                && is_user_range(arg1 as *const u8, core::mem::size_of::<u64>() as u64)
            {
                let mask: u64 = 1;
                copy_to_user(
                    arg1 as *mut u8,
                    &mask as *const u64 as *const u8,
                    core::mem::size_of::<u64>() as u64,
                );
            }
            (*r).rax = core::mem::size_of::<u64>() as u64;
        }
        53 => {
            let pid = crate::scheduler::krust_sched_fork(r);
            (*r).rax = if pid >= 0 { pid as u64 } else { 0xFFFFFFFF };
        }
        54 => {
            let pid = crate::scheduler::krust_sched_fork(r);
            if pid == 0 {
                (*r).rax = 0;
            } else if pid > 0 {
                crate::scheduler::krust_sched_waitpid(pid, ptr::null_mut());
                (*r).rax = pid as u64;
            } else {
                (*r).rax = 0xFFFFFFFF;
            }
        }
        55 => {
            // socket(type): SOCK_STREAM=1, SOCK_DGRAM=2
            let result = crate::net::net_socket(arg1 as u32);
            (*r).rax = if result >= 0 { result as u64 } else { 0xFFFFFFFF };
        }
        56 => {
            // connect(sockfd, ip[4], port)
            let sockfd = arg1 as i32;
            let ip_ptr = arg2 as *const u8;
            let port = arg3 as u16;
            if !is_user_range(ip_ptr, 4) {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let mut ip = [0u8; 4];
                core::arch::asm!("stac");
                for i in 0..4u64 {
                    ip[i as usize] = ptr::read_volatile(ip_ptr.add(i as usize));
                }
                core::arch::asm!("clac");
                let result = crate::net::net_connect(sockfd, ip, port);
                (*r).rax = if result == 0 { 0 } else { 0xFFFFFFFF };
            }
        }
        57 => {
            // sendto(sockfd, data, len, ip, port)
            let sockfd = arg1 as i32;
            let data_ptr = arg2 as *const u8;
            let data_len = arg3 as usize;
            let ip_ptr = arg4 as *const u8;
            let port = arg5 as u16;
            if !is_user_range(data_ptr, data_len as u64) || !is_user_range(ip_ptr, 4) {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let mut data = [0u8; 4096];
                let copy_len = core::cmp::min(data_len, 4096);
                core::arch::asm!("stac");
                for i in 0..copy_len {
                    data[i] = ptr::read_volatile(data_ptr.add(i));
                }
                let mut ip = [0u8; 4];
                for i in 0..4u64 {
                    ip[i as usize] = ptr::read_volatile(ip_ptr.add(i as usize));
                }
                core::arch::asm!("clac");
                match crate::net::net_sendto(sockfd, &data[..copy_len], ip, port) {
                    Ok(sent) => { (*r).rax = sent as u64; }
                    Err(e) => { (*r).rax = (e as i64) as u64; }
                }
            }
        }
        58 => {
            // recvfrom(sockfd, buf, maxlen) -> (bytes_read, src_ip[4], src_port)
            let sockfd = arg1 as i32;
            let buf_ptr = arg2 as *mut u8;
            let maxlen = arg3 as usize;
            let info_ptr = arg4 as *mut u8; // optional: 6 bytes = ip[4] + port(u16)
            if !is_user_range(buf_ptr, maxlen as u64) {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let mut buf = [0u8; 4096];
                let copy_max = core::cmp::min(maxlen, 4096);
                match crate::net::net_recvfrom(sockfd, &mut buf[..copy_max]) {
                    Ok((n, src_ip, src_port)) => {
                        core::arch::asm!("stac");
                        for i in 0..n {
                            ptr::write_volatile(buf_ptr.add(i), buf[i]);
                        }
                        if !info_ptr.is_null() && is_user_range(info_ptr, 6) {
                            for i in 0..4u64 {
                                ptr::write_volatile(info_ptr.add(i as usize), src_ip[i as usize]);
                            }
                            ptr::write_volatile(info_ptr.add(4), (src_port >> 8) as u8);
                            ptr::write_volatile(info_ptr.add(5), src_port as u8);
                        }
                        core::arch::asm!("clac");
                        (*r).rax = n as u64;
                    }
                    Err(e) => {
                        (*r).rax = (e as i64) as u64;
                    }
                }
            }
        }
        59 => {
            // bind(sockfd, port)
            let sockfd = arg1 as i32;
            let port = arg2 as u16;
            let result = crate::net::net_bind(sockfd, port);
            (*r).rax = if result == 0 { 0 } else { 0xFFFFFFFF };
        }
        60 => {
            // close_socket(sockfd)
            let sockfd = arg1 as i32;
            let result = crate::net::net_close_socket(sockfd);
            (*r).rax = if result == 0 { 0 } else { 0xFFFFFFFF };
        }
        61 => {
            let oldfd = arg1 as i32;
            let newfd = arg2 as i32;
            let result = crate::vfs::krust_vfs_dup2(oldfd, newfd);
            (*r).rax = if result >= 0 { result as u64 } else { 0xFFFFFFFF };
        }
        62 => {
            // listen(sockfd, backlog)
            let sockfd = arg1 as i32;
            let backlog = arg2 as i32;
            let result = crate::net::net_listen(sockfd, backlog);
            (*r).rax = if result == 0 { 0 } else { 0xFFFFFFFF };
        }
        63 => {
            // accept(sockfd, client_ip_out, client_port_out)
            let sockfd = arg1 as i32;
            let ip_ptr = arg2 as *mut u8;
            let port_ptr = arg3 as *mut u16;
            let ip_valid = ip_ptr.is_null() || is_user_range(ip_ptr, 4);
            let port_valid = port_ptr.is_null() || is_user_range(port_ptr as *const u8, 2);
            if !ip_valid || !port_valid {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let result = crate::net::net_accept(sockfd, ip_ptr, port_ptr);
                (*r).rax = if result >= 0 { result as u64 } else { 0xFFFFFFFF };
            }
        }
        // ─── 64: ioctl(fd, request, arg) ───────────────────────
        64 => {
            let fd = arg1 as i32;
            let request = arg2 as u32;
            let user_arg = arg3 as *mut u8;
            match request {
                // TIOCGWINSZ — get terminal window size
                0x5413 => {
                    if user_arg.is_null() || !is_user_range(user_arg, 8) {
                        (*r).rax = 0xFFFFFFFF;
                    } else {
                        let fb_info = crate::framebuffer::krust_framebuffer_info();
                        let cols = fb_info.width / 8;
                        let rows = fb_info.height / 16;
                        core::arch::asm!("stac");
                        ptr::write_volatile(user_arg.add(0), (cols as u16 >> 8) as u8);
                        ptr::write_volatile(user_arg.add(1), cols as u8);
                        ptr::write_volatile(user_arg.add(2), (rows as u16 >> 8) as u8);
                        ptr::write_volatile(user_arg.add(3), rows as u8);
                        ptr::write_volatile(user_arg.add(4), (fb_info.width as u16 >> 8) as u8);
                        ptr::write_volatile(user_arg.add(5), fb_info.width as u8);
                        ptr::write_volatile(user_arg.add(6), (fb_info.height as u16 >> 8) as u8);
                        ptr::write_volatile(user_arg.add(7), fb_info.height as u8);
                        core::arch::asm!("clac");
                        (*r).rax = 0;
                    }
                }
                // TIOCSWINSZ — set terminal window size (no-op for now)
                0x5414 => { (*r).rax = 0; }
                // FIONREAD — bytes available for reading
                0x541B => {
                    if fd >= 0 && fd < MAX_FDS as i32 && fd_ref()[fd as usize].used {
                        let node = fd_ref()[fd as usize].node;
                        let avail: u32 = if !node.is_null() && (*node).type_ == 0 {
                            (*node).size.saturating_sub(fd_ref()[fd as usize].offset)
                        } else if crate::net::net_socket_has_data(fd) {
                            1
                        } else {
                            0
                        };
                        if !user_arg.is_null() && is_user_range(user_arg, 4) {
                            core::arch::asm!("stac");
                            ptr::write_volatile(user_arg as *mut u32, avail);
                            core::arch::asm!("clac");
                        }
                        (*r).rax = 0;
                    } else {
                        (*r).rax = 0xFFFFFFFF;
                    }
                }
                // TIOCSCTTY — set controlling terminal
                0x540E => { (*r).rax = 0; }
                // TIOCNOTTY — give up controlling terminal
                0x5422 => { (*r).rax = 0; }
                _ => { (*r).rax = 0xFFFFFFFF; }
            }
        }
        // ─── 65: poll(fds, nfds, timeout_ms) ──────────────────
        65 => {
            let pollfds_ptr = arg1 as *mut u8;
            let nfds = arg2 as usize;
            let timeout_ms = arg3 as i32;
            if nfds > 64 || pollfds_ptr.is_null() || !is_user_range(pollfds_ptr, (nfds * 8) as u64) {
                (*r).rax = 0xFFFFFFFF;
            } else {
                // Read pollfd array from userspace
                let mut fds_buf = [(0i32, 0u16, 0u16); 64];
                core::arch::asm!("stac");
                for i in 0..nfds {
                    let base = pollfds_ptr.add(i * 8);
                    let pfd = ptr::read_volatile(base as *const i32);
                    let events = ptr::read_volatile(base.add(4) as *const u16);
                    fds_buf[i] = (pfd, events, 0);
                }
                core::arch::asm!("clac");

                let mut total_ready: i32 = 0;
                let mut any_ready = false;
                let deadline = if timeout_ms >= 0 {
                    Some(crate::pittimer::krust_pittimer_get_ticks() as u64 + (timeout_ms as u64 * 3))
                } else {
                    None // infinite wait
                };

                loop {
                    total_ready = 0;
                    for i in 0..nfds {
                        let (fd, events, _) = fds_buf[i];
                        let mut revents: u16 = 0;
                        if fd < 0 {
                            revents = 0x0020; // POLLNVAL
                        } else if events & 0x001 != 0 {
                            // POLLIN
                            let has_data = if crate::vfs::is_pipe_fd(fd) {
                                crate::vfs::pipe_has_data(fd)
                            } else if fd >= 0 && fd < MAX_FDS as i32 && fd_ref()[fd as usize].used {
                                let node = fd_ref()[fd as usize].node;
                                if !node.is_null() && (*node).type_ == 0 {
                                    fd_ref()[fd as usize].offset < (*node).size
                                } else {
                                    crate::net::net_socket_has_data(fd)
                                }
                            } else {
                                false
                            };
                            if has_data { revents |= 0x001; }
                        }
                        if events & 0x004 != 0 {
                            // POLLOUT
                            let can_write = if crate::vfs::is_pipe_fd(fd) {
                                crate::vfs::pipe_can_write(fd)
                            } else {
                                true // files are always writable in this OS
                            };
                            if can_write { revents |= 0x004; }
                        }
                        fds_buf[i].2 = revents;
                        if revents != 0 {
                            total_ready += 1;
                            any_ready = true;
                        }
                    }

                    if any_ready || (timeout_ms == 0) {
                        break;
                    }
                    if let Some(dl) = deadline {
                        if crate::pittimer::krust_pittimer_get_ticks() as u64 >= dl {
                            break;
                        }
                    }
                    crate::scheduler::krust_sched_yield();
                }

                // Write back revents
                core::arch::asm!("stac");
                for i in 0..nfds {
                    let base = pollfds_ptr.add(i * 8);
                    ptr::write_volatile(base.add(4) as *mut u16, fds_buf[i].2);
                }
                core::arch::asm!("clac");
                (*r).rax = total_ready as u64;
            }
        }
        // ─── 66: clock_gettime(clockid, tp) ───────────────────
        66 => {
            let clockid = arg1 as i32;
            let tp = arg2 as *mut u8;
            if tp.is_null() || !is_user_range(tp, 16) {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let (tv_sec, tv_nsec) = match clockid {
                    0 => { // CLOCK_REALTIME — compute epoch from RTC + ticks
                        let rtc = unsafe { crate::cmos_rtc::krust_cmos_read_time() };
                        let y = rtc.year as u64;
                        let m = rtc.month as u64;
                        let d = rtc.day as u64;
                        let h = rtc.hour as u64;
                        let min = rtc.minute as u64;
                        let s = rtc.second as u64;
                        // Simple days-to-seconds since epoch
                        let days = (y - 1970) * 365 + (y - 1969) / 4 + (m - 1) * 30 + d - 1;
                        let secs = days * 86400 + h * 3600 + min * 60 + s;
                        (secs, 0u64)
                    }
                    1 | 4 => { // CLOCK_MONOTONIC / CLOCK_MONOTONIC_RAW
                        let ticks = crate::pittimer::krust_pittimer_get_ticks() as u64;
                        // PIT runs at 1193182 Hz base, configured to ~100 Hz
                        let ns = ticks * 10_000_000; // 10ms per tick
                        (ns / 1_000_000_000, ns % 1_000_000_000)
                    }
                    _ => { (*r).rax = 0xFFFFFFFF; return; }
                };
                core::arch::asm!("stac");
                ptr::write_volatile(tp as *mut u64, tv_sec);
                ptr::write_volatile(tp.add(8) as *mut u64, tv_nsec);
                core::arch::asm!("clac");
                (*r).rax = 0;
            }
        }
        // ─── 67: nanosleep(req, rem) ─────────────────────────
        67 => {
            let req = arg1 as *const u8;
            if req.is_null() || !is_user_range(req, 16) {
                (*r).rax = 0;
            } else {
                core::arch::asm!("stac");
                let tv_sec = ptr::read_volatile(req as *const u64);
                let tv_nsec = ptr::read_volatile(req.add(8) as *const u64);
                core::arch::asm!("clac");
                let total_ns = tv_sec * 1_000_000_000 + tv_nsec;
                // Convert to PIT ticks (roughly 100 Hz → 10ms per tick)
                let ticks = (total_ns / 10_000_000) as u32;
                if ticks > 0 {
                    crate::scheduler::krust_sched_sleep_ticks(ticks);
                } else if total_ns > 0 {
                    crate::power::hpet::krust_hpet_busy_wait_ns(total_ns);
                }
                // Signal that remainder is zero (no interrupt during sleep)
                let rem = arg2 as *mut u8;
                if !rem.is_null() && is_user_range(rem, 16) {
                    core::arch::asm!("stac");
                    ptr::write_volatile(rem as *mut u64, 0u64);
                    ptr::write_volatile(rem.add(8) as *mut u64, 0u64);
                    core::arch::asm!("clac");
                }
                (*r).rax = 0;
            }
        }
        // ─── 68: getuid() ─────────────────────────────────────
        68 => {
            let current = crate::scheduler::krust_sched_current();
            if !current.is_null() {
                (*r).rax = (*current).uid as u64;
            } else {
                (*r).rax = 0;
            }
        }
        // ─── 69: setuid(uid) ──────────────────────────────────
        69 => {
            let current = crate::scheduler::krust_sched_current();
            if !current.is_null() {
                if (*current).uid == 0 {
                    (*current).uid = arg1 as u16;
                    (*r).rax = 0;
                } else {
                    (*r).rax = 0xFFFFFFFF;
                }
            } else {
                (*r).rax = 0xFFFFFFFF;
            }
        }
        // ─── 70: getgid() ─────────────────────────────────────
        70 => {
            let current = crate::scheduler::krust_sched_current();
            if !current.is_null() {
                (*r).rax = (*current).gid as u64;
            } else {
                (*r).rax = 0;
            }
        }
        // ─── 71: setgid(gid) ──────────────────────────────────
        71 => {
            let current = crate::scheduler::krust_sched_current();
            if !current.is_null() {
                if (*current).uid == 0 {
                    (*current).gid = arg1 as u16;
                    (*r).rax = 0;
                } else {
                    (*r).rax = 0xFFFFFFFF;
                }
            } else {
                (*r).rax = 0xFFFFFFFF;
            }
        }
        // ─── 72: geteuid() ────────────────────────────────────
        72 => {
            let current = crate::scheduler::krust_sched_current();
            if !current.is_null() {
                (*r).rax = (*current).uid as u64;
            } else {
                (*r).rax = 0;
            }
        }
        // ─── 73: getegid() ────────────────────────────────────
        73 => {
            let current = crate::scheduler::krust_sched_current();
            if !current.is_null() {
                (*r).rax = (*current).gid as u64;
            } else {
                (*r).rax = 0;
            }
        }
        // ─── 74: fchmod(fd, mode) ─────────────────────────────
        74 => {
            let current = crate::scheduler::krust_sched_current();
            if current.is_null() || (*current).uid != 0 {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let fd = arg1 as i32;
                let mode = arg2 as u16;
                if fd < 0 || fd >= MAX_FDS as i32 || !fd_ref()[fd as usize].used {
                    (*r).rax = 0xFFFFFFFF;
                } else {
                    let node = fd_ref()[fd as usize].node;
                    if !node.is_null() {
                        (*node).mode = ((*node).mode & 0xF000) | (mode & 0x0FFF);
                        (*r).rax = 0;
                    } else {
                        (*r).rax = 0xFFFFFFFF;
                    }
                }
            }
        }
        // ─── 75: fchown(fd, uid, gid) ─────────────────────────
        75 => {
            let current = crate::scheduler::krust_sched_current();
            if current.is_null() || (*current).uid != 0 {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let fd = arg1 as i32;
                let uid = arg2 as u16;
                let gid = arg3 as u16;
                if fd < 0 || fd >= MAX_FDS as i32 || !fd_ref()[fd as usize].used {
                    (*r).rax = 0xFFFFFFFF;
                } else {
                    let node = fd_ref()[fd as usize].node;
                    if !node.is_null() {
                        (*node).uid = uid;
                        (*node).gid = gid;
                        (*r).rax = 0;
                    } else {
                        (*r).rax = 0xFFFFFFFF;
                    }
                }
            }
        }
        // ─── 76: chmod(path, mode) ────────────────────────────
        76 => {
            let current = crate::scheduler::krust_sched_current();
            if current.is_null() || (*current).uid != 0 {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let mut path_buf = [0u8; 256];
                if copy_string_from_user(path_buf.as_mut_ptr(), arg1 as *const u8, 256) < 0 {
                    (*r).rax = 0xFFFFFFFF;
                } else {
                    let node = crate::vfs::krust_vfs_resolve(path_buf.as_ptr());
                    if !node.is_null() {
                        (*node).mode = ((*node).mode & 0xF000) | (arg2 as u16 & 0x0FFF);
                        (*r).rax = 0;
                    } else {
                        (*r).rax = 0xFFFFFFFF;
                    }
                }
            }
        }
        // ─── 77: chown(path, uid, gid) ────────────────────────
        77 => {
            let current = crate::scheduler::krust_sched_current();
            if current.is_null() || (*current).uid != 0 {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let mut path_buf = [0u8; 256];
                if copy_string_from_user(path_buf.as_mut_ptr(), arg1 as *const u8, 256) < 0 {
                    (*r).rax = 0xFFFFFFFF;
                } else {
                    let node = crate::vfs::krust_vfs_resolve(path_buf.as_ptr());
                    if !node.is_null() {
                        (*node).uid = arg2 as u16;
                        (*node).gid = arg3 as u16;
                        (*r).rax = 0;
                    } else {
                        (*r).rax = 0xFFFFFFFF;
                    }
                }
            }
        }
        // ─── 78: getdents(fd, buf, count) ─────────────────────
        78 => {
            let fd = arg1 as i32;
            let buf_ptr = arg2 as *mut u8;
            let count = arg3 as usize;
            if fd < 0 || fd >= MAX_FDS as i32 || !fd_ref()[fd as usize].used || buf_ptr.is_null() || !is_user_range(buf_ptr, count as u64) {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let node = fd_ref()[fd as usize].node;
                if node.is_null() || (*node).type_ != 1 {
                    (*r).rax = 0xFFFFFFFF;
                } else {
                    // Iterate children starting from offset
                    let mut offset = fd_ref()[fd as usize].offset as usize;
                    let mut child = (*node).children;
                    let mut pos = 0usize;
                    let mut written = 0u32;
                    while !child.is_null() {
                        if pos >= offset {
                            let name_len = crate::klib::krust_strlen((*child).name.as_ptr());
                            // Linux linux_dirent64: ino(8) + offset(8) + reclen(2) + type(1) + name
                            let reclen = ((19 + name_len + 1 + 7) & !7) as u16;
                            if pos + reclen as usize > offset + count {
                                break;
                            }
                            core::arch::asm!("stac");
                            let d_ino = (child as u64) as u64;
                            ptr::write_volatile(buf_ptr.add(pos) as *mut u64, d_ino);
                            ptr::write_volatile(buf_ptr.add(pos + 8) as *mut u64, (pos as u64) + 1);
                            ptr::write_volatile(buf_ptr.add(pos + 16) as *mut u16, reclen);
                            ptr::write_volatile(buf_ptr.add(pos + 18), (*child).type_);
                            for i in 0..name_len {
                                ptr::write_volatile(buf_ptr.add(pos + 19 + i), (*child).name[i]);
                            }
                            ptr::write_volatile(buf_ptr.add(pos + 19 + name_len), 0);
                            core::arch::asm!("clac");
                            pos += reclen as usize;
                            written += reclen as u32;
                        } else {
                            pos += 1;
                        }
                        child = (*child).next;
                    }
                    fd_ref()[fd as usize].offset = (pos) as u32;
                    (*r).rax = if written > 0 { written as u64 } else { 0 };
                }
            }
        }
        // ─── 79: symlink(target, linkpath) ──────────────────
        79 => {
            let mut target_buf = [0u8; 256];
            let mut linkpath_buf = [0u8; 256];
            if copy_string_from_user(target_buf.as_mut_ptr(), arg1 as *const u8, 256) < 0 {
                (*r).rax = 0xFFFFFFFF;
            } else if copy_string_from_user(linkpath_buf.as_mut_ptr(), arg2 as *const u8, 256) < 0 {
                (*r).rax = 0xFFFFFFFF;
            } else {
                (*r).rax = if crate::vfs::krust_vfs_symlink(target_buf.as_ptr(), linkpath_buf.as_ptr()) == 0 {
                    0
                } else {
                    0xFFFFFFFF
                };
            }
        }
        // ─── 80: readlink(path, buf, bufsize) ──────────────
        80 => {
            let mut path_buf = [0u8; 256];
            if copy_string_from_user(path_buf.as_mut_ptr(), arg1 as *const u8, 256) < 0 {
                (*r).rax = 0xFFFFFFFF;
            } else if arg2 == 0 || arg3 == 0 || !is_user_range(arg2 as *const u8, arg3) {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let result = crate::vfs::krust_vfs_readlink(path_buf.as_ptr(), arg2 as *mut u8, arg3 as u32);
                (*r).rax = if result >= 0 { result as u64 } else { 0xFFFFFFFF };
            }
        }
        // ─── 81: system(cmd_string) — fork+exec shell ──────
        81 => {
            (*r).rax = 0xFFFFFFFF;
        }
        // ─── 82: setenv(name, value) ──────────────────────
        82 => {
            let mut name_buf = [0u8; 64];
            let mut val_buf = [0u8; 256];
            if copy_string_from_user(name_buf.as_mut_ptr(), arg1 as *const u8, 64) < 0
                || copy_string_from_user(val_buf.as_mut_ptr(), arg2 as *const u8, 256) < 0
            {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let current = crate::scheduler::krust_sched_current();
                if current.is_null() {
                    (*r).rax = 0xFFFFFFFF;
                } else {
                    let t = &mut *current;
                    let mut found = false;
                    for i in 0..t.env_count as usize {
                        if !t.env_keys[i].is_null() {
                            let mut match_name = true;
                            for j in 0..64 {
                                let a = core::ptr::read_volatile(t.env_keys[i].add(j));
                                let b = name_buf[j];
                                if a != b || (a == 0 && b == 0) { if a != b { match_name = false; } break; }
                            }
                            if match_name {
                                if !t.env_vals[i].is_null() { crate::heap::krust_free(t.env_vals[i]); }
                                let vlen = val_buf.iter().position(|&c| c == 0).unwrap_or(255);
                                let vptr = crate::heap::krust_malloc(vlen as u32 + 1);
                                if !vptr.is_null() {
                                    crate::klib::krust_memcpy(vptr, val_buf.as_ptr(), vlen + 1);
                                }
                                t.env_vals[i] = vptr;
                                found = true;
                                break;
                            }
                        }
                    }
                    if !found && t.env_count < 32 {
                        let idx = t.env_count as usize;
                        let nlen = name_buf.iter().position(|&c| c == 0).unwrap_or(63);
                        let vlen = val_buf.iter().position(|&c| c == 0).unwrap_or(255);
                        let nptr = crate::heap::krust_malloc(nlen as u32 + 1);
                        let vptr = crate::heap::krust_malloc(vlen as u32 + 1);
                        if !nptr.is_null() && !vptr.is_null() {
                            crate::klib::krust_memcpy(nptr, name_buf.as_ptr(), nlen + 1);
                            crate::klib::krust_memcpy(vptr, val_buf.as_ptr(), vlen + 1);
                            t.env_keys[idx] = nptr;
                            t.env_vals[idx] = vptr;
                            t.env_count += 1;
                            (*r).rax = 0;
                        } else {
                            if !nptr.is_null() { crate::heap::krust_free(nptr); }
                            if !vptr.is_null() { crate::heap::krust_free(vptr); }
                            (*r).rax = 0xFFFFFFFF;
                        }
                    } else {
                        (*r).rax = 0;
                    }
                }
            }
        }
        // ─── 83: getenv(name, buf, bufsize) ───────────────
        83 => {
            let mut name_buf = [0u8; 64];
            if copy_string_from_user(name_buf.as_mut_ptr(), arg1 as *const u8, 64) < 0 {
                (*r).rax = 0xFFFFFFFF;
            } else if arg2 == 0 || arg3 == 0 || !is_user_range(arg2 as *const u8, arg3) {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let current = crate::scheduler::krust_sched_current();
                if current.is_null() {
                    (*r).rax = 0xFFFFFFFF;
                } else {
                    let t = &*current;
                    let mut result: i64 = 0xFFFFFFFF;
                    for i in 0..t.env_count as usize {
                        if !t.env_keys[i].is_null() {
                            let mut match_name = true;
                            for j in 0..64 {
                                let a = core::ptr::read_volatile(t.env_keys[i].add(j));
                                let b = name_buf[j];
                                if a != b || (a == 0 && b == 0) { if a != b { match_name = false; } break; }
                            }
                            if match_name {
                                if !t.env_vals[i].is_null() {
                                    let mut vlen: usize = 0;
                                    while core::ptr::read_volatile(t.env_vals[i].add(vlen)) != 0 { vlen += 1; }
                                    let copy_len = core::cmp::min(vlen, arg3 as usize - 1);
                                    core::arch::asm!("stac");
                                    for j in 0..copy_len {
                                        let c = core::ptr::read_volatile((arg2 as *mut u8).add(j));
                                        core::ptr::write_volatile((arg2 as *mut u8).add(j), c);
                                    }
                                    core::ptr::write_volatile((arg2 as *mut u8).add(copy_len), 0u8);
                                    core::arch::asm!("clac");
                                    result = copy_len as i64;
                                } else {
                                    core::arch::asm!("stac");
                                    core::ptr::write_volatile(arg2 as *mut u8, 0u8);
                                    core::arch::asm!("clac");
                                    result = 0;
                                }
                                break;
                            }
                        }
                    }
                    (*r).rax = result as u64;
                }
            }
        }
        // ─── 84: ps(buf, bufsize) — list all processes ──────
        84 => {
            if arg1 == 0 || arg3 == 0 || !is_user_range(arg1 as *const u8, arg3) {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let mut kbuf = [0u8; 4096];
                let len = crate::fs::procfs::proc_list_processes(&mut kbuf);
                let copy_len = core::cmp::min(len, arg3 as usize - 1);
                core::arch::asm!("stac");
                for i in 0..copy_len {
                    core::ptr::write_volatile((arg1 as *mut u8).add(i), kbuf[i]);
                }
                core::ptr::write_volatile((arg1 as *mut u8).add(copy_len), 0u8);
                core::arch::asm!("clac");
                (*r).rax = copy_len as u64;
            }
        }
        // ─── 85: free_info(total_ptr, free_ptr) — memory info ─
        85 => {
            if arg1 != 0 && arg2 != 0 {
                let mut total_kb: u32 = 0;
                let mut free_kb: u32 = 0;
                crate::mm::mm::krust_mm_info(&mut total_kb, &mut free_kb);
                core::arch::asm!("stac");
                core::ptr::write_volatile(arg1 as *mut u32, total_kb);
                core::ptr::write_volatile(arg2 as *mut u32, free_kb);
                core::arch::asm!("clac");
                (*r).rax = 0;
            } else {
                (*r).rax = 0xFFFFFFFF;
            }
        }
        // ─── 86: list_env(buf, bufsize) — list all env vars ──
        86 => {
            if arg1 == 0 || arg3 == 0 || !is_user_range(arg1 as *const u8, arg3) {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let current = crate::scheduler::krust_sched_current();
                if current.is_null() {
                    (*r).rax = 0xFFFFFFFF;
                } else {
                    let t = &*current;
                    let mut kbuf = [0u8; 4096];
                    let mut pos = 0;
                    for i in 0..t.env_count as usize {
                        if !t.env_keys[i].is_null() {
                            // key=value\n
                            let mut j = 0;
                            while j < 64 {
                                let c = core::ptr::read_volatile(t.env_keys[i].add(j));
                                if c == 0 || pos >= kbuf.len() - 2 { break; }
                                kbuf[pos] = c; pos += 1; j += 1;
                            }
                            if pos < kbuf.len() { kbuf[pos] = b'='; pos += 1; }
                            if !t.env_vals[i].is_null() {
                                let mut j = 0;
                                while j < 256 {
                                    let c = core::ptr::read_volatile(t.env_vals[i].add(j));
                                    if c == 0 || pos >= kbuf.len() - 2 { break; }
                                    kbuf[pos] = c; pos += 1; j += 1;
                                }
                            }
                            if pos < kbuf.len() { kbuf[pos] = b'\n'; pos += 1; }
                        }
                    }
                    let copy_len = core::cmp::min(pos, arg3 as usize - 1);
                    core::arch::asm!("stac");
                    for i in 0..copy_len {
                        core::ptr::write_volatile((arg1 as *mut u8).add(i), kbuf[i]);
                    }
                    core::ptr::write_volatile((arg1 as *mut u8).add(copy_len), 0u8);
                    core::arch::asm!("clac");
                    (*r).rax = copy_len as u64;
                }
            }
        }
        // ─── 87: proc_info(pid, buf, bufsize) — per-process info ──
        87 => {
            let target_pid = arg1 as u32;
            if arg2 == 0 || arg3 == 0 || !is_user_range(arg2 as *const u8, arg3) {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let next_id = crate::scheduler::krust_sched_get_next_id();
                let task = crate::scheduler::krust_sched_get_task(target_pid);
                if task.is_null() || target_pid >= next_id {
                    (*r).rax = 0xFFFFFFFF;
                } else {
                    let t = &*task;
                    let mut kbuf = [0u8; 1024];
                    let mut pos = 0;
                    let state_str: [u8; 9] = match t.state {
                        crate::scheduler::TaskState::RUNNING | crate::scheduler::TaskState::READY => *b"running\0\0",
                        crate::scheduler::TaskState::BLOCKED | crate::scheduler::TaskState::WAITING => *b"sleeping\0",
                        crate::scheduler::TaskState::EXITED => *b"zombie\0\0\0",
                    };
                    let state_len = match t.state {
                        crate::scheduler::TaskState::RUNNING | crate::scheduler::TaskState::READY => 7usize,
                        crate::scheduler::TaskState::BLOCKED | crate::scheduler::TaskState::WAITING => 8usize,
                        crate::scheduler::TaskState::EXITED => 6usize,
                    };
                    crate::fs::procfs::append_str(&mut kbuf, &mut pos, b"Name:  \0");
                    crate::fs::procfs::append_str(&mut kbuf, &mut pos, &state_str[..state_len]);
                    crate::fs::procfs::append_str(&mut kbuf, &mut pos, b"\n\0");
                    crate::fs::procfs::append_str(&mut kbuf, &mut pos, b"Pid:   \0");
                    crate::fs::procfs::append_u32(&mut kbuf, &mut pos, t.id);
                    crate::fs::procfs::append_str(&mut kbuf, &mut pos, b"\n\0");
                    crate::fs::procfs::append_str(&mut kbuf, &mut pos, b"PPid:  \0");
                    crate::fs::procfs::append_u32(&mut kbuf, &mut pos, t.ppid);
                    crate::fs::procfs::append_str(&mut kbuf, &mut pos, b"\n\0");
                    crate::fs::procfs::append_str(&mut kbuf, &mut pos, b"Uid:   \0");
                    crate::fs::procfs::append_u32(&mut kbuf, &mut pos, t.uid as u32);
                    crate::fs::procfs::append_str(&mut kbuf, &mut pos, b"\n\0");
                    crate::fs::procfs::append_str(&mut kbuf, &mut pos, b"Gid:   \0");
                    crate::fs::procfs::append_u32(&mut kbuf, &mut pos, t.gid as u32);
                    crate::fs::procfs::append_str(&mut kbuf, &mut pos, b"\n\0");
                    crate::fs::procfs::append_str(&mut kbuf, &mut pos, b"State: \0");
                    let state_char = match t.state {
                        crate::scheduler::TaskState::RUNNING | crate::scheduler::TaskState::READY => b'R',
                        crate::scheduler::TaskState::BLOCKED | crate::scheduler::TaskState::WAITING => b'S',
                        crate::scheduler::TaskState::EXITED => b'Z',
                    };
                    kbuf[pos] = state_char; pos += 1;
                    crate::fs::procfs::append_str(&mut kbuf, &mut pos, b"\n\0");
                    crate::fs::procfs::append_str(&mut kbuf, &mut pos, b"Prio:  \0");
                    crate::fs::procfs::append_u32(&mut kbuf, &mut pos, t.priority);
                    crate::fs::procfs::append_str(&mut kbuf, &mut pos, b"\n\0");
                    let copy_len = core::cmp::min(pos, arg3 as usize - 1);
                    core::arch::asm!("stac");
                    for i in 0..copy_len {
                        core::ptr::write_volatile((arg2 as *mut u8).add(i), kbuf[i]);
                    }
                    core::ptr::write_volatile((arg2 as *mut u8).add(copy_len), 0u8);
                    core::arch::asm!("clac");
                    (*r).rax = copy_len as u64;
                }
            }
        }
        // ─── 88: setpgid(pid, pgid) ──
        88 => {
            let target_pid = arg1 as u32;
            let new_pgid = arg2 as u32;
            let current = crate::scheduler::krust_sched_current();
            if current.is_null() {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let actual_pid = if target_pid == 0 { (*current).id } else { target_pid };
                let actual_pgid = if new_pgid == 0 { actual_pid } else { new_pgid };
                let task = crate::scheduler::krust_sched_get_task(actual_pid);
                if task.is_null() {
                    (*r).rax = 0xFFFFFFFF;
                } else {
                    (*task).pgid = actual_pgid;
                    (*r).rax = 0;
                }
            }
        }
        // ─── 89: getpgid(pid) ──
        89 => {
            let target_pid = arg1 as u32;
            let current = crate::scheduler::krust_sched_current();
            if current.is_null() {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let actual_pid = if target_pid == 0 { (*current).id } else { target_pid };
                let task = crate::scheduler::krust_sched_get_task(actual_pid);
                if task.is_null() {
                    (*r).rax = 0xFFFFFFFF;
                } else {
                    (*r).rax = (*task).pgid as u64;
                }
            }
        }
        // ─── 90: getsid(pid) ──
        90 => {
            let target_pid = arg1 as u32;
            let current = crate::scheduler::krust_sched_current();
            if current.is_null() {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let actual_pid = if target_pid == 0 { (*current).id } else { target_pid };
                let task = crate::scheduler::krust_sched_get_task(actual_pid);
                if task.is_null() {
                    (*r).rax = 0xFFFFFFFF;
                } else {
                    (*r).rax = (*task).sid as u64;
                }
            }
        }
        // ─── 91: setsid() ──
        91 => {
            let current = crate::scheduler::krust_sched_current();
            if current.is_null() {
                (*r).rax = 0xFFFFFFFF;
            } else {
                let new_sid = (*current).id;
                (*current).sid = new_sid;
                (*current).pgid = new_sid;
                (*r).rax = new_sid as u64;
            }
        }
        // ─── 92: getpgrp() ──
        92 => {
            let current = crate::scheduler::krust_sched_current();
            if current.is_null() {
                (*r).rax = 0xFFFFFFFF;
            } else {
                (*r).rax = (*current).pgid as u64;
            }
        }
        _ => {
            (*r).rax = 0xFFFFFFFF;
        }
    }
}
