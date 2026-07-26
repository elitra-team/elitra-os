use core::ptr;
use crate::scheduler::VNode;

const MAX_CMD_LEN: usize = 256;
const MAX_ARGS: usize = 32;

extern "C" {
    fn krust_vfs_resolve(path: *const u8) -> *mut VNode;
    fn krust_vfs_create_dir(path: *const u8) -> i32;
    fn krust_vfs_remove_node(path: *const u8) -> i32;
    fn krust_vfs_write_file(path: *const u8, data: *const u8, size: u32) -> i32;
    fn krust_vfs_pipe_create(fds: *mut i32) -> i32;
    fn krust_vfs_pipe_read(fd: i32, buf: *mut u8, size: u32) -> i32;
    fn krust_vfs_pipe_write(fd: i32, data: *const u8, size: u32) -> i32;
    fn krust_vfs_pipe_close(fd: i32);
    fn krust_vfs_root_node() -> *mut VNode;
    fn krust_mount_find(mount_point: *const u8) -> *mut crate::mount::MountInfo;
    fn krust_mount_count() -> i32;
    fn krust_mount_get(index: i32) -> *mut crate::mount::MountInfo;
    fn krust_mount_umount(mount_point: *const u8) -> i32;
    fn krust_fat32_delete_file(fs: *mut crate::fat32::Instance, dir_cluster: u32, name: *const u8) -> i32;
    fn krust_fat32_create_dir(fs: *mut crate::fat32::Instance, dir_cluster: u32, name: *const u8) -> i32;
    fn krust_elf_load(data: *const u8, size: u32, entry: *mut u64) -> i32;
    fn krust_sched_create_elf(entry: u64, argc: i32, argv: *const *const u8, stdin_fd: i32, stdout_fd: i32) -> i32;
    fn krust_sched_create(entry: extern "C" fn(), stdin_fd: i32, stdout_fd: i32) -> i32;
    fn krust_sched_yield();
    fn krust_sched_exit(code: u32);
    fn krust_sched_waitpid(pid: i32, status: *mut i32) -> i32;
    fn krust_sched_current_tid() -> i32;
    fn krust_ata_print_info(drive: i32);
    fn krust_ata_drive_count() -> i32;
    fn krust_ata_flush();
    fn krust_nvme_flush();
    fn krust_nvme_is_ready() -> bool;
    fn krust_malloc(size: u32) -> *mut u8;
    fn krust_free(ptr: *mut u8);
    fn krust_realloc(ptr: *mut u8, size: u32) -> *mut u8;
    fn krust_memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8;
    fn krust_memset(s: *mut u8, c: i32, n: usize) -> *mut u8;
    fn krust_strlen(s: *const u8) -> usize;
    fn krust_strcmp(s1: *const u8, s2: *const u8) -> i32;
    fn krust_strncmp(s1: *const u8, s2: *const u8, n: usize) -> i32;
    fn krust_strncpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8;
    fn krust_uitoa(num: u32, buf: *mut u8);
    fn krust_uitoa64_base(num: u64, buf: *mut u8, base: u32);
    fn krust_mm_info(total_kb: *mut u32, free_kb: *mut u32);
    fn krust_pittimer_get_ticks() -> u32;
    fn krust_paging_page_directory() -> *mut u64;
    fn krust_paging_get_phys(virt: u64) -> u64;
    fn krust_outb(port: u16, val: u8);
    fn krust_outw(port: u16, val: u16);
}

unsafe fn t_write(s: *const u8) {
    crate::terminal::krust_terminal_writestring(s);
}

unsafe fn t_write_color(s: *const u8, color: u8) {
    crate::terminal::krust_terminal_writestring_color(s, color);
}

unsafe fn t_putchar(c: u8) {
    crate::terminal::krust_terminal_putchar(c);
}

unsafe fn t_uint(n: u32) {
    let mut buf = [0u8; 16];
    krust_uitoa(n, buf.as_mut_ptr());
    t_write(buf.as_ptr());
}

unsafe fn t_hex64(n: u64) {
    let mut buf = [0u8; 20];
    krust_uitoa64_base(n, buf.as_mut_ptr(), 16);
    t_write(buf.as_ptr());
}

unsafe fn t_hex(n: u32) {
    let mut buf = [0u8; 12];
    krust_uitoa64_base(n as u64, buf.as_mut_ptr(), 16);
    t_write(buf.as_ptr());
}

// --- Test tasks ---

extern "C" fn test_task1() {
    unsafe {
        let mut count = 0i32;
        while count < 5 {
            t_write(b"[Task1] count=\0" as *const u8);
            t_uint(count as u32);
            t_putchar(b'\n');
            count += 1;
            for _ in 0..50000000 { core::hint::spin_loop(); }
            krust_sched_yield();
        }
        t_write(b"[Task1] exiting\n\0" as *const u8);
        krust_sched_exit(0);
    }
}

extern "C" fn test_task2() {
    unsafe {
        let mut count = 0i32;
        while count < 3 {
            t_write(b"[Task2] hello from task! count=\0" as *const u8);
            t_uint(count as u32);
            t_putchar(b'\n');
            count += 1;
            for _ in 0..30000000 { core::hint::spin_loop(); }
            krust_sched_yield();
        }
        t_write(b"[Task2] exiting\n\0" as *const u8);
        krust_sched_exit(0);
    }
}

// --- Helpers ---

unsafe fn print_node(node: *mut VNode, indent: usize) {
    for _ in 0..indent { t_putchar(b' '); }
    if (*node).type_ == 1 {
        crate::terminal::krust_terminal_set_color(0x03);
    }
    t_write((*node).name.as_ptr());
    crate::terminal::krust_terminal_set_color(0x07);
    if (*node).type_ == 1 { t_putchar(b'/'); }
    if (*node).type_ == 0 && (*node).size > 0 {
        t_write(b"  (\0" as *const u8);
        t_uint((*node).size);
        t_write(b" bytes)\0" as *const u8);
    }
    t_putchar(b'\n');
}

unsafe fn list_dir(dir: *mut VNode, indent: usize) {
    let mut c = (*dir).children;
    while !c.is_null() {
        print_node(c, indent);
        c = (*c).next;
    }
}

unsafe fn count_vfs_nodes(node: *mut VNode, files: *mut i32, dirs: *mut i32) {
    if node.is_null() { return; }
    if (*node).type_ == 0 { *files += 1; }
    if (*node).type_ == 1 { *dirs += 1; }
    let mut c = (*node).children;
    while !c.is_null() {
        if (*c).type_ == 1 {
            count_vfs_nodes(c, files, dirs);
        } else {
            *files += 1;
        }
        c = (*c).next;
    }
}

unsafe fn exec_file(
    path: *const u8, argc: i32, argv: *const *const u8,
    stdin_fd: i32, stdout_fd: i32,
) -> i32 {
    let mut node = krust_vfs_resolve(path);
    if node.is_null() || (*node).type_ != 0 {
        let p_len = krust_strlen(path);
        if p_len < 55 {
            let mut elf_path = [0u8; 64];
            let prefix = b"/bin/";
            let suffix = b".elf";
            let mut ei = 0usize;
            for &c in prefix { elf_path[ei] = c; ei += 1; }
            let mut pi = 0usize;
            while pi < p_len { elf_path[ei] = ptr::read_volatile(path.add(pi)); ei += 1; pi += 1; }
            for &c in suffix { elf_path[ei] = c; ei += 1; }
            elf_path[ei] = 0;
            node = krust_vfs_resolve(elf_path.as_ptr());
        }
        if node.is_null() || (*node).type_ != 0 { return -1; }
    }

    let mut entry: u64 = 0;
    if krust_elf_load((*node).data, (*node).size, &mut entry) != 0 { return -1; }
    krust_sched_create_elf(entry, argc, argv, stdin_fd, stdout_fd)
}

// --- Command handlers ---

unsafe fn cmd_help() {
    crate::cli_art::cli_art_print_help();
}

unsafe fn cmd_clear() {
    crate::terminal::krust_terminal_clear();
}

unsafe fn cmd_echo(args: *const *const u8, argc: i32) {
    for i in 1..argc {
        if i > 1 { t_putchar(b' '); }
        let s = *args.add(i as usize);
        if !s.is_null() { t_write(s); }
    }
    t_putchar(b'\n');
}

unsafe fn cmd_uptime() {
    crate::cli_art::cli_art_print_uptime();
}

unsafe fn cmd_meminfo() {
    crate::cli_art::cli_art_print_meminfo();
}

unsafe fn cmd_cpuinfo() {
    use crate::cpuid;
    if !cpuid::has_smep() && !cpuid::has_smap() && !cpuid::has_sse() {
        t_write(b"CPUID: not initialized\n\0" as *const u8);
        return;
    }
    t_write(b"CPU Vendor: \0" as *const u8);
    t_write(cpuid::vendor().as_ptr());
    t_putchar(b'\n');

    let brand = cpuid::brand();
    if brand[0] != 0 {
        t_write(b"CPU Brand: \0" as *const u8);
        t_write(brand.as_ptr());
        t_putchar(b'\n');
    }

    t_write(b"Features: \0" as *const u8);
    if cpuid::has_sse() { t_write(b"SSE \0" as *const u8); }
    if cpuid::has_sse2() { t_write(b"SSE2 \0" as *const u8); }
    if cpuid::has_sse3() { t_write(b"SSE3 \0" as *const u8); }
    if cpuid::has_ssse3() { t_write(b"SSSE3 \0" as *const u8); }
    if cpuid::has_sse41() { t_write(b"SSE4.1 \0" as *const u8); }
    if cpuid::has_sse42() { t_write(b"SSE4.2 \0" as *const u8); }
    if cpuid::has_avx() { t_write(b"AVX \0" as *const u8); }
    if cpuid::has_avx2() { t_write(b"AVX2 \0" as *const u8); }
    if cpuid::has_rdrand() { t_write(b"RDRAND \0" as *const u8); }
    if cpuid::has_rdtscp() { t_write(b"RDTSCP \0" as *const u8); }
    if cpuid::has_smep() { t_write(b"SMEP \0" as *const u8); }
    if cpuid::has_smap() { t_write(b"SMAP \0" as *const u8); }
    if cpuid::has_hypervisor() { t_write(b"HYPERVISOR \0" as *const u8); }
    t_putchar(b'\n');
}

unsafe fn cmd_version() {
    crate::cli_art::cli_art_print_version_box();
}

unsafe fn cmd_reboot() {
    t_write(b"Rebooting...\n\0" as *const u8);
    let mut good: u8 = 0x02;
    while good & 0x02 != 0 {
        core::arch::asm!("in al, dx", out("al") good, in("dx") 0x64u16);
    }
    core::arch::asm!("out dx, al", in("dx") 0x64u16, in("al") 0xFEu8);
    t_write(b"Reboot failed!\n\0" as *const u8);
}

unsafe fn cmd_shutdown() {
    t_write(b"Shutting down...\n\0" as *const u8);
    krust_outw(0x604, 0x2000);
    krust_outw(0xB004, 0x2000);
    t_write(b"Shutdown failed!\n\0" as *const u8);
}

unsafe fn cmd_tasks() {
    t_write(b"Tasks: use 'newt' to spawn a task\n\0" as *const u8);
}

unsafe fn cmd_testmalloc() {
    t_write(b"Testing malloc/free...\n\0" as *const u8);

    let p1 = krust_malloc(64);
    t_write(b"  malloc(64) = 0x\0" as *const u8);
    t_hex(p1 as u32);
    t_putchar(b'\n');

    let p2 = krust_malloc(256);
    t_write(b"  malloc(256) = 0x\0" as *const u8);
    t_hex(p2 as u32);
    t_putchar(b'\n');

    let p3 = krust_malloc(1024);
    t_write(b"  malloc(1024) = 0x\0" as *const u8);
    t_hex(p3 as u32);
    t_putchar(b'\n');

    krust_free(p2);
    t_write(b"  free(p2) OK\n\0" as *const u8);

    let p4 = krust_malloc(128);
    t_write(b"  malloc(128) = 0x\0" as *const u8);
    t_hex(p4 as u32);
    t_write(b" (should reuse p2)\n\0" as *const u8);

    let p5 = krust_realloc(p1, 128);
    t_write(b"  realloc(p1, 128) = 0x\0" as *const u8);
    t_hex(p5 as u32);
    t_putchar(b'\n');

    krust_free(p3);
    krust_free(p4);
    krust_free(p5);
    t_write(b"  malloc test passed\n\0" as *const u8);
}

unsafe fn cmd_testpaging() {
    t_write(b"Paging: PD=0x\0" as *const u8);
    t_hex64(krust_paging_page_directory() as u64);
    t_putchar(b'\n');

    let heap_test: *mut u32 = 0x40001000 as *mut u32;
    ptr::write_volatile(heap_test, 0xDEADBEEF);
    t_write(b"  Heap write test: 0x40001000 = 0x\0" as *const u8);
    t_hex(ptr::read_volatile(heap_test));
    t_putchar(b'\n');

    let phys = krust_paging_get_phys(0x40001000);
    t_write(b"  Phys addr: 0x\0" as *const u8);
    t_hex64(phys);
    t_putchar(b'\n');
}

unsafe fn cmd_createtask() {
    t_write(b"Creating test tasks...\n\0" as *const u8);
    let id1 = krust_sched_create(test_task1, -1, -1);
    let id2 = krust_sched_create(test_task2, -1, -1);
    t_write(b"  Task \0" as *const u8);
    t_uint(id1 as u32);
    t_write(b" and task \0" as *const u8);
    t_uint(id2 as u32);
    t_write(b" created\n\0" as *const u8);
}

unsafe fn cmd_ls(args: *mut u8) {
    let path = if !args.is_null() && ptr::read_volatile(args) != 0 {
        args
    } else {
        b"/\0" as *const u8 as *mut u8
    };
    let node = krust_vfs_resolve(path as *const u8);
    if node.is_null() {
        t_write(b"list: \0" as *const u8);
        t_write(path as *const u8);
        t_write(b": not found\n\0" as *const u8);
        return;
    }
    if (*node).type_ == 1 {
        // Print header
        t_putchar(b' ');
        crate::terminal::krust_terminal_set_color(0x0E);
        t_write(path as *const u8);
        crate::terminal::krust_terminal_set_color(0x07);
        t_write(b"/\n\0" as *const u8);

        let mut child = (*node).children;
        let mut file_count: u32 = 0;
        let mut dir_count: u32 = 0;

        while !child.is_null() {
            // Type indicator with color
            if (*child).type_ == 1 {
                crate::terminal::krust_terminal_set_color(0x0B); // cyan for dirs
                t_putchar(b'[');
                t_write(b"dir \0" as *const u8);
                t_putchar(b']');
                t_putchar(b' ');
                crate::terminal::krust_terminal_set_color(0x0B);
                t_write((*child).name.as_ptr());
                t_putchar(b'/');
                crate::terminal::krust_terminal_set_color(0x07);
                dir_count += 1;
            } else {
                crate::terminal::krust_terminal_set_color(0x08); // dark gray prefix
                t_putchar(b'[');
                t_write(b"file\0" as *const u8);
                t_putchar(b']');
                t_putchar(b' ');
                // Color based on file size
                if (*child).size > 1024 {
                    crate::terminal::krust_terminal_set_color(0x0F); // bright white for large files
                } else {
                    crate::terminal::krust_terminal_set_color(0x07); // gray for small files
                }
                t_write((*child).name.as_ptr());
                // Show size
                crate::terminal::krust_terminal_set_color(0x08);
                t_write(b"  (\0" as *const u8);
                t_uint((*child).size);
                t_write(b" B)\0" as *const u8);
                crate::terminal::krust_terminal_set_color(0x07);
                file_count += 1;
            }
            t_putchar(b'\n');
            child = (*child).next;
        }

        // Summary line
        crate::terminal::krust_terminal_set_color(0x08);
        t_putchar(b' ');
        t_putchar(b' ');
        t_uint(dir_count);
        t_write(b" directories, \0" as *const u8);
        t_uint(file_count);
        t_write(b" files\n\0" as *const u8);
        crate::terminal::krust_terminal_set_color(0x07);
    } else {
        // Single file - show detailed info
        crate::terminal::krust_terminal_set_color(0x0F);
        t_write((*node).name.as_ptr());
        crate::terminal::krust_terminal_set_color(0x07);
        t_putchar(b' ');
        t_putchar(b' ');
        if (*node).size > 0 {
            crate::terminal::krust_terminal_set_color(0x08);
            t_write(b"(\0" as *const u8);
            t_uint((*node).size);
            t_write(b" bytes)\0" as *const u8);
        } else {
            crate::terminal::krust_terminal_set_color(0x08);
            t_write(b"(empty)\0" as *const u8);
        }
        crate::terminal::krust_terminal_set_color(0x07);
        t_putchar(b'\n');
    }
}

unsafe fn cmd_cat(args: *mut u8) {
    if args.is_null() || ptr::read_volatile(args) == 0 {
        t_write(b"Usage: dump <path>\n\0" as *const u8);
        return;
    }
    let node = krust_vfs_resolve(args as *const u8);
    if node.is_null() || ((*node).type_ != 0 && (*node).type_ != 2) {
        t_write(b"dump: \0" as *const u8);
        t_write(args as *const u8);
        t_write(b": not found\n\0" as *const u8);
        return;
    }

    if (*node).type_ == 2 {
        let mut buf: [u8; 64] = [0; 64];
        if let Some(read_fn) = (*node).dev_read {
            let n = read_fn(node, buf.as_mut_ptr(), 64, 0);
            if n > 0 {
                crate::terminal::krust_terminal_write(buf.as_ptr(), n as usize);
            }
        }
        t_putchar(b'\n');
        return;
    }

    if !(*node).data.is_null() && (*node).size > 0 {
        crate::terminal::krust_terminal_write((*node).data, (*node).size as usize);
    }
    t_putchar(b'\n');
}

unsafe fn cmd_vfsinfo() {
    t_write(b"VFS root: 0x\0" as *const u8);
    t_hex64(krust_vfs_root_node() as u64);
    t_putchar(b'\n');

    let mut file_count: i32 = 0;
    let mut dir_count: i32 = 0;
    count_vfs_nodes(krust_vfs_root_node(), &mut file_count as *mut _, &mut dir_count as *mut _);
    t_write(b"  Directories: \0" as *const u8);
    t_uint(dir_count as u32);
    t_putchar(b'\n');
    t_write(b"  Files: \0" as *const u8);
    t_uint(file_count as u32);
    t_putchar(b'\n');
}

unsafe fn cmd_touch(args: *mut u8) {
    if args.is_null() || ptr::read_volatile(args) == 0 {
        t_write(b"Usage: create <path>\n\0" as *const u8);
        return;
    }
    if krust_vfs_write_file(args as *const u8, ptr::null(), 0) == 0 {
        t_write(b"create: created '\0" as *const u8);
        t_write(args as *const u8);
        t_write(b"'\n\0" as *const u8);
    } else {
        t_write(b"create: failed to create '\0" as *const u8);
        t_write(args as *const u8);
        t_write(b"'\n\0" as *const u8);
    }
}

unsafe fn cmd_rm(args: *mut u8) {
    if args.is_null() || ptr::read_volatile(args) == 0 {
        t_write(b"Usage: del <path>\n\0" as *const u8);
        return;
    }

    let mi = krust_mount_find(args as *const u8);
    if !mi.is_null() && (*mi).type_ == 1 {
        let fat = (*mi).instance as *mut crate::fat32::Instance;
        let mut name = args as *const u8;
        let mut p = args as *const u8;
        while ptr::read_volatile(p) != 0 {
            if ptr::read_volatile(p) == b'/' { name = p.add(1); }
            p = p.add(1);
        }
        if krust_fat32_delete_file(fat, (*fat).root_cluster, name) == 0 {
            krust_vfs_remove_node(args as *const u8);
            t_write(b"del: removed '\0" as *const u8);
            t_write(args as *const u8);
            t_write(b"'\n\0" as *const u8);
        } else {
            t_write(b"del: failed '\0" as *const u8);
            t_write(args as *const u8);
            t_write(b"'\n\0" as *const u8);
        }
        return;
    }

    if krust_vfs_remove_node(args as *const u8) == 0 {
        t_write(b"del: removed '\0" as *const u8);
        t_write(args as *const u8);
        t_write(b"'\n\0" as *const u8);
    } else {
        t_write(b"del: failed '\0" as *const u8);
        t_write(args as *const u8);
        t_write(b"'\n\0" as *const u8);
    }
}

unsafe fn cmd_mkdir(args: *mut u8) {
    if args.is_null() || ptr::read_volatile(args) == 0 {
        t_write(b"Usage: md <path>\n\0" as *const u8);
        return;
    }

    let mi = krust_mount_find(args as *const u8);
    if !mi.is_null() && (*mi).type_ == 1 {
        let fat = (*mi).instance as *mut crate::fat32::Instance;
        let mut name = args as *const u8;
        let mut p = args as *const u8;
        while ptr::read_volatile(p) != 0 {
            if ptr::read_volatile(p) == b'/' { name = p.add(1); }
            p = p.add(1);
        }
        if krust_fat32_create_dir(fat, (*fat).root_cluster, name) == 0 {
            krust_vfs_create_dir(args as *const u8);
            t_write(b"md: created '\0" as *const u8);
            t_write(args as *const u8);
            t_write(b"'\n\0" as *const u8);
        } else {
            t_write(b"md: failed '\0" as *const u8);
            t_write(args as *const u8);
            t_write(b"'\n\0" as *const u8);
        }
        return;
    }

    if krust_vfs_create_dir(args as *const u8) == 0 {
        t_write(b"md: created '\0" as *const u8);
        t_write(args as *const u8);
        t_write(b"'\n\0" as *const u8);
    } else {
        t_write(b"md: failed '\0" as *const u8);
        t_write(args as *const u8);
        t_write(b"'\n\0" as *const u8);
    }
}

unsafe fn cmd_write(args: *const *const u8, argc: i32) {
    if argc < 2 {
        t_write(b"Usage: put <path> <text>\n\0" as *const u8);
        return;
    }
    let path = *args.add(1);
    let mut content: *mut u8 = ptr::null_mut();
    let mut content_len: usize = 0;
    if argc > 2 {
        let mut total_len: usize = 0;
        for i in 2..argc {
            let s = *args.add(i as usize);
            if !s.is_null() {
                total_len += krust_strlen(s) + 1;
            }
        }
        content = krust_malloc(total_len as u32) as *mut u8;
        if content.is_null() {
            t_write(b"put: allocation failed\n\0" as *const u8);
            return;
        }
        let mut p = content;
        for i in 2..argc {
            let s = *args.add(i as usize);
            if s.is_null() { continue; }
            let sl = krust_strlen(s);
            krust_memcpy(p, s, sl);
            p = p.add(sl);
            if i + 1 < argc { ptr::write_volatile(p, b' '); p = p.add(1); }
        }
        ptr::write_volatile(p, 0);
        content_len = total_len - 1;
    }

    if krust_vfs_write_file(path, content, content_len as u32) == 0 {
        t_write(b"put: wrote \0" as *const u8);
        t_uint(content_len as u32);
        t_write(b" bytes to '\0" as *const u8);
        t_write(path);
        t_write(b"'\n\0" as *const u8);
    } else {
        t_write(b"put: failed\n\0" as *const u8);
    }

    if !content.is_null() { krust_free(content); }
}

unsafe fn cmd_mount(_args: *mut u8) {
    let count = krust_mount_count();
    t_write(b"Mounted filesystems (\0" as *const u8);
    t_uint(count as u32);
    t_write(b"):\n\0" as *const u8);
    for i in 0..16 {
        let m = krust_mount_get(i);
        if m.is_null() { continue; }
        if (*m).used {
            let type_str = match (*m).type_ {
                0 => b"ramfs\0" as *const u8,
                1 => b"fat32\0" as *const u8,
                2 => b"ext2\0" as *const u8,
                _ => b"unknown\0" as *const u8,
            };
            t_write(b"  \0" as *const u8);
            t_write((*m).mount_point.as_ptr());
            t_write(b"  type=\0" as *const u8);
            t_write(type_str);
            t_putchar(b'\n');
        }
    }
}

unsafe fn cmd_exec(args: *mut u8) {
    if args.is_null() || ptr::read_volatile(args) == 0 {
        t_write(b"Usage: exec <path>\n\0" as *const u8);
        return;
    }
    let argv_list: [*const u8; 2] = [args as *const u8, ptr::null()];
    let tid = exec_file(args as *const u8, 1, argv_list.as_ptr(), -1, -1);
    if tid < 0 {
        t_write(b"exec: '\0" as *const u8);
        t_write(args as *const u8);
        t_write(b"' not found or failed\n\0" as *const u8);
        return;
    }
    t_write(b"exec: '\0" as *const u8);
    t_write(args as *const u8);
    t_write(b"' loaded, task \0" as *const u8);
    t_uint(tid as u32);
    t_write(b" running\n\0" as *const u8);
}

unsafe fn cmd_ata(_args: *mut u8) {
    let count = krust_ata_drive_count();
    t_write(b"ata: \0" as *const u8);
    t_uint(count as u32);
    t_write(b" drive(s)\n\0" as *const u8);
    for d in 0..count {
        krust_ata_print_info(d);
    }
}

unsafe fn cmd_sync(_args: *mut u8) {
    t_write(b"sync: flushing to disk...\n\0" as *const u8);
    krust_ata_flush();
    if krust_nvme_is_ready() {
        krust_nvme_flush();
    }
    t_write(b"sync: done\n\0" as *const u8);
}

unsafe fn cmd_cd(args: *mut u8) {
    if args.is_null() || ptr::read_volatile(args) == 0 {
        let mut cwd_buf = [0u8; 128];
        crate::vfs::krust_vfs_getcwd(cwd_buf.as_mut_ptr(), 128);
        t_write(cwd_buf.as_ptr());
        t_putchar(b'\n');
        return;
    }
    let result = crate::vfs::krust_vfs_chdir(args as *const u8);
    if result != 0 {
        t_write(b"cd: \0" as *const u8);
        t_write(args as *const u8);
        t_write(b": no such directory\n\0" as *const u8);
    }
}

unsafe fn cmd_ps(_args: *mut u8) {
    crate::cli_art::cli_art_print_ps();
}

unsafe fn cmd_kill(args: *mut u8) {
    if args.is_null() || ptr::read_volatile(args) == 0 {
        t_write(b"Usage: kill <pid>\n\0" as *const u8);
        return;
    }
    let pid = crate::klib::krust_atoi(args as *const u8);
    if pid <= 0 {
        t_write(b"kill: invalid pid\n\0" as *const u8);
        return;
    }
    let result = crate::scheduler::krust_sched_kill(pid, 9);
    if result == 0 {
        t_write(b"kill: process \0" as *const u8);
        t_uint(pid as u32);
        t_write(b" killed\n\0" as *const u8);
    } else {
        t_write(b"kill: failed to kill process \0" as *const u8);
        t_uint(pid as u32);
        t_putchar(b'\n');
    }
}

unsafe fn cmd_df(_args: *mut u8) {
    crate::cli_art::cli_art_print_df();
}

unsafe fn cmd_date(_args: *mut u8) {
    crate::cli_art::cli_art_print_date();
}

unsafe fn cmd_umount(args: *mut u8) {
    if args.is_null() || ptr::read_volatile(args) == 0 {
        t_write(b"Usage: unm <path>\n\0" as *const u8);
        return;
    }
    if krust_mount_umount(args as *const u8) == 0 {
        krust_vfs_remove_node(args as *const u8);
        t_write(b"unm: '\0" as *const u8);
        t_write(args as *const u8);
        t_write(b"' removed\n\0" as *const u8);
    } else {
        t_write(b"unm: '\0" as *const u8);
        t_write(args as *const u8);
        t_write(b"' not found\n\0" as *const u8);
    }
}

unsafe fn cmd_neofetch() {
    crate::cli_art::cli_art_print_neofetch();
}

unsafe fn cmd_tree(args: *mut u8) {
    let path = if !args.is_null() && ptr::read_volatile(args) != 0 {
        args
    } else {
        b"/\0" as *const u8 as *mut u8
    };
    let node = krust_vfs_resolve(path as *const u8);
    if node.is_null() {
        t_write(b"tree: '\0" as *const u8);
        t_write(path as *const u8);
        t_write(b"' not found\n\0" as *const u8);
        return;
    }
    crate::cli_art::cli_art_print_tree(node, b"\0", true);
}

unsafe fn cmd_top() {
    crate::cli_art::cli_art_print_top();
}

unsafe fn cmd_cowsay(args: *const *const u8, argc: i32) {
    if argc < 2 {
        crate::cli_art::cli_art_print_cowsay(b"Moo!\0".as_ptr());
        return;
    }
    // Join args into a single string
    let mut buf = [0u8; 256];
    let mut pos = 0;
    for i in 1..argc {
        let s = *args.add(i as usize);
        if s.is_null() { continue; }
        let sl = krust_strlen(s);
        if pos + sl < buf.len() {
            krust_memcpy(buf.as_mut_ptr().add(pos), s, sl);
            pos += sl;
        }
        if i + 1 < argc && pos < buf.len() {
            buf[pos] = b' ';
            pos += 1;
        }
    }
    buf[pos] = 0;
    crate::cli_art::cli_art_print_cowsay(buf.as_ptr());
}

unsafe fn parse_args(cmd: *mut u8, args: *mut *mut u8, argc: *mut i32) {
    let mut ac = 0i32;
    let mut p = cmd;
    while ptr::read_volatile(p) != 0 && ptr::read_volatile(p) == b' ' { p = p.add(1); }
    while ptr::read_volatile(p) != 0 && ac < MAX_ARGS as i32 {
        if ptr::read_volatile(p) == b'"' {
            p = p.add(1);
            *args.add(ac as usize) = p;
            while ptr::read_volatile(p) != 0 && ptr::read_volatile(p) != b'"' { p = p.add(1); }
            if ptr::read_volatile(p) != 0 {
                ptr::write_volatile(p, 0);
                p = p.add(1);
            }
            ac += 1;
        } else {
            *args.add(ac as usize) = p;
            ac += 1;
            while ptr::read_volatile(p) != 0 && ptr::read_volatile(p) != b' ' { p = p.add(1); }
            if ptr::read_volatile(p) != 0 {
                ptr::write_volatile(p, 0);
                p = p.add(1);
            }
        }
        while ptr::read_volatile(p) != 0 && ptr::read_volatile(p) == b' ' { p = p.add(1); }
    }
    *args.add(ac as usize) = ptr::null_mut();
    *argc = ac;
}

unsafe fn is_write_cmd(name: *const u8) -> bool {
    krust_strcmp(name, b"create\0" as *const u8) == 0
        || krust_strcmp(name, b"del\0" as *const u8) == 0
        || krust_strcmp(name, b"md\0" as *const u8) == 0
        || krust_strcmp(name, b"put\0" as *const u8) == 0
}

#[no_mangle]
pub unsafe fn shell_run() {
    crate::cli_art::cli_art_login();
    crate::cli_art::cli_art_print_splash();

    let mut cmd_line: [u8; MAX_CMD_LEN] = [0; MAX_CMD_LEN];
    let mut args: [*mut u8; MAX_ARGS] = [ptr::null_mut(); MAX_ARGS];
    let mut argc: i32 = 0;

    loop {
        let cur_task = crate::scheduler::krust_sched_get_task(crate::scheduler::krust_sched_get_pid() as u32);
        let cwd_ptr = if !cur_task.is_null() { (*cur_task).cwd.as_ptr() as *const u8 } else { b"/\0" as *const u8 };
        crate::cli_art::cli_art_print_prompt(cwd_ptr);
        crate::terminal::krust_terminal_set_color(0x07);

        crate::terminal::krust_terminal_readline(cmd_line.as_mut_ptr(), MAX_CMD_LEN as i32);

        if ptr::read_volatile(cmd_line.as_ptr()) == 0 { continue; }

        // Check for pipe (|)
        let mut pipe_pos: *mut u8 = ptr::null_mut();
        let mut p = cmd_line.as_mut_ptr();
        while ptr::read_volatile(p) != 0 {
            if ptr::read_volatile(p) == b'|' { pipe_pos = p; break; }
            p = p.add(1);
        }

        if !pipe_pos.is_null() {
            ptr::write_volatile(pipe_pos, 0);
            let cmd1 = cmd_line.as_mut_ptr();
            let cmd2 = pipe_pos.add(1);

            let mut end1 = cmd1;
            while ptr::read_volatile(end1) != 0 { end1 = end1.add(1); }
            while end1 > cmd1 && ptr::read_volatile(end1.sub(1)) == b' ' {
                end1 = end1.sub(1);
                ptr::write_volatile(end1, 0);
            }
            let mut cp = cmd2;
            while ptr::read_volatile(cp) == b' ' { cp = cp.add(1); }

            if ptr::read_volatile(cmd1) == 0 || ptr::read_volatile(cp) == 0 {
                t_write(b"Usage: <cmd1> | <cmd2>\n\0" as *const u8);
                continue;
            }

            let mut fds: [i32; 2] = [0; 2];
            if krust_vfs_pipe_create(fds.as_mut_ptr()) != 0 {
                t_write(b"pipe: creation failed\n\0" as *const u8);
                continue;
            }

            let args1: [*const u8; 2] = [cmd1 as *const u8, ptr::null()];
            let args2: [*const u8; 2] = [cp as *const u8, ptr::null()];
            let tid1 = exec_file(cmd1 as *const u8, 1, args1.as_ptr(), -1, fds[1]);
            if tid1 < 0 {
                t_write(b"pipe: '\0" as *const u8);
                t_write(cmd1 as *const u8);
                t_write(b"' not found\n\0" as *const u8);
                krust_vfs_pipe_close(fds[0]);
                krust_vfs_pipe_close(fds[1]);
                continue;
            }
            let tid2 = exec_file(cp as *const u8, 1, args2.as_ptr(), fds[0], -1);
            if tid2 < 0 {
                t_write(b"pipe: '\0" as *const u8);
                t_write(cp as *const u8);
                t_write(b"' not found\n\0" as *const u8);
                let _ = krust_sched_waitpid(tid1, ptr::null_mut());
                krust_vfs_pipe_close(fds[0]);
                krust_vfs_pipe_close(fds[1]);
                continue;
            }

            let _ = krust_sched_waitpid(tid1, ptr::null_mut());
            krust_vfs_pipe_close(fds[1]);
            let _ = krust_sched_waitpid(tid2, ptr::null_mut());
            krust_vfs_pipe_close(fds[0]);
            t_write(b"pipe: done\n\0" as *const u8);
            continue;
        }

        // Check for output redirection (>)
        let mut redir_out: *mut u8 = ptr::null_mut();
        let mut p = cmd_line.as_mut_ptr();
        while ptr::read_volatile(p) != 0 {
            if ptr::read_volatile(p) == b'>' { redir_out = p; break; }
            p = p.add(1);
        }

        if !redir_out.is_null() {
            ptr::write_volatile(redir_out, 0);
            let mut cmd = cmd_line.as_mut_ptr();
            while ptr::read_volatile(cmd) == b' ' { cmd = cmd.add(1); }
            let mut end_cmd = cmd;
            while ptr::read_volatile(end_cmd) != 0 { end_cmd = end_cmd.add(1); }
            while end_cmd > cmd && ptr::read_volatile(end_cmd.sub(1)) == b' ' {
                end_cmd = end_cmd.sub(1);
                ptr::write_volatile(end_cmd, 0);
            }
            let file = redir_out.add(1);
            let mut fp = file;
            while ptr::read_volatile(fp) == b' ' { fp = fp.add(1); }

            if ptr::read_volatile(cmd) == 0 || ptr::read_volatile(fp) == 0 {
                t_write(b"Usage: <cmd> > <file>\n\0" as *const u8);
                continue;
            }

            let mut fds: [i32; 2] = [0; 2];
            if krust_vfs_pipe_create(fds.as_mut_ptr()) != 0 {
                t_write(b"redir: pipe creation failed\n\0" as *const u8);
                continue;
            }

            let args_cmd: [*const u8; 2] = [cmd as *const u8, ptr::null()];
            let tid = exec_file(cmd as *const u8, 1, args_cmd.as_ptr(), -1, fds[1]);
            if tid < 0 {
                t_write(b"redir: '\0" as *const u8);
                t_write(cmd as *const u8);
                t_write(b"' not found\n\0" as *const u8);
                krust_vfs_pipe_close(fds[0]);
                krust_vfs_pipe_close(fds[1]);
                continue;
            }

            let _ = krust_sched_waitpid(tid, ptr::null_mut());
            krust_vfs_pipe_close(fds[1]);

            let mut accum: [u8; 8192] = [0; 8192];
            let mut total: u32 = 0;
            loop {
                let mut buf: [u8; 4096] = [0; 4096];
                let n = krust_vfs_pipe_read(fds[0], buf.as_mut_ptr(), 4096);
                if n <= 0 { break; }
                if total as usize + n as usize > 8192 { break; }
                krust_memcpy(accum.as_mut_ptr().add(total as usize), buf.as_ptr(), n as usize);
                total += n as u32;
            }
            krust_vfs_pipe_close(fds[0]);

            if total > 0 {
                krust_vfs_write_file(fp as *const u8, accum.as_ptr(), total);
                krust_ata_flush();
                if krust_nvme_is_ready() { krust_nvme_flush(); }
            }
            t_write(b"redir: done\n\0" as *const u8);
            continue;
        }

        parse_args(cmd_line.as_mut_ptr(), args.as_mut_ptr(), &mut argc as *mut _);
        if argc == 0 { continue; }

        let cmd_name = *args.as_mut_ptr();

        if krust_strcmp(cmd_name, b"?\0" as *const u8) == 0 || krust_strcmp(cmd_name, b"help\0" as *const u8) == 0 {
            cmd_help();
        } else if krust_strcmp(cmd_name, b"clr\0" as *const u8) == 0 {
            cmd_clear();
        } else if krust_strcmp(cmd_name, b"say\0" as *const u8) == 0 {
            cmd_echo(args.as_mut_ptr() as *const *const u8, argc);
        } else if krust_strcmp(cmd_name, b"upt\0" as *const u8) == 0 {
            cmd_uptime();
        } else if krust_strcmp(cmd_name, b"mem\0" as *const u8) == 0 {
            cmd_meminfo();
        } else if krust_strcmp(cmd_name, b"cpu\0" as *const u8) == 0 {
            cmd_cpuinfo();
        } else if krust_strcmp(cmd_name, b"ver\0" as *const u8) == 0 {
            cmd_version();
        } else if krust_strcmp(cmd_name, b"rst\0" as *const u8) == 0 {
            cmd_reboot();
        } else if krust_strcmp(cmd_name, b"off\0" as *const u8) == 0 {
            cmd_shutdown();
        } else if krust_strcmp(cmd_name, b"jobs\0" as *const u8) == 0 {
            cmd_tasks();
        } else if krust_strcmp(cmd_name, b"mall\0" as *const u8) == 0 {
            cmd_testmalloc();
        } else if krust_strcmp(cmd_name, b"pt\0" as *const u8) == 0 {
            cmd_testpaging();
        } else if krust_strcmp(cmd_name, b"newt\0" as *const u8) == 0 {
            cmd_createtask();
        } else if krust_strcmp(cmd_name, b"list\0" as *const u8) == 0 {
            cmd_ls(if argc > 1 { *args.as_mut_ptr().add(1) } else { ptr::null_mut() });
        } else if krust_strcmp(cmd_name, b"dump\0" as *const u8) == 0 {
            cmd_cat(if argc > 1 { *args.as_mut_ptr().add(1) } else { ptr::null_mut() });
        } else if krust_strcmp(cmd_name, b"exec\0" as *const u8) == 0 {
            cmd_exec(if argc > 1 { *args.as_mut_ptr().add(1) } else { ptr::null_mut() });
        } else if krust_strcmp(cmd_name, b"ata\0" as *const u8) == 0 {
            cmd_ata(if argc > 1 { *args.as_mut_ptr().add(1) } else { ptr::null_mut() });
        } else if krust_strcmp(cmd_name, b"sync\0" as *const u8) == 0 {
            cmd_sync(if argc > 1 { *args.as_mut_ptr().add(1) } else { ptr::null_mut() });
        } else if krust_strcmp(cmd_name, b"fs\0" as *const u8) == 0 {
            cmd_vfsinfo();
        } else if krust_strcmp(cmd_name, b"create\0" as *const u8) == 0 {
            cmd_touch(if argc > 1 { *args.as_mut_ptr().add(1) } else { ptr::null_mut() });
        } else if krust_strcmp(cmd_name, b"del\0" as *const u8) == 0 {
            cmd_rm(if argc > 1 { *args.as_mut_ptr().add(1) } else { ptr::null_mut() });
        } else if krust_strcmp(cmd_name, b"md\0" as *const u8) == 0 {
            cmd_mkdir(if argc > 1 { *args.as_mut_ptr().add(1) } else { ptr::null_mut() });
        } else if krust_strcmp(cmd_name, b"put\0" as *const u8) == 0 {
            cmd_write(args.as_mut_ptr() as *const *const u8, argc);
        } else if krust_strcmp(cmd_name, b"mnt\0" as *const u8) == 0 {
            cmd_mount(if argc > 1 { *args.as_mut_ptr().add(1) } else { ptr::null_mut() });
        } else if krust_strcmp(cmd_name, b"unm\0" as *const u8) == 0 {
            cmd_umount(if argc > 1 { *args.as_mut_ptr().add(1) } else { ptr::null_mut() });
        } else if krust_strcmp(cmd_name, b"cd\0" as *const u8) == 0 {
            cmd_cd(if argc > 1 { *args.as_mut_ptr().add(1) } else { ptr::null_mut() });
        } else if krust_strcmp(cmd_name, b"ps\0" as *const u8) == 0 {
            cmd_ps(ptr::null_mut());
        } else if krust_strcmp(cmd_name, b"kill\0" as *const u8) == 0 {
            cmd_kill(if argc > 1 { *args.as_mut_ptr().add(1) } else { ptr::null_mut() });
        } else if krust_strcmp(cmd_name, b"df\0" as *const u8) == 0 {
            cmd_df(ptr::null_mut());
        } else if krust_strcmp(cmd_name, b"date\0" as *const u8) == 0 {
            cmd_date(ptr::null_mut());
        } else if krust_strcmp(cmd_name, b"neo\0" as *const u8) == 0 || krust_strcmp(cmd_name, b"neofetch\0" as *const u8) == 0 {
            cmd_neofetch();
        } else if krust_strcmp(cmd_name, b"tree\0" as *const u8) == 0 {
            cmd_tree(if argc > 1 { *args.as_mut_ptr().add(1) } else { ptr::null_mut() });
        } else if krust_strcmp(cmd_name, b"top\0" as *const u8) == 0 {
            cmd_top();
        } else if krust_strcmp(cmd_name, b"cowsay\0" as *const u8) == 0 || krust_strcmp(cmd_name, b"moo\0" as *const u8) == 0 {
            cmd_cowsay(args.as_mut_ptr() as *const *const u8, argc);
        } else {
            t_write(b"Unknown command: \0" as *const u8);
            t_write(cmd_name as *const u8);
            t_putchar(b'\n');
        }

        if is_write_cmd(cmd_name) {
            krust_ata_flush();
            if krust_nvme_is_ready() { krust_nvme_flush(); }
        }
    }
}
