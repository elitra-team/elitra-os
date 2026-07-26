use core::ptr;
use crate::scheduler::VNode;
use crate::net::NetDevice;

extern "C" fn reschedule_handler(r: *mut crate::scheduler::Registers) {
    unsafe { crate::scheduler::krust_sched_preempt(r); }
}

extern "C" {
    fn krust_ata_init();
    fn krust_ps2kbd_init();
    fn krust_vfs_resolve(path: *const u8) -> *mut VNode;
    fn krust_vfs_create_dir(path: *const u8) -> i32;
    fn krust_vfs_create_file(path: *const u8, data: *const u8, size: u32) -> i32;
    fn krust_vfs_create_device(
        path: *const u8,
        dev_read: Option<extern "C" fn(*mut VNode, *mut u8, u32, u32) -> i32>,
        dev_write: Option<extern "C" fn(*mut VNode, *const u8, u32, u32) -> i32>,
    ) -> i32;
    fn krust_vfs_write_file(path: *const u8, data: *const u8, size: u32) -> i32;
    fn krust_vfs_pipe_create(fds: *mut i32) -> i32;
    fn krust_vfs_pipe_read(fd: i32, buf: *mut u8, size: u32) -> i32;
    fn krust_vfs_pipe_write(fd: i32, data: *const u8, size: u32) -> i32;
    fn krust_vfs_pipe_close(fd: i32);
    fn krust_mount_init();
    fn krust_mount_mount(mount_point: *const u8, type_: u32, instance: *mut u8) -> i32;
    fn krust_fat32_init(fs: *mut crate::fat32::Instance, image: *const u8, image_size: usize) -> u8;
    fn krust_ata_drive_count() -> i32;
    fn krust_ata_find_partitions(drive: i32, parts: *mut crate::ata_pio::Partition, max_parts: i32) -> i32;
    fn krust_ata_read(drive: i32, lba: u32, count_: u8, buf: *mut u8) -> bool;
    fn krust_ata_mount_partition_buffer(drive: i32, lba_start: u32, sectors: u32, buffer: *mut u8) -> bool;
    fn krust_ata_flush();
    fn krust_elf_load(data: *const u8, size: u32, entry: *mut u64) -> i32;
    fn krust_sched_create_init(elf_data: *const u8, elf_size: u32) -> i32;
    fn krust_sched_yield();
    fn krust_malloc(size: u32) -> *mut u8;
    fn krust_free(ptr: *mut u8);
    fn krust_memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8;
    fn krust_strlen(s: *const u8) -> usize;
    fn krust_strcmp(s1: *const u8, s2: *const u8) -> i32;
    fn krust_strncpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8;
    fn krust_uitoa(num: u32, buf: *mut u8);
    fn krust_fat32_mount(fs: *mut crate::fat32::Instance, mount_point: *const u8) -> u8;
    static _binary_fat32_img_start: u8;
    static _binary_fat32_img_end: u8;
}

#[repr(C, packed)]
struct MultibootFB {
    flags: u32,
    _pad: [u32; 11],
    _vbe: [u32; 9],
    fb_addr: u64,
    fb_pitch: u32,
    fb_width: u32,
    fb_height: u32,
    fb_bpp: u8,
    fb_type: u8,
}

unsafe fn init_framebuffer(magic: u32, addr: u32) {
    if magic == 0x2BADB002 {
        // Multiboot1: info struct is at addr directly
        let mbi = &*(addr as *const MultibootFB);
        if mbi.flags & (1 << 12) != 0 {
            // bit 12 = framebuffer info available
            crate::framebuffer::krust_framebuffer_init(
                mbi.fb_addr, mbi.fb_width, mbi.fb_height, mbi.fb_pitch, mbi.fb_bpp,
            );
            crate::framebuffer::krust_framebuffer_clear(crate::framebuffer::COLOR_BLACK);
            crate::fb_console::fb_console_init(mbi.fb_width, mbi.fb_height, mbi.fb_pitch, mbi.fb_bpp, mbi.fb_addr as *mut u8);
            SAVED_FB_W = mbi.fb_width;
            SAVED_FB_H = mbi.fb_height;
            FB_AVAILABLE = true;
            serial(b"step: fb ok (multiboot1)\n\0");
            return;
        }
    } else if magic == 0x36d76289 {
        // Multiboot2: parse tags to find framebuffer (type 5)
        let info = addr as *const u8;
        let total_size = ptr::read_volatile(info as *const u32);
        let mut offset = 8u32; // skip total_size + reserved
        while offset < total_size {
            let tag_type = ptr::read_volatile(info.add(offset as usize) as *const u32);
            let tag_size = ptr::read_volatile(info.add(offset as usize + 4) as *const u32);
            if tag_type == 0 || tag_size < 8 { break; } // end tag
            if tag_type == 5 && tag_size >= 20 {
                // Framebuffer tag
                let fb_addr = ptr::read_volatile(info.add(offset as usize + 8) as *const u64);
                let fb_pitch = ptr::read_volatile(info.add(offset as usize + 16) as *const u32);
                let fb_width = ptr::read_volatile(info.add(offset as usize + 20) as *const u32);
                let fb_height = ptr::read_volatile(info.add(offset as usize + 24) as *const u32);
                let fb_bpp = ptr::read_volatile(info.add(offset as usize + 28));
                crate::framebuffer::krust_framebuffer_init(fb_addr, fb_width, fb_height, fb_pitch, fb_bpp);
                crate::framebuffer::krust_framebuffer_clear(crate::framebuffer::COLOR_BLACK);
                crate::fb_console::fb_console_init(fb_width, fb_height, fb_pitch, fb_bpp, fb_addr as *mut u8);
                SAVED_FB_W = fb_width;
                SAVED_FB_H = fb_height;
                FB_AVAILABLE = true;
                serial(b"step: fb ok (multiboot2)\n\0");
                return;
            }
            // align to 8 bytes
            offset += (tag_size + 7) & !7;
        }
    } else if magic == 0x00000000 && addr != 0 {
        // PVH boot: addr points to hvm_start_info, check its magic at offset 0
        let hvm = addr as *const u8;
        let hvm_magic = ptr::read_volatile(hvm as *const u32);
        if hvm_magic == 0x36d76289 {
            // hvm_start_info framebuffer is at offset 124
            let fb_addr = ptr::read_volatile(hvm.add(124) as *const u64);
            let fb_size = ptr::read_volatile(hvm.add(132) as *const u32);
            // fb_size encodes width:16 | height:16 (if non-zero)
            let fb_w = fb_size & 0xFFFF;
            let fb_h = (fb_size >> 16) & 0xFFFF;
            if fb_addr != 0 && fb_w != 0 && fb_h != 0 {
                // QEMU standard VGA framebuffer is 640x480x32 with pitch = width*4
                let fb_pitch = fb_w * 4;
                let fb_bpp = 32u8;
                crate::framebuffer::krust_framebuffer_init(fb_addr, fb_w, fb_h, fb_pitch, fb_bpp);
                crate::framebuffer::krust_framebuffer_clear(crate::framebuffer::COLOR_BLACK);
                crate::fb_console::fb_console_init(fb_w, fb_h, fb_pitch, fb_bpp, fb_addr as *mut u8);
                SAVED_FB_W = fb_w;
                SAVED_FB_H = fb_h;
                FB_AVAILABLE = true;
                serial(b"step: fb ok (pvh)\n\0");
                return;
            }
            // Even if framebuffer tag not in hvm_start_info, VGA text buffer is at 0xB8000
            // Try standard VGA text mode framebuffer info
            serial(b"pvh: no framebuffer in hvm_start_info, trying VGA fallback\n\0");
        }
    }
    serial(b"step: fb not available\n\0");
}

static mut SAVED_FB_W: u32 = 0;
static mut SAVED_FB_H: u32 = 0;
static mut FB_AVAILABLE: bool = false;

unsafe fn serial(s: &[u8]) {
    crate::ns16550::krust_ns16550_write_str(s.as_ptr());
}

extern "C" fn block_dev_read(
    _node: *mut VNode, buf: *mut u8, size: u32, offset: u32,
) -> i32 {
    unsafe {
        let dev_index = (*_node).data as usize;
        if let Some(dev) = crate::block::get_device(dev_index) {
            let sector_size = dev.sector_size;
            if sector_size == 0 { return 0; }
            let start_sector = (offset as u64) / (sector_size as u64);
            let read_count = core::cmp::min((size + sector_size - 1) / sector_size, 256u32);
            if crate::block::block_read(dev_index, start_sector, read_count, buf) == 0 {
                size as i32
            } else {
                0
            }
        } else {
            0
        }
    }
}

extern "C" fn block_dev_write(
    _node: *mut VNode, buf: *const u8, size: u32, offset: u32,
) -> i32 {
    unsafe {
        let dev_index = (*_node).data as usize;
        if let Some(dev) = crate::block::get_device(dev_index) {
            let sector_size = dev.sector_size;
            if sector_size == 0 { return 0; }
            let start_sector = (offset as u64) / (sector_size as u64);
            let write_count = core::cmp::min((size + sector_size - 1) / sector_size, 256u32);
            if crate::block::block_write(dev_index, start_sector, write_count, buf) == 0 {
                size as i32
            } else {
                0
            }
        } else {
            0
        }
    }
}

unsafe fn fb_console(s: &[u8]) {
    let p = s.as_ptr();
    core::hint::black_box(p);
    crate::fb_console::fb_console_writestring(p);
}

unsafe fn fb_console_color(s: &[u8], color: u8) {
    let p = s.as_ptr();
    core::hint::black_box(p);
    crate::fb_console::fb_console_set_color(color & 0x0F, (color >> 4) & 0x0F);
    crate::fb_console::fb_console_writestring(p);
}

unsafe fn serial_num(n: u32) {
    let mut buf: [u8; 16] = [0; 16];
    let mut i = 0;
    let mut n = n;
    if n == 0 {
        buf[i] = b'0';
        i += 1;
    } else {
        while n > 0 {
            buf[i] = b'0' + ((n % 10) as u8);
            n /= 10;
            i += 1;
        }
    }
    for j in 0..(i/2) {
        let tmp = buf[j];
        buf[j] = buf[i-1-j];
        buf[i-1-j] = tmp;
    }
    crate::ns16550::krust_ns16550_write_str(buf.as_ptr());
}





extern "C" fn dev_null_read(
    _node: *mut VNode, _buf: *mut u8, _size: u32, _offset: u32,
) -> i32 {
    0
}

extern "C" fn dev_null_write(
    _node: *mut VNode, _buf: *const u8, size: u32, _offset: u32,
) -> i32 {
    size as i32
}

extern "C" fn dev_zero_read(
    _node: *mut VNode, buf: *mut u8, size: u32, _offset: u32,
) -> i32 {
    unsafe {
        for i in 0..size {
            ptr::write_volatile(buf.add(i as usize), 0);
        }
    }
    size as i32
}

extern "C" fn dev_zero_write(
    _node: *mut VNode, _buf: *const u8, size: u32, _offset: u32,
) -> i32 {
    size as i32
}

extern "C" fn dev_random_read(
    _node: *mut VNode, buf: *mut u8, size: u32, _offset: u32,
) -> i32 {
    unsafe {
        crate::rdrand::fill_bytes(core::slice::from_raw_parts_mut(buf, size as usize));
    }
    size as i32
}

extern "C" fn dev_random_write(
    _node: *mut VNode, _buf: *const u8, size: u32, _offset: u32,
) -> i32 {
    size as i32
}

extern "C" fn mouse_dev_read(
    _node: *mut VNode, buf: *mut u8, size: u32, _offset: u32,
) -> i32 {
    unsafe {
        if (size as usize) < core::mem::size_of::<crate::ps2mouse::MousePacket>() {
            return 0;
        }
        let mut pkt = crate::ps2mouse::MousePacket { flags: 0, dx: 0, dy: 0 };
        if !crate::ps2mouse::krust_ps2mouse_read_packet(&mut pkt as *mut _) {
            return 0;
        }
        ptr::copy_nonoverlapping(
            &pkt as *const _ as *const u8,
            buf,
            core::mem::size_of::<crate::ps2mouse::MousePacket>(),
        );
    }
    core::mem::size_of::<crate::ps2mouse::MousePacket>() as i32
}

extern "C" fn mouse_dev_write(
    _node: *mut VNode, _buf: *const u8, size: u32, _offset: u32,
) -> i32 {
    size as i32
}

extern "C" fn tty_dev_read(
    _node: *mut VNode, buf: *mut u8, size: u32, _offset: u32,
) -> i32 {
    unsafe {
        // /dev/tty reads from the current process's stdin
        let current = crate::scheduler::krust_sched_current();
        if !current.is_null() && (*current).stdin_fd >= 0 {
            crate::vfs::krust_vfs_pipe_read((*current).stdin_fd, buf, size)
        } else {
            // Fall back to PS/2 keyboard
            let mut total = 0u32;
            while total < size {
                let c = crate::ps2keyboard::krust_ps2kbd_getchar();
                if c == 0 {
                    break;
                }
                ptr::write_volatile(buf.add(total as usize), c);
                total += 1;
                if c == b'\n' {
                    break;
                }
            }
            total as i32
        }
    }
}

extern "C" fn tty_dev_write(
    _node: *mut VNode, buf: *const u8, size: u32, _offset: u32,
) -> i32 {
    unsafe {
        // /dev/tty writes to stdout (VGA + serial)
        crate::vga::krust_vga_write(buf, size as usize);
        crate::ns16550::krust_ns16550_write_buf(buf, size as usize);
    }
    size as i32
}

extern "C" fn kmsg_dev_read(
    _node: *mut VNode, buf: *mut u8, size: u32, _offset: u32,
) -> i32 {
    unsafe {
        // kmsg returns nothing for now (no ring buffer implemented)
        0
    }
}

extern "C" fn kmsg_dev_write(
    _node: *mut VNode, buf: *const u8, size: u32, _offset: u32,
) -> i32 {
    unsafe {
        crate::ns16550::krust_ns16550_write_buf(buf, size as usize);
        crate::vga::krust_vga_write(buf, size as usize);
    }
    size as i32
}

unsafe fn init_initrd() {
    krust_vfs_create_dir(b"/bin\0" as *const u8);
    krust_vfs_create_dir(b"/home\0" as *const u8);
    krust_vfs_create_dir(b"/dev\0" as *const u8);

    krust_vfs_create_device(
        b"/dev/null\0" as *const u8,
        Some(dev_null_read),
        Some(dev_null_write),
    );
    krust_vfs_create_device(
        b"/dev/zero\0" as *const u8,
        Some(dev_zero_read),
        Some(dev_zero_write),
    );
    krust_vfs_create_device(
        b"/dev/random\0" as *const u8,
        Some(dev_random_read),
        Some(dev_random_write),
    );
    // urandom is identical to random (no distinction in this OS)
    krust_vfs_create_device(
        b"/dev/urandom\0" as *const u8,
        Some(dev_random_read),
        Some(dev_random_write),
    );
    krust_vfs_create_device(
        b"/dev/mouse\0" as *const u8,
        Some(mouse_dev_read),
        Some(mouse_dev_write),
    );
    krust_vfs_create_device(
        b"/dev/tty\0" as *const u8,
        Some(tty_dev_read),
        Some(tty_dev_write),
    );
    krust_vfs_create_device(
        b"/dev/kmsg\0" as *const u8,
        Some(kmsg_dev_read),
        Some(kmsg_dev_write),
    );
    krust_vfs_create_dir(b"/dev/input\0" as *const u8);
    krust_vfs_create_dir(b"/tmp\0" as *const u8);

    let banner = b"Welcome to Elitra OS!\n\
This is a minimal x86-64 hobby operating system.\n\
Features: preemptive multitasking, VFS, framebuffer console.\n\0";
    let banner_len = krust_strlen(banner.as_ptr());
    krust_vfs_create_file(b"/home/readme.txt\0" as *const u8, banner.as_ptr(), banner_len as u32);

    let mut ver_buf = [0u8; 256];
    let mut vpos = 0;
    for &b in crate::KERNEL_NAME.as_bytes() { if vpos < ver_buf.len() { ver_buf[vpos] = b; vpos += 1; } }
    if vpos < ver_buf.len() { ver_buf[vpos] = b' '; vpos += 1; }
    if vpos < ver_buf.len() { ver_buf[vpos] = b'v'; vpos += 1; }
    for &b in crate::KERNEL_VERSION.as_bytes() { if vpos < ver_buf.len() { ver_buf[vpos] = b; vpos += 1; } }
    let suffix = b"\nArchitecture: \0";
    for &b in suffix { if b != 0 && vpos < ver_buf.len() { ver_buf[vpos] = b; vpos += 1; } }
    for &b in crate::KERNEL_ARCH.as_bytes() { if vpos < ver_buf.len() { ver_buf[vpos] = b; vpos += 1; } }
    let suffix2 = b"\nKernel: Preemptive multitasking\nFS: Virtual File System (ramfs)\n\0";
    for &b in suffix2 { if b != 0 && vpos < ver_buf.len() { ver_buf[vpos] = b; vpos += 1; } }
    ver_buf[vpos] = 0;
    let ver_len = vpos;
    krust_vfs_create_file(b"/etc/version\0" as *const u8, ver_buf.as_ptr(), ver_len as u32);

    let help = b"Elitra OS Shell\n\
  ls [path]    - List directory contents\n\
  cat <file>   - Display file contents\n\
  vfsinfo      - Show VFS information\n\0";
    let help_len = krust_strlen(help.as_ptr());
    krust_vfs_create_file(b"/home/help.txt\0" as *const u8, help.as_ptr(), help_len as u32);
}

#[no_mangle]
pub unsafe extern "C" fn kernel_main(magic: u32, addr: u32) {
    crate::ns16550::krust_ns16550_init();
    serial(b"\n=== Elitra OS Boot ===\n\0");

    crate::mm::mm::krust_mm_init(magic, addr);
    serial(b"step: mm ok\n\0");

    crate::vga::krust_vga_init();
    crate::vga::krust_vga_writestring_color(b"Elitra OS - Booting...\n\0" as *const u8, 0x02);
    serial(b"step: display init ok\n\0");

    fb_console(b"Installing GDT... \0");
    crate::gdt::install();
    serial(b"step: gdt ok\n\0");

    fb_console(b"Installing TSS... \0");
    crate::tss::init();
    serial(b"step: tss ok\n\0");

    fb_console(b"Installing IDT... \0");
    crate::idt::install();
    serial(b"step: idt ok\n\0");

    fb_console(b"Installing ISRs... \0");
    crate::isr::install();
    crate::isr::krust_isr_register_handler(0x40, reschedule_handler);
    serial(b"step: isr ok\n\0");

    fb_console(b"Installing IRQs... \0");
    crate::irq::install();
    serial(b"step: irq ok\n\0");

    fb_console(b"Initializing APIC... \0");
    crate::apic_hw::krust_apic_init();
    serial(b"step: apic ok\n\0");

    fb_console(b"Initializing PIT... \0");
    crate::pittimer::krust_pittimer_init(100);
    serial(b"step: pit ok\n\0");

    fb_console(b"Initializing keyboard... \0");
    krust_ps2kbd_init();
    serial(b"step: kbd ok\n\0");

    fb_console(b"Initializing RTC... \0");
    crate::cmos_rtc::krust_cmos_rtc_init();
    serial(b"step: rtc ok\n\0");

    fb_console(b"Initializing PS/2 mouse... \0");
    crate::ps2mouse::krust_ps2mouse_init();
    serial(b"step: mouse ok\n\0");

    fb_console(b"Enabling paging... \0");
    crate::paging::krust_paging_init();
    serial(b"step: paging ok\n\0");

    fb_console(b"Initializing FPU... \0");
    core::arch::asm!("fninit");
    serial(b"step: fpu ok\n\0");

    crate::cpuid::init();
    fb_console(b"Enabling SMEP/SMAP... \0");
    if crate::cpuid::has_smep() || crate::cpuid::has_smap() {
        let mut cr4: u64;
        core::arch::asm!("mov {v}, cr4", v = out(reg) cr4);
        if crate::cpuid::has_smep() {
            cr4 |= 1 << 20;
        }
        if crate::cpuid::has_smap() {
            cr4 |= 1 << 21;
        }
        core::arch::asm!("mov cr4, {v}", v = in(reg) cr4);
        serial(b"smep/smap ok\n\0");
    } else {
        serial(b"smep/smap not supported\n\0");
    }

    init_framebuffer(magic, addr);

     // Initialize mouse cursor
     unsafe {
         crate::mouse_cursor::mouse_cursor_init();
     }
     serial(b"step: mouse cursor ok\n\0");

     // Now use fb_console for all further output

     fb_console(b"Initializing heap... \0");
     crate::heap::krust_heap_init();
     serial(b"step: heap ok\n\0");

     fb_console(b"Initializing PCI... \0");
    crate::pci::PCI::config_read_word(0, 0, 0, 0);
    serial(b"step: pci ok\n\0");

    fb_console(b"Initializing VGA... \0");
    if crate::vga::krust_vga_vbe_init() {
        serial(b"step: vbe ok (1024x768x32)\n\0");
    } else {
        serial(b"step: vbe not available, using VGA text\n\0");
    }

    fb_console(b"Initializing ATA... \0");
    krust_ata_init();
    serial(b"step: ata ok\n\0");

    fb_console(b"Initializing NVMe... \0");
    if crate::nvme::nvme_init() {
        serial(b"step: nvme ok\n\0");
    } else {
        serial(b"step: nvme not found\n\0");
    }

    fb_console(b"Initializing HDA... \0");
    if crate::hda::init_hda() {
        serial(b"step: hda ok\n\0");
    } else {
        serial(b"step: hda not found\n\0");
    }

    fb_console(b"Initializing VFS... \0");
    crate::vfs::krust_vfs_init();
    krust_mount_init();
    init_initrd();

    crate::vfs::krust_vfs_create_dir(b"/mnt\0" as *const u8);

    crate::procfs::init_procfs();

    let mut fat_instance = crate::fat32::Instance {
        image: ptr::null_mut(),
        image_size: 0,
        bytes_per_sector: 0,
        sectors_per_cluster: 0,
        reserved_sectors: 0,
        num_fats: 0,
        sectors_per_fat: 0,
        root_cluster: 0,
        first_data_sector: 0,
        first_fat_sector: 0,
        total_clusters: 0,
        write_callback: None,
    };
    let mut fat32_ok = false;

    if krust_ata_drive_count() > 0 {
        for d in 0..krust_ata_drive_count() {
            let mut parts: [crate::ata_pio::Partition; 4] = [
                crate::ata_pio::Partition { valid: false, type_: 0, lba_start: 0, sector_count: 0 },
                crate::ata_pio::Partition { valid: false, type_: 0, lba_start: 0, sector_count: 0 },
                crate::ata_pio::Partition { valid: false, type_: 0, lba_start: 0, sector_count: 0 },
                crate::ata_pio::Partition { valid: false, type_: 0, lba_start: 0, sector_count: 0 },
            ];
            let np = krust_ata_find_partitions(d, parts.as_mut_ptr(), 4);
            if np > 0 {
                let mut total = parts[0].sector_count;
                if total > 4096 { total = 4096; }
                let buf = krust_malloc(total * 512);
                if !buf.is_null() {
                    let mut ata_ok = true;
                    let mut chunk = 0u32;
                    while chunk < total && ata_ok {
                        let n = if total - chunk > 256 { 256 } else { total - chunk };
                        if !krust_ata_read(d, parts[0].lba_start + chunk, n as u8, buf.add((chunk * 512) as usize)) {
                            ata_ok = false;
                        }
                        chunk += n;
                    }
                    if ata_ok {
                        let _ = krust_ata_mount_partition_buffer(d, parts[0].lba_start, total, buf);
                        if krust_fat32_init(&mut fat_instance as *mut _, buf, (total * 512) as usize) != 0 {
                            unsafe extern "C" fn ata_write_wrapper(_fs: *mut crate::fat32::Instance, byte_offset: u32, size: u32) {
                                crate::ata_pio::krust_ata_mark_dirty(byte_offset, size);
                            }
                            fat_instance.write_callback = Some(ata_write_wrapper);
                            if krust_fat32_mount(&mut fat_instance as *mut _, b"/mnt\0" as *const u8) != 0 {
                                krust_mount_mount(b"/mnt\0" as *const u8, 1, &mut fat_instance as *mut _ as *mut u8);
                                fb_console_color(b"FAT32 mounted from ATA\n\0", 0x02);
                                fat32_ok = true;
                            }
                        }
                    }
                    if !fat32_ok { krust_free(buf); }
                }
                break;
            }
        }
    }

    // Try NVMe if ATA didn't mount FAT32
    if !fat32_ok && crate::nvme::nvme_is_ready() {
        let mut nvme_parts: [crate::nvme::NvmePartition; 4] = [
            crate::nvme::NvmePartition { valid: false, type_: 0, lba_start: 0, sector_count: 0 },
            crate::nvme::NvmePartition { valid: false, type_: 0, lba_start: 0, sector_count: 0 },
            crate::nvme::NvmePartition { valid: false, type_: 0, lba_start: 0, sector_count: 0 },
            crate::nvme::NvmePartition { valid: false, type_: 0, lba_start: 0, sector_count: 0 },
        ];
        let np = crate::nvme::nvme_find_partitions(nvme_parts.as_mut_ptr(), 4);
        if np > 0 {
            let mut total = nvme_parts[0].sector_count;
            if total > 4096 { total = 4096; }
            let buf = krust_malloc((total * 512) as u32);
            if !buf.is_null() {
                let mut chunk = 0u64;
                let mut read_ok = true;
                while chunk < total && read_ok {
                    let n = if total - chunk > 256 { 256 } else { total - chunk };
                    if crate::nvme::nvme_read(
                        nvme_parts[0].lba_start + chunk,
                        n as u32,
                        buf.add((chunk * 512) as usize),
                    ) != 0 {
                        read_ok = false;
                    }
                    chunk += n;
                }
                if read_ok {
                    let _ = crate::nvme::nvme_mount_partition_buffer(
                        nvme_parts[0].lba_start, total, buf,
                    );
                    if krust_fat32_init(
                        &mut fat_instance as *mut _,
                        buf,
                        (total * 512) as usize,
                    ) != 0 {
                        unsafe extern "C" fn nvme_write_wrapper(
                            _fs: *mut crate::fat32::Instance,
                            byte_offset: u32,
                            size: u32,
                        ) {
                            crate::nvme::nvme_mark_dirty(byte_offset, size);
                        }
                        fat_instance.write_callback = Some(nvme_write_wrapper);
                        if krust_fat32_mount(
                            &mut fat_instance as *mut _,
                            b"/mnt\0" as *const u8,
                        ) != 0 {
                            krust_mount_mount(
                                b"/mnt\0" as *const u8,
                                1,
                                &mut fat_instance as *mut _ as *mut u8,
                            );
                            fb_console_color(b"FAT32 mounted from NVMe\n\0", 0x02);
                            fat32_ok = true;
                        }
                    }
                }
                if !fat32_ok { krust_free(buf); }
            }
        }
    }

    if !fat32_ok {
        let fat32_size = (&_binary_fat32_img_end as *const u8 as usize)
            - (&_binary_fat32_img_start as *const u8 as usize);
        if krust_fat32_init(
            &mut fat_instance as *mut _,
            &_binary_fat32_img_start as *const u8,
            fat32_size,
        ) != 0 {
            if krust_fat32_mount(&mut fat_instance as *mut _, b"/mnt\0" as *const u8) != 0 {
                krust_mount_mount(b"/mnt\0" as *const u8, 1, &mut fat_instance as *mut _ as *mut u8);
                fb_console_color(b"FAT32 mounted (embedded)\n\0", 0x02);
            } else {
                fb_console_color(b"FAT32 mount failed\n\0", 0x06);
            }
        } else {
            fb_console_color(b"FAT32 mount failed\n\0", 0x06);
        }
    }

    serial(b"step: vfs ok\n\0");

    fb_console(b"Initializing ACPI... \0");
    crate::acpi::krust_acpi_init();
    serial(b"step: acpi ok\n\0");

    fb_console(b"Detecting CPU features... \0");
    crate::cpuid::init();
    crate::cpuid::print_info();
    serial(b"step: cpuid ok\n\0");

    fb_console(b"Initializing RNG... \0");
    crate::rdrand::init();
    if crate::rdrand::is_available() {
        serial(b"RDRAND available\n\0");
    } else {
        serial(b"RDRAND not available, using fallback PRNG\n\0");
    }
    serial(b"step: rng ok\n\0");

    core::arch::asm!("out dx, al", in("dx") 0x3F8u16, in("al") b'!');
    core::arch::asm!("out dx, al", in("dx") 0x3F8u16, in("al") b'\n');

    //fb_console(b"Initializing I/O APIC... \0");
    serial(b"step: pre ioapic\n\0");
    if crate::acpi::IOAPIC_ADDR != 0 {
        serial(b"IOAPIC addr!=0\n\0");
        if crate::ioapic::init(crate::acpi::IOAPIC_ADDR) {
            serial(b"IOAPIC initialized\n\0");
        } else {
            serial(b"IOAPIC init failed\n\0");
        }
    } else {
        serial(b"no IOAPIC found\n\0");
    }
    serial(b"step: ioapic ok\n\0");

    fb_console(b"Initializing e1000 network... \0");
    if let Some(device) = crate::net::e1000::init_e1000() {
        let mac = device.mac_address();
        serial(b"e1000: MAC \0");
        for i in 0..6u8 {
            if i > 0 { serial(b":\0"); }
            let mut hex = [b'0'; 2];
            hex[0] = b"0123456789ABCDEF"[((mac[i as usize] >> 4) & 0x0F) as usize];
            hex[1] = b"0123456789ABCDEF"[(mac[i as usize] & 0x0F) as usize];
            serial(&hex);
        }
        serial(b"\n\0");
        crate::net::e1000::set_mac(mac);
        crate::net::e1000::set_ip([10, 0, 2, 15]);

        let bus = crate::net::e1000::get_pci_bus();
        let slot = crate::net::e1000::get_pci_slot();
        let func = crate::net::e1000::get_pci_func();
        let irq_line = crate::pci::PCI::read_irq_line(bus, slot, func);
        let irq_pin = crate::pci::PCI::read_irq_pin(bus, slot, func);
        serial(b"e1000: PCI IRQ line=\0");
        serial_num(irq_line as u32);
        serial(b" pin=\0");
        serial_num(irq_pin as u32);
        serial(b"\n\0");

        if crate::ioapic::is_available() && irq_line < 24 {
            crate::ioapic::set_redirect(irq_line, 0x41, 0, true, false);
            crate::ioapic::enable_redirect(irq_line);
            serial(b"e1000: IOAPIC routed\n\0");
        }

        crate::net::e1000::krust_e1000_irq_install(0);
        serial(b"e1000: IRQ handler installed\n\0");

        extern "C" {
            fn krust_net_init(mac: *const u8, ip: *const u8, netmask: *const u8, gateway: *const u8) -> i32;
        }
        let ip = crate::net::e1000::get_ip();
        let gw = [10u8, 0, 2, 2];
        let nm = [255u8, 255, 255, 0];
        krust_net_init(mac.as_ptr(), ip.as_ptr(), nm.as_ptr(), gw.as_ptr());

        crate::net::init_net_stack(device);

        fb_console_color(b"e1000 initialized with interrupts\n\0", 0x0A);
        serial(b"step: e1000 ok\n\0");
    } else {
        fb_console_color(b"e1000 not found\n\0", 0x0E);
        serial(b"step: e1000 not found\n\0");
    }

    // USB controller auto-detection via PCI
    fb_console(b"Scanning for USB controllers... \0");
    serial(b"usb: scanning PCI for USB controllers\n\0");
    {
        // Scan all PCI slots for USB controllers (class 0x0C, subclass 0x03)
        for bus in 0..=255u16 {
            for slot in 0..32u8 {
                for func in 0..8u8 {
                    let vid = crate::pci::PCI::config_read_word(bus as u8, slot, func, 0x00);
                    if vid == 0xFFFF { continue; }
                    let class_reg = crate::pci::PCI::config_read_dword(bus as u8, slot, func, 0x08);
                    let cc = ((class_reg >> 24) & 0xFF) as u8;
                    let sc = ((class_reg >> 16) & 0xFF) as u8;
                    let prog_if = ((class_reg >> 8) & 0xFF) as u8;
                    if cc == 0x0C && sc == 0x03 {
                        // Enable bus mastering + memory/IO space
                        let mut cmd = crate::pci::PCI::config_read_word(bus as u8, slot, func, 0x04);
                        cmd |= (1 << 1) | (1 << 2); // Memory Space + Bus Master
                        crate::pci::PCI::config_write_word(bus as u8, slot, func, 0x04, cmd);

                        match prog_if {
                            0x00 => {
                                // UHCI - I/O space, BAR4 has I/O port base
                                let bar4 = crate::pci::PCI::read_bar(bus as u8, slot, func, 4);
                                let io_base = (bar4 & 0xFFFC) as u16;
                                serial(b"uhci: found at IO=\0");
                                serial_num(io_base as u32);
                                serial(b"\n\0");
                                let rc = crate::usb::krust_usb_init_uhci(io_base);
                                if rc == 0 {
                                    fb_console_color(b"UHCI initialized\n\0", 0x0A);
                                    serial(b"uhci: init ok\n\0");
                                    // Enumerate USB storage on UHCI
                                    crate::usb::krust_usb_enumerate_storage();
                                }
                            }
                            0x20 => {
                                // EHCI - memory space, BAR0 has MMIO base
                                let bar0 = crate::pci::PCI::read_bar(bus as u8, slot, func, 0);
                                let mmio_phys = (bar0 & 0xFFFFFFF0) as u64;
                                serial(b"ehci: found at MMIO=\0");
                                serial_num(mmio_phys as u32);
                                serial(b"\n\0");
                                // Enable memory space access
                                cmd |= 1 << 1;
                                crate::pci::PCI::config_write_word(bus as u8, slot, func, 0x04, cmd);
                                let rc = crate::ehci::krust_ehci_init(mmio_phys);
                                if rc == 0 {
                                    fb_console_color(b"EHCI initialized\n\0", 0x0A);
                                    serial(b"ehci: init ok\n\0");
                                    // Enumerate USB storage on EHCI
                                    crate::ehci::krust_ehci_enumerate_storage();
                                }
                            }
                            _ => {
                                serial(b"usb: unsupported controller prog_if=\0");
                                serial_num(prog_if as u32);
                                serial(b"\n\0");
                            }
                        }
                    }
                }
            }
        }
    }

    fb_console(b"Detecting block devices... \0");
    crate::block::detect_all_devices();
    let bcount = crate::block::device_count();
    serial(b"block: \0");
    serial_num(bcount as u32);
    serial(b" device(s)\n\0");
    for i in 0..bcount {
        if let Some(dev) = crate::block::get_device(i) {
            let mut devpath = [0u8; 16];
            devpath[..4].copy_from_slice(b"/dev");
            devpath[4] = b'/';
            let nlen = dev.name.iter().position(|&c| c == 0).unwrap_or(dev.name.len());
            devpath[5..5 + nlen].copy_from_slice(&dev.name[..nlen]);

            // Create device node with read/write handlers backed by block layer
            let dev_idx = i as u32;
            crate::vfs::krust_vfs_create_device(
                devpath.as_ptr(),
                Some(block_dev_read),
                Some(block_dev_write),
            );

            serial(b"  /dev/\0");
            serial(&dev.name[..nlen]);
            serial(b" [");
            let driver_name: &[u8] = match dev.driver {
                crate::block::BlockDriverType::ATA => b"ATA",
                crate::block::BlockDriverType::NVMe => b"NVMe",
                crate::block::BlockDriverType::AHCI => b"AHCI",
                crate::block::BlockDriverType::VirtIO => b"VirtIO",
                crate::block::BlockDriverType::USB => b"USB",
            };
            serial(driver_name);
            serial(b"] \0");
            serial_num(dev.sector_size);
            serial(b" bytes/sect, \0");
            serial_num(dev.total_sectors as u32);
            serial(b" sectors\n\0");
        }
    }
    serial(b"step: block ok\n\0");

    fb_console(b"Initializing HPET... \0");
    if crate::hpet::init(0xFED00000) {
        serial(b"HPET initialized\n\0");
    } else {
        serial(b"HPET not available\n\0");
    }
    serial(b"step: hpet ok\n\0");

    fb_console(b"Starting secondary CPUs... \0");
    crate::smp::krust_smp_start_aps();
    serial(b"step: smp ok\n\0");

    fb_console(b"Initializing compositor... \0");
    if FB_AVAILABLE {
        unsafe { crate::gui::compositor_init(SAVED_FB_W, SAVED_FB_H); }
        crate::mouse_cursor::GUI_ACTIVE = true;
        serial(b"step: gui ok\n\0");
    }

    fb_console(b"Initializing scheduler... \0");
    crate::scheduler::krust_sched_init();
    serial(b"step: sched ok\n\0");

    fb_console(b"Initializing swap... \0");
    if crate::swap::krust_swap_is_ready() == 0 {
        let swap_drive = 0i32;
        let swap_lba = 0x100000u32;
        let swap_sectors = 0x10000u32;
        let rc = crate::swap::krust_swap_init(swap_drive, swap_lba, swap_sectors);
        if rc == 0 {
            fb_console_color(b"swap: 32MB on ATA drive 0\n\0", 0x0A);
            serial(b"swap: initialized\n\0");
        } else {
            fb_console_color(b"swap: init failed\n\0", 0x0E);
            serial(b"swap: init failed\n\0");
        }
    } else {
        serial(b"swap: already ready\n\0");
    }
    serial(b"step: swap ok\n\0");

     fb_console(b"Installing syscalls... \0");
     crate::syscalls::syscall_init();
     serial(b"step: syscall ok\n\0");

     fb_console(b"Enabling interrupts... \0");
     crate::irq::enable_interrupts();
     serial(b"step: interrupts enabled\n\0");

     fb_console(b"Elitra OS boot complete\n\0");

    serial(b"-- Pipe test --\n\0");
    let mut pipe_fds: [i32; 2] = [0; 2];
    if krust_vfs_pipe_create(pipe_fds.as_mut_ptr()) == 0 {
        let test_msg = b"Hello from pipe!";
        let len = krust_strlen(test_msg.as_ptr());
        krust_vfs_pipe_write(pipe_fds[1], test_msg.as_ptr(), len as u32);
        krust_vfs_pipe_close(pipe_fds[1]);
        let mut pbuf: [u8; 64] = [0; 64];
        let n = krust_vfs_pipe_read(pipe_fds[0], pbuf.as_mut_ptr(), 63);
        if n > 0 {
            pbuf[n as usize] = 0;
            serial(b"pipe test: read \0");
            serial_num(n as u32);
            serial(b" bytes: '\0");
            crate::ns16550::krust_ns16550_write_str(pbuf.as_ptr());
            serial(b"'\n\0");
        } else {
            serial(b"pipe test: FAILED\n\0");
        }
        krust_vfs_pipe_close(pipe_fds[0]);
    } else {
        serial(b"pipe test: FAILED (pipe_create)\n\0");
    }

    serial(b"-- Redir test --\n\0");
    let mut redir_fds: [i32; 2] = [0; 2];
    if krust_vfs_pipe_create(redir_fds.as_mut_ptr()) == 0 {
        let hello = b"Hello from ELF program!";
        let hlen = krust_strlen(hello.as_ptr());
        krust_vfs_pipe_write(redir_fds[1], hello.as_ptr(), hlen as u32);
        krust_vfs_pipe_close(redir_fds[1]);
        let mut rbuf: [u8; 128] = [0; 128];
        let rn = krust_vfs_pipe_read(redir_fds[0], rbuf.as_mut_ptr(), 127);
        if rn > 0 {
            rbuf[rn as usize] = 0;
            krust_vfs_write_file(b"/home/redir_test.txt\0" as *const u8, rbuf.as_ptr(), rn as u32);
            krust_ata_flush();
            if crate::nvme::nvme_is_ready() { crate::nvme::nvme_flush(); }
            let check = krust_vfs_resolve(b"/home/redir_test.txt\0" as *const u8);
            if !check.is_null() {
                serial(b"redir test: ok\n\0");
            } else {
                serial(b"redir test: FAILED (file not found after write)\n\0");
            }
        } else {
            serial(b"redir test: FAILED (read returned 0)\n\0");
        }
        krust_vfs_pipe_close(redir_fds[0]);
    } else {
        serial(b"redir test: FAILED (pipe_create)\n\0");
    }

    crate::terminal::krust_terminal_init();

    let mut init_node = krust_vfs_resolve(b"/mnt/bin/init.elf\0" as *const u8);
    if init_node.is_null() || (*init_node).type_ != 0 {
        init_node = krust_vfs_resolve(b"/bin/init.elf\0" as *const u8);
    }
    if init_node.is_null() || (*init_node).type_ != 0 {
        serial(b"init.elf not found, using kernel shell\n\0");
        crate::shell::shell_run();
    } else {
        let tid = krust_sched_create_init((*init_node).data, (*init_node).size);
        if tid >= 0 {
            serial(b"init: /bin/init.elf spawned\n\0");
            krust_sched_yield();
            serial(b"init: returned, using kernel shell\n\0");
            crate::shell::shell_run();
        } else {
            serial(b"init.elf load failed, using kernel shell\n\0");
            crate::shell::shell_run();
        }
    }
}
