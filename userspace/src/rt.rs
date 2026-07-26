use core::fmt::Write;
use core::panic::PanicInfo;

// --- Intrinsics / builtins ---

#[no_mangle]
pub unsafe extern "C" fn strlen(s: *const u8) -> usize {
    let mut i = 0;
    while core::ptr::read_volatile(s.add(i)) != 0 { i += 1; }
    i
}

#[no_mangle]
pub unsafe extern "C" fn memcmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    for i in 0..n {
        let a = core::ptr::read_volatile(s1.add(i));
        let b = core::ptr::read_volatile(s2.add(i));
        if a != b { return a as i32 - b as i32; }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn strcmp(s1: *const u8, s2: *const u8) -> i32 {
    let mut i = 0;
    loop {
        let a = core::ptr::read_volatile(s1.add(i));
        let b = core::ptr::read_volatile(s2.add(i));
        if a != b { return a as i32 - b as i32; }
        if a == 0 { break; }
        i += 1;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    for i in 0..n {
        core::ptr::write_volatile(dest.add(i), core::ptr::read_volatile(src.add(i)));
    }
    dest
}

#[no_mangle]
pub unsafe extern "C" fn memset(s: *mut u8, c: i32, n: usize) -> *mut u8 {
    for i in 0..n {
        core::ptr::write_volatile(s.add(i), c as u8);
    }
    s
}

#[no_mangle]
pub unsafe extern "C" fn memmove(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    if dest < src as *mut u8 {
        for i in 0..n {
            core::ptr::write_volatile(dest.add(i), core::ptr::read_volatile(src.add(i)));
        }
    } else {
        let mut i = n;
        while i > 0 {
            i -= 1;
            core::ptr::write_volatile(dest.add(i), core::ptr::read_volatile(src.add(i)));
        }
    }
    dest
}

// --- Panic handler ---

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let _ = Stdout.write_fmt(format_args!("*** PANIC ***\n"));
    let _ = Stdout.write_fmt(format_args!("{}", info.message()));
    let _ = Stdout.write_str("\n");
    if let Some(loc) = info.location() {
        let _ = Stdout.write_fmt(format_args!("  at {}:{}\n", loc.file(), loc.line()));
    }
    sys_exit();
}

// --- Syscalls ---

#[inline(never)]
unsafe fn syscall_2(num: u32, arg1: u32, arg2: u32) -> u32 {
    let result: u32;
    let _tmp: u64;
    core::arch::asm!(
        "mov {_tmp}, rbx",
        "mov ebx, {arg1:e}",
        "int 0x80",
        "mov rbx, {_tmp}",
        in("eax") num,
        arg1 = in(reg) arg1,
        in("ecx") arg2,
        _tmp = out(reg) _tmp,
        lateout("eax") result,
        options(nostack)
    );
    result
}

#[inline(never)]
unsafe fn syscall_3(num: u32, arg1: u32, arg2: u32, arg3: u32) -> u32 {
    let result: u32;
    let _tmp: u64;
    core::arch::asm!(
        "mov {_tmp}, rbx",
        "mov ebx, {arg1:e}",
        "int 0x80",
        "mov rbx, {_tmp}",
        in("eax") num,
        arg1 = in(reg) arg1,
        in("ecx") arg2,
        in("edx") arg3,
        _tmp = out(reg) _tmp,
        lateout("eax") result,
        options(nostack)
    );
    result
}

#[inline(never)]
unsafe fn syscall_5(num: u32, arg1: u32, arg2: u32, arg3: u32, arg4: u32, arg5: u32) -> u32 {
    let result: u32;
    let _tmp: u64;
    core::arch::asm!(
        "mov {_tmp}, rbx",
        "mov ebx, {arg1:e}",
        "int 0x80",
        "mov rbx, {_tmp}",
        in("eax") num,
        arg1 = in(reg) arg1,
        in("ecx") arg2,
        in("edx") arg3,
        in("r10") arg4,
        in("r8") arg5,
        _tmp = out(reg) _tmp,
        lateout("eax") result,
        options(nostack)
    );
    result
}

pub fn sys_write(data: &[u8]) {
    unsafe { syscall_3(0, data.as_ptr() as u32, data.len() as u32, 0); }
}

pub fn sys_exit() -> ! {
    sys_exit_code(0)
}

pub fn sys_exit_code(code: u32) -> ! {
    unsafe { syscall_3(1, code, 0, 0); }
    loop { core::hint::spin_loop() }
}

pub fn sys_sleep(ms: u32) {
    unsafe { syscall_3(2, ms, 0, 0); }
}

pub fn sys_yield() {
    unsafe { syscall_3(3, 0, 0, 0); }
}

pub fn sys_open(path: &str) -> isize {
    let bytes = path.as_bytes();
    if bytes.len() >= 254 { return -1; }
    let mut cpath = [0u8; 256];
    cpath[..bytes.len()].copy_from_slice(bytes);
    let r = unsafe { syscall_2(4, &cpath as *const _ as u32, 0) };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}

pub fn sys_read(fd: isize, buf: &mut [u8]) -> isize {
    let r = unsafe { syscall_3(5, fd as u32, buf.as_mut_ptr() as u32, buf.len() as u32) };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}

pub fn sys_close(fd: isize) -> isize {
    let r = unsafe { syscall_2(6, fd as u32, 0) };
    if r == 0xFFFFFFFF { -1 } else { 0 }
}

pub fn sys_readdir(path: &str, buf: &mut [u8]) -> isize {
    // Ensure null-terminated path by copying into a stack buffer
    let bytes = path.as_bytes();
    if bytes.len() >= 254 { return -1; }
    let mut cpath = [0u8; 256];
    cpath[..bytes.len()].copy_from_slice(bytes);
    // cpath is already zeroed, so it's null-terminated
    let r = unsafe { syscall_3(7, &cpath as *const _ as u32, buf.as_mut_ptr() as u32, buf.len() as u32) };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}

// --- New syscalls (filesystem operations) ---

pub fn sys_write_file(path: &str, data: &[u8]) -> isize {
    let bytes = path.as_bytes();
    if bytes.len() >= 254 { return -1; }
    let mut cpath = [0u8; 256];
    cpath[..bytes.len()].copy_from_slice(bytes);
    let r = unsafe { syscall_3(8, &cpath as *const _ as u32, data.as_ptr() as u32, data.len() as u32) };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}

pub fn sys_mkdir(path: &str) -> isize {
    let bytes = path.as_bytes();
    if bytes.len() >= 254 { return -1; }
    let mut cpath = [0u8; 256];
    cpath[..bytes.len()].copy_from_slice(bytes);
    let r = unsafe { syscall_2(9, &cpath as *const _ as u32, 0) };
    if r == 0xFFFFFFFF { -1 } else { 0 }
}

pub fn sys_unlink(path: &str) -> isize {
    let bytes = path.as_bytes();
    if bytes.len() >= 254 { return -1; }
    let mut cpath = [0u8; 256];
    cpath[..bytes.len()].copy_from_slice(bytes);
    let r = unsafe { syscall_2(10, &cpath as *const _ as u32, 0) };
    if r == 0xFFFFFFFF { -1 } else { 0 }
}

pub fn sys_rmdir(path: &str) -> isize {
    let bytes = path.as_bytes();
    if bytes.len() >= 254 { return -1; }
    let mut cpath = [0u8; 256];
    cpath[..bytes.len()].copy_from_slice(bytes);
    let r = unsafe { syscall_2(11, &cpath as *const _ as u32, 0) };
    if r == 0xFFFFFFFF { -1 } else { 0 }
}

pub fn sys_rename(old_path: &str, new_path: &str) -> isize {
    let old_bytes = old_path.as_bytes();
    let new_bytes = new_path.as_bytes();
    if old_bytes.len() >= 254 || new_bytes.len() >= 254 { return -1; }
    let mut old_cpath = [0u8; 256];
    old_cpath[..old_bytes.len()].copy_from_slice(old_bytes);
    let mut new_cpath = [0u8; 256];
    new_cpath[..new_bytes.len()].copy_from_slice(new_bytes);
    let r = unsafe { syscall_3(12, &old_cpath as *const _ as u32, &new_cpath as *const _ as u32, 0) };
    if r == 0xFFFFFFFF { -1 } else { 0 }
}

#[repr(C)]
pub struct FileStat {
    pub type_: u8,
    pub size: u32,
    pub name: [u8; 64],
    pub uid: u16,
    pub gid: u16,
    pub mode: u16,
}

pub fn sys_stat(path: &str, st: &mut FileStat) -> isize {
    let bytes = path.as_bytes();
    if bytes.len() >= 254 { return -1; }
    let mut cpath = [0u8; 256];
    cpath[..bytes.len()].copy_from_slice(bytes);
    let r = unsafe { syscall_3(13, &cpath as *const _ as u32, st as *mut _ as u32, core::mem::size_of::<FileStat>() as u32) };
    if r == 0xFFFFFFFF { -1 } else { 0 }
}

// --- First-fit free-list allocator ---

static mut HEAP_BASE: *mut u8 = core::ptr::null_mut();
static mut HEAP_END: *mut u8 = core::ptr::null_mut();

#[repr(C)]
struct BlockHeader {
    size: u32,     // total block size including header (always 8-byte aligned)
    free: bool,
    next: *mut BlockHeader,
}

const HDR_SIZE: usize = core::mem::size_of::<BlockHeader>();
const ALIGN8: usize = !7usize;

static mut FREE_LIST: *mut BlockHeader = core::ptr::null_mut();
static mut ALLOC_PTR: *mut u8 = core::ptr::null_mut(); // bump pointer for new blocks

unsafe fn align_up(v: usize, a: usize) -> usize { (v + a - 1) & !(a - 1) }

unsafe fn extend_heap(need: usize) -> *mut BlockHeader {
    let chunk = align_up(need + HDR_SIZE, 4096);
    if ALLOC_PTR.add(chunk) > HEAP_END { return core::ptr::null_mut(); }
    let blk = ALLOC_PTR as *mut BlockHeader;
    (*blk).size = chunk as u32;
    (*blk).free = true;
    (*blk).next = FREE_LIST;
    FREE_LIST = blk;
    ALLOC_PTR = ALLOC_PTR.add(chunk);
    blk
}

#[no_mangle]
pub unsafe extern "C" fn heap_init(base: *mut u8, size: usize) {
    HEAP_BASE = base;
    HEAP_END = base.add(size);
    ALLOC_PTR = base;
    FREE_LIST = core::ptr::null_mut();
    // Create one initial free block spanning the entire heap
    let blk = base as *mut BlockHeader;
    (*blk).size = size as u32;
    (*blk).free = true;
    (*blk).next = core::ptr::null_mut();
    FREE_LIST = blk;
    ALLOC_PTR = base.add(size);
}

#[no_mangle]
pub unsafe extern "C" fn malloc(size: usize) -> *mut u8 {
    if size == 0 { return core::ptr::null_mut(); }
    let need = align_up(size + HDR_SIZE, 8);
    // First-fit search
    let mut cur = FREE_LIST;
    while !cur.is_null() {
        if (*cur).free && (*cur).size as usize >= need {
            // Split block if remainder is large enough
            let remainder = (*cur).size as usize - need;
            if remainder >= HDR_SIZE + 8 {
                let new_blk = (cur as *mut u8).add(need) as *mut BlockHeader;
                (*new_blk).size = remainder as u32;
                (*new_blk).free = true;
                (*new_blk).next = (*cur).next;
                (*cur).next = new_blk;
                (*cur).size = need as u32;
            }
            (*cur).free = false;
            return (cur as *mut u8).add(HDR_SIZE);
        }
        cur = (*cur).next;
    }
    // No suitable block found, extend heap
    let blk = extend_heap(size);
    if blk.is_null() { return core::ptr::null_mut(); }
    (*blk).free = false;
    // Split if possible
    let total = (*blk).size as usize;
    if total > need + HDR_SIZE + 8 {
        let new_blk = (blk as *mut u8).add(need) as *mut BlockHeader;
        (*new_blk).size = (total - need) as u32;
        (*new_blk).free = true;
        (*new_blk).next = (*blk).next;
        (*blk).next = new_blk;
        (*blk).size = need as u32;
    }
    (blk as *mut u8).add(HDR_SIZE)
}

#[no_mangle]
pub unsafe extern "C" fn free(ptr: *mut u8) {
    if ptr.is_null() { return; }
    let hdr = (ptr.sub(HDR_SIZE)) as *mut BlockHeader;
    (*hdr).free = true;
    // Coalesce with next block if free
    let next = (*hdr).next;
    if !next.is_null() && (*next).free {
        (*hdr).size += (*next).size;
        (*hdr).next = (*next).next;
    }
    // Coalesce with previous block (linear scan, acceptable for hobby OS)
    let mut prev = FREE_LIST;
    while !prev.is_null() && (*prev).next != hdr {
        prev = (*prev).next;
    }
    if !prev.is_null() && prev != hdr && (*prev).free {
        (*prev).size += (*hdr).size;
        (*prev).next = (*hdr).next;
    }
}

#[no_mangle]
pub unsafe extern "C" fn realloc(ptr: *mut u8, size: usize) -> *mut u8 {
    if ptr.is_null() { return malloc(size); }
    if size == 0 { free(ptr); return core::ptr::null_mut(); }
    let hdr = (ptr.sub(HDR_SIZE)) as *mut BlockHeader;
    let old_usable = (*hdr).size as usize - HDR_SIZE;
    if old_usable >= size { return ptr; }
    let newp = malloc(size);
    if !newp.is_null() {
        core::ptr::copy_nonoverlapping(ptr, newp, old_usable);
        free(ptr);
    }
    newp
}

// --- stdin / getchar ---

pub fn sys_getchar() -> u8 {
    unsafe {
        let result: u32;
        core::arch::asm!("int 0x80", inout("eax") 14u32 => result, options(nostack));
        result as u8
    }
}

// --- Pipes ---

pub fn sys_pipe_create(fds: &mut [i32; 2]) -> isize {
    let r = unsafe { syscall_2(15, fds.as_mut_ptr() as u32, 0) };
    if r == 0xFFFFFFFF { -1 } else { 0 }
}

pub fn sys_pipe_read(fd: i32, buf: &mut [u8]) -> isize {
    let r = unsafe { syscall_3(16, fd as u32, buf.as_mut_ptr() as u32, buf.len() as u32) };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}

pub fn sys_pipe_write(fd: i32, buf: &[u8]) -> isize {
    let r = unsafe { syscall_3(17, fd as u32, buf.as_ptr() as u32, buf.len() as u32) };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}

pub fn sys_pipe_close(fd: i32) {
    unsafe { syscall_2(18, fd as u32, 0); }
}

pub fn sys_write_fd(fd: i32, data: &[u8]) -> isize {
    let r = unsafe { syscall_3(19, fd as u32, data.as_ptr() as u32, data.len() as u32) };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}

// --- System control ---

pub fn sys_clear() {
    unsafe { syscall_2(20, 0, 0); }
}

pub fn sys_system_info(buf: &mut [u8]) -> u32 {
    unsafe { syscall_3(21, buf.as_mut_ptr() as u32, buf.len() as u32, 0) }
}

pub fn sys_getcwd(buf: &mut [u8]) -> isize {
    let r = unsafe { syscall_3(25, buf.as_mut_ptr() as u32, buf.len() as u32, 0) };
    if r == 0xFFFFFFFF { -1 } else { 0 }
}

#[repr(C)]
pub struct RTCInfo {
    pub second: u8,
    pub minute: u8,
    pub hour: u8,
    pub day: u8,
    pub month: u8,
    pub year: u16,
}

pub fn sys_gettime(info: &mut RTCInfo) -> isize {
    let r = unsafe { syscall_2(27, info as *mut _ as u32, core::mem::size_of::<RTCInfo>() as u32) };
    if r == 0xFFFFFFFF { -1 } else { 0 }
}

// --- New syscalls: fork/execve/waitpid/signals ---

pub fn sys_fork() -> isize {
    unsafe {
        let result: u32;
        core::arch::asm!(
            "int 0x80",
            inout("eax") 28u32 => result,
            options(nostack)
        );
        if result == 0xFFFFFFFF { -1 } else { result as isize }
    }
}

pub fn sys_execve(path: &str, argv: &[*const u8]) -> isize {
    let bytes = path.as_bytes();
    if bytes.len() >= 254 { return -1; }
    let mut cpath = [0u8; 256];
    cpath[..bytes.len()].copy_from_slice(bytes);
    let arg1 = &cpath as *const _ as u32;
    let arg2 = argv.len() as u32;
    let arg3 = argv.as_ptr() as u32;
    unsafe {
        let result: u32;
        let _tmp: u64;
        core::arch::asm!(
            "mov {_tmp}, rbx",
            "mov ebx, {arg1:e}",
            "int 0x80",
            "mov rbx, {_tmp}",
            in("eax") 29u32,
            arg1 = in(reg) arg1,
            in("ecx") arg2,
            in("edx") arg3,
            _tmp = out(reg) _tmp,
            lateout("eax") result,
            options(nostack)
        );
        if result == 0xFFFFFFFF { -1 } else { result as isize }
    }
}

pub fn sys_waitpid(pid: isize, status: &mut i32) -> isize {
    let arg1 = pid as u32;
    let arg2 = status as *mut _ as u32;
    unsafe {
        let result: u32;
        let _tmp: u64;
        core::arch::asm!(
            "mov {_tmp}, rbx",
            "mov ebx, {arg1:e}",
            "xor edx, edx",
            "int 0x80",
            "mov rbx, {_tmp}",
            in("eax") 30u32,
            arg1 = in(reg) arg1,
            in("ecx") arg2,
            _tmp = out(reg) _tmp,
            lateout("eax") result,
            lateout("edx") _,
            options(nostack)
        );
        if result == 0xFFFFFFFF { -1 } else { result as isize }
    }
}

pub fn sys_getpid() -> isize {
    unsafe {
        let result: u32;
        core::arch::asm!(
            "int 0x80",
            inout("eax") 31u32 => result,
            options(nostack)
        );
        result as isize
    }
}

pub fn sys_getppid() -> isize {
    unsafe {
        let result: u32;
        core::arch::asm!(
            "int 0x80",
            inout("eax") 35u32 => result,
            options(nostack)
        );
        result as isize
    }
}

pub fn sys_kill(pid: isize, sig: i32) -> isize {
    let arg1 = pid as u32;
    let arg2 = sig as u32;
    unsafe {
        let result: u32;
        let _tmp: u64;
        core::arch::asm!(
            "mov {_tmp}, rbx",
            "mov ebx, {arg1:e}",
            "int 0x80",
            "mov rbx, {_tmp}",
            in("eax") 32u32,
            arg1 = in(reg) arg1,
            in("ecx") arg2,
            _tmp = out(reg) _tmp,
            lateout("eax") result,
            options(nostack)
        );
        if result == 0xFFFFFFFF { -1 } else { 0 }
    }
}

pub fn sys_sigaction(sig: i32, handler: u32) -> isize {
    let arg1 = sig as u32;
    unsafe {
        let result: u32;
        let _tmp: u64;
        core::arch::asm!(
            "mov {_tmp}, rbx",
            "mov ebx, {arg1:e}",
            "int 0x80",
            "mov rbx, {_tmp}",
            in("eax") 33u32,
            arg1 = in(reg) arg1,
            in("ecx") handler,
            _tmp = out(reg) _tmp,
            lateout("eax") result,
            options(nostack)
        );
        if result == 0xFFFFFFFF { -1 } else { 0 }
    }
}

pub fn sys_sigreturn() -> isize {
    unsafe {
        let result: u32;
        core::arch::asm!(
            "int 0x80",
            inout("eax") 34u32 => result,
            options(nostack)
        );
        result as isize
    }
}

pub fn sys_chdir(path: &str) -> isize {
    let bytes = path.as_bytes();
    if bytes.len() >= 254 { return -1; }
    let mut cpath = [0u8; 256];
    cpath[..bytes.len()].copy_from_slice(bytes);
    let r = unsafe { syscall_2(26, &cpath as *const _ as u32, 0) };
    if r == 0xFFFFFFFF { -1 } else { 0 }
}

pub fn sys_open_write(path: &str) -> isize {
    let bytes = path.as_bytes();
    if bytes.len() >= 254 { return -1; }
    let mut cpath = [0u8; 256];
    cpath[..bytes.len()].copy_from_slice(bytes);
    let r = unsafe { syscall_2(24, &cpath as *const _ as u32, 0) };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}

pub fn sys_reboot() {
    unsafe { syscall_2(22, 0, 0); }
}

pub fn sys_poweroff() {
    unsafe { syscall_2(23, 0, 0); }
}

pub fn sys_dup2(oldfd: i32, newfd: i32) -> isize {
    let arg1 = oldfd as u32;
    let arg2 = newfd as u32;
    unsafe {
        let result: u32;
        let _tmp: u64;
        core::arch::asm!(
            "mov {_tmp}, rbx",
            "mov ebx, {arg1:e}",
            "int 0x80",
            "mov rbx, {_tmp}",
            in("eax") 61u32,
            arg1 = in(reg) arg1,
            in("ecx") arg2,
            _tmp = out(reg) _tmp,
            lateout("eax") result,
            options(nostack)
        );
        if result == 0xFFFFFFFF { -1 } else { result as isize }
    }
}

// --- Helper: get arg string ---

pub unsafe fn arg_at(argv: *const *const u8, index: usize) -> &'static str {
    let ptr = *argv.add(index);
    let mut len = 0;
    while *ptr.add(len) != 0 { len += 1; }
    core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
}

// --- I/O ---

pub struct Stdout;

impl Write for Stdout {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        sys_write(s.as_bytes());
        Ok(())
    }
}

#[allow(unused_macros)]
macro_rules! print {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = Stdout.write_fmt(format_args!($($arg)*));
    }};
}

#[allow(unused_macros)]
macro_rules! println {
    () => { print!("\n"); };
    ($($arg:tt)*) => {{
        print!($($arg)*);
        print!("\n");
    }};
}

// --- Network syscalls ---

pub fn sys_socket(sock_type: u32) -> i32 {
    let r = unsafe { syscall_3(55, sock_type, 0, 0) };
    if r == 0xFFFFFFFF { -1 } else { r as i32 }
}

pub fn sys_connect(sockfd: i32, ip: &[u8; 4], port: u16) -> i32 {
    let r = unsafe { syscall_3(56, sockfd as u32, ip.as_ptr() as u32, port as u32) };
    if r == 0xFFFFFFFF { -1 } else { r as i32 }
}

pub fn sys_sendto(sockfd: i32, data: &[u8], ip: &[u8; 4], port: u16) -> i32 {
    let r = unsafe {
        syscall_5(57, sockfd as u32, data.as_ptr() as u32, data.len() as u32, ip.as_ptr() as u32, port as u32)
    };
    if r == 0xFFFFFFFF { -1 } else { r as i32 }
}

pub struct RecvInfo {
    pub src_ip: [u8; 4],
    pub src_port: u16,
}

pub fn sys_recvfrom(sockfd: i32, buf: &mut [u8]) -> Result<(usize, RecvInfo), i32> {
    let mut info = [0u8; 8];
    let r = unsafe {
        syscall_5(58, sockfd as u32, buf.as_mut_ptr() as u32, buf.len() as u32, info.as_mut_ptr() as u32, 0)
    };
    if r == 0xFFFFFFFF {
        Err(-1)
    } else {
        let n = r as usize;
        let recv_info = RecvInfo {
            src_ip: [info[0], info[1], info[2], info[3]],
            src_port: ((info[4] as u16) << 8) | (info[5] as u16),
        };
        Ok((n, recv_info))
    }
}

pub fn sys_bind(sockfd: i32, port: u16) -> i32 {
    let r = unsafe { syscall_3(59, sockfd as u32, port as u32, 0) };
    if r == 0xFFFFFFFF { -1 } else { r as i32 }
}

pub fn sys_close_socket(sockfd: i32) -> i32 {
    let r = unsafe { syscall_3(60, sockfd as u32, 0, 0) };
    if r == 0xFFFFFFFF { -1 } else { r as i32 }
}

pub fn sys_listen(sockfd: i32, backlog: i32) -> i32 {
    let r = unsafe { syscall_3(62, sockfd as u32, backlog as u32, 0) };
    if r == 0xFFFFFFFF { -1 } else { r as i32 }
}

pub fn sys_accept(sockfd: i32, client_ip: Option<&mut [u8; 4]>, client_port: Option<&mut u16>) -> i32 {
    let ip_ptr = client_ip.map(|p| p.as_ptr() as u32).unwrap_or(0);
    let port_ptr = client_port.map(|p| p as *mut u16 as u32).unwrap_or(0);
    let r = unsafe { syscall_3(63, sockfd as u32, ip_ptr, port_ptr) };
    if r == 0xFFFFFFFF { -1 } else { r as i32 }
}

// --- New syscalls (64-78) ---

#[repr(C)]
pub struct Winsize {
    pub ws_col: u16,
    pub ws_row: u16,
    pub ws_xpixel: u16,
    pub ws_ypixel: u16,
}

pub fn sys_ioctl(fd: i32, request: u32, arg: *mut u8) -> isize {
    let r = unsafe { syscall_3(64, fd as u32, request, arg as u32) };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}

pub fn sys_ioctl_winsize(fd: i32, ws: &mut Winsize) -> isize {
    let r = unsafe { syscall_3(64, fd as u32, 0x5413, ws as *mut _ as u32) };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}

#[repr(C)]
pub struct PollFd {
    pub fd: i32,
    pub events: u16,
    pub revents: u16,
}

pub const POLLIN: u16 = 0x001;
pub const POLLOUT: u16 = 0x004;
pub const POLLERR: u16 = 0x008;
pub const POLLHUP: u16 = 0x010;
pub const POLLNVAL: u16 = 0x020;

pub fn sys_poll(fds: &mut [PollFd], timeout_ms: i32) -> isize {
    let r = unsafe {
        syscall_3(65, fds.as_mut_ptr() as u32, fds.len() as u32, timeout_ms as u32)
    };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}

pub struct Timespec {
    pub tv_sec: u64,
    pub tv_nsec: u64,
}

pub fn sys_clock_gettime(clockid: i32, tp: &mut Timespec) -> isize {
    let r = unsafe { syscall_2(66, clockid as u32, tp as *mut _ as u32) };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}

pub const CLOCK_REALTIME: i32 = 0;
pub const CLOCK_MONOTONIC: i32 = 1;

pub fn sys_nanosleep(req: &Timespec, rem: Option<&mut Timespec>) -> isize {
    let rem_ptr = rem.map(|r| r as *mut _ as u32).unwrap_or(0);
    let r = unsafe { syscall_2(67, req as *const _ as u32, rem_ptr) };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}

pub fn sys_getuid() -> u16 {
    let r: u32;
    unsafe { core::arch::asm!("int 0x80", inout("eax") 68u32 => r) };
    r as u16
}

pub fn sys_setuid(uid: u16) -> isize {
    let r = unsafe { syscall_2(69, uid as u32, 0) };
    if r == 0xFFFFFFFF { -1 } else { 0 }
}

pub fn sys_getgid() -> u16 {
    let r: u32;
    unsafe { core::arch::asm!("int 0x80", inout("eax") 70u32 => r) };
    r as u16
}

pub fn sys_setgid(gid: u16) -> isize {
    let r = unsafe { syscall_2(71, gid as u32, 0) };
    if r == 0xFFFFFFFF { -1 } else { 0 }
}

pub fn sys_geteuid() -> u16 {
    let r: u32;
    unsafe { core::arch::asm!("int 0x80", inout("eax") 72u32 => r) };
    r as u16
}

pub fn sys_getegid() -> u16 {
    let r: u32;
    unsafe { core::arch::asm!("int 0x80", inout("eax") 73u32 => r) };
    r as u16
}

pub fn sys_fchmod(fd: i32, mode: u16) -> isize {
    let r = unsafe { syscall_2(74, fd as u32, mode as u32) };
    if r == 0xFFFFFFFF { -1 } else { 0 }
}

pub fn sys_fchown(fd: i32, uid: u16, gid: u16) -> isize {
    let r = unsafe { syscall_3(75, fd as u32, uid as u32, gid as u32) };
    if r == 0xFFFFFFFF { -1 } else { 0 }
}

pub fn sys_chmod(path: &str, mode: u16) -> isize {
    let bytes = path.as_bytes();
    if bytes.len() >= 254 { return -1; }
    let mut cpath = [0u8; 256];
    cpath[..bytes.len()].copy_from_slice(bytes);
    let r = unsafe { syscall_2(76, &cpath as *const _ as u32, mode as u32) };
    if r == 0xFFFFFFFF { -1 } else { 0 }
}

pub fn sys_chown(path: &str, uid: u16, gid: u16) -> isize {
    let bytes = path.as_bytes();
    if bytes.len() >= 254 { return -1; }
    let mut cpath = [0u8; 256];
    cpath[..bytes.len()].copy_from_slice(bytes);
    let r = unsafe { syscall_3(77, &cpath as *const _ as u32, uid as u32, gid as u32) };
    if r == 0xFFFFFFFF { -1 } else { 0 }
}

#[repr(C)]
pub struct LinuxDirent64 {
    pub d_ino: u64,
    pub d_off: u64,
    pub d_reclen: u16,
    pub d_type: u8,
}

pub fn sys_getdents(fd: i32, buf: &mut [u8], count: usize) -> isize {
    let r = unsafe { syscall_3(78, fd as u32, buf.as_mut_ptr() as u32, count as u32) };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}

pub fn sys_symlink(target: &str, linkpath: &str) -> isize {
    let t_bytes = target.as_bytes();
    let l_bytes = linkpath.as_bytes();
    if t_bytes.len() >= 254 || l_bytes.len() >= 254 { return -1; }
    let mut t_buf = [0u8; 256];
    t_buf[..t_bytes.len()].copy_from_slice(t_bytes);
    let mut l_buf = [0u8; 256];
    l_buf[..l_bytes.len()].copy_from_slice(l_bytes);
    let r = unsafe { syscall_2(79, &t_buf as *const _ as u32, &l_buf as *const _ as u32) };
    if r == 0xFFFFFFFF { -1 } else { 0 }
}

pub fn sys_readlink(path: &str, buf: &mut [u8]) -> isize {
    let bytes = path.as_bytes();
    if bytes.len() >= 254 { return -1; }
    let mut cpath = [0u8; 256];
    cpath[..bytes.len()].copy_from_slice(bytes);
    let r = unsafe { syscall_3(80, &cpath as *const _ as u32, buf.as_mut_ptr() as u32, buf.len() as u32) };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}

pub fn sys_lseek(fd: isize, offset: isize, whence: i32) -> isize {
    let r = unsafe { syscall_3(40, fd as u32, offset as u32, whence as u32) };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}

pub fn sys_brk(addr: u64) -> u64 {
    let r = unsafe { syscall_2(38, addr as u32, 0) };
    r as u64
}

pub fn sys_dup(fd: i32) -> isize {
    let r = unsafe { syscall_2(41, fd as u32, 0) };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}

pub fn sys_fcntl(fd: i32, cmd: i32, arg: u32) -> isize {
    let r = unsafe { syscall_3(42, fd as u32, cmd as u32, arg) };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}

pub fn sys_mmap(addr: u64, length: u32, prot: u32, flags: u32, fd: i32, offset: u32) -> isize {
    let r = unsafe { syscall_5(36, addr as u32, length, prot, flags, fd as u32) };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}

pub fn sys_munmap(addr: u64, length: u32) -> isize {
    let r = unsafe { syscall_3(37, addr as u32, length, 0) };
    if r == 0xFFFFFFFF { -1 } else { 0 }
}

pub fn sys_select(nfds: i32, readfds: *mut u8, writefds: *mut u8, exceptfds: *mut u8, timeout: *mut u8) -> isize {
    let r = unsafe { syscall_5(45, nfds as u32, readfds as u32, writefds as u32, exceptfds as u32, timeout as u32) };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}

pub fn sys_getdirentries(fd: i32, buf: &mut [u8]) -> isize {
    sys_getdents(fd, buf, buf.len())
}

pub fn sys_setenv(name: &str, value: &str) -> isize {
    let n = name.as_bytes();
    let v = value.as_bytes();
    if n.len() >= 64 || v.len() >= 256 { return -1; }
    let mut nbuf = [0u8; 64];
    nbuf[..n.len()].copy_from_slice(n);
    let mut vbuf = [0u8; 256];
    vbuf[..v.len()].copy_from_slice(v);
    let r = unsafe { syscall_3(82, &nbuf as *const _ as u32, &vbuf as *const _ as u32, 0) };
    if r == 0xFFFFFFFF { -1 } else { 0 }
}

pub fn sys_getenv(name: &str, buf: &mut [u8]) -> isize {
    let n = name.as_bytes();
    if n.len() >= 64 { return -1; }
    let mut nbuf = [0u8; 64];
    nbuf[..n.len()].copy_from_slice(n);
    let r = unsafe { syscall_3(83, &nbuf as *const _ as u32, buf.as_mut_ptr() as u32, buf.len() as u32) };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}

pub fn sys_getenv_ptr(name: &str, buf: *mut u8, bufsize: u32) -> isize {
    let n = name.as_bytes();
    if n.len() >= 64 { return -1; }
    let mut nbuf = [0u8; 64];
    nbuf[..n.len()].copy_from_slice(n);
    let r = unsafe { syscall_3(83, &nbuf as *const _ as u32, buf as u32, bufsize) };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}

// --- Syscall 84: ps (list processes) ---

pub fn sys_ps(buf: &mut [u8]) -> isize {
    let r = unsafe { syscall_3(84, buf.as_mut_ptr() as u32, buf.len() as u32, buf.len() as u32) };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}

// --- Syscall 85: free_info(total_kb, free_kb) ---

pub fn sys_free_info(total: &mut u32, free: &mut u32) -> isize {
    let r = unsafe { syscall_3(85, total as *mut _ as u32, free as *mut _ as u32, 0) };
    if r == 0xFFFFFFFF { -1 } else { 0 }
}

// --- Syscall 86: list_env(buf, bufsize) ---

pub fn sys_list_env(buf: &mut [u8]) -> isize {
    let r = unsafe { syscall_3(86, buf.as_mut_ptr() as u32, buf.len() as u32, buf.len() as u32) };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}

// --- Syscall 87: proc_info(pid, buf, bufsize) ---

pub fn sys_proc_info(pid: u32, buf: &mut [u8]) -> isize {
    let r = unsafe { syscall_3(87, pid, buf.as_mut_ptr() as u32, buf.len() as u32) };
    if r == 0xFFFFFFFF { -1 } else { r as isize }
}
