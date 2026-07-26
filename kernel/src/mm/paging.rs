use core::ptr;
use crate::scheduler::{Registers, Task, VMA, TaskState, krust_sched_deliver_signals};

const COW_BIT: u64 = 0x200;
const PTE_MASK: u64 = 0x000FFFFFFFFFF000;
const FLAGS_MASK: u64 = 0x8000000000000FFF;
const PS_2MB: u64 = 0x80;
const PAGE_PRESENT: u64 = 0x1;
const PAGE_WRITE: u64 = 0x2;
const PAGE_USER: u64 = 0x4;
const PAGE_NX: u64 = 0x8000000000000000;
const PAGE_SIZE: u64 = 4096;
const HEAP_VADDR: u64 = 0x40000000;
const HEAP_INITIAL: u64 = 0x400000;
const PROT_WRITE: u64 = 2;
const PROT_READ: u64 = 1;
const PROT_EXEC: u64 = 4;

fn pml4_i(v: u64) -> u64 { (v >> 39) & 0x1FF }
fn pdp_i(v: u64) -> u64 { (v >> 30) & 0x1FF }
fn pd_i(v: u64) -> u64 { (v >> 21) & 0x1FF }
fn pt_i(v: u64) -> u64 { (v >> 12) & 0x1FF }

static mut PML4: *mut u64 = core::ptr::null_mut();
static mut HEAP_PHYS: u64 = 0;
static mut MMIO_NEXT: u64 = 0xFFFF900000000000;

unsafe fn memset(s: *mut u8, c: i32, n: usize) {
    for i in 0..n {
        ptr::write_volatile(s.add(i), c as u8);
    }
}

unsafe fn memcpy(dst: *mut u8, src: *const u8, n: usize) {
    for i in 0..n {
        ptr::write_volatile(dst.add(i), ptr::read_volatile(src.add(i)));
    }
}

unsafe fn serial_puthex(val: u64) {
    let hex = b"0123456789abcdef";
    krust_serial_putchar(b'0');
    krust_serial_putchar(b'x');
    for i in (0..16).rev() {
        let nibble = ((val >> (i * 4)) & 0xF) as usize;
        krust_serial_putchar(hex[nibble]);
    }
}

unsafe fn vga_puthex(val: u64) {
    let hex = b"0123456789abcdef";
    krust_vga_putchar(b'0');
    krust_vga_putchar(b'x');
    for i in (0..16).rev() {
        let nibble = ((val >> (i * 4)) & 0xF) as usize;
        krust_vga_putchar(hex[nibble]);
    }
}

unsafe fn serial_putdec(val: u32) {
    if val == 0 {
        krust_serial_putchar(b'0');
        return;
    }
    let mut buf = [0u8; 10];
    let mut n = val;
    let mut i = 0;
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    for j in (0..i).rev() {
        krust_serial_putchar(buf[j]);
    }
}

unsafe fn read_cr3() -> u64 {
    let val: u64;
    core::arch::asm!("mov {}, cr3", out(reg) val);
    val
}

unsafe fn write_cr3(val: u64) {
    core::arch::asm!("mov cr3, {}", in(reg) val);
}

extern "C" {
    fn krust_cow_init(total_frames: u32) -> i32;
    fn krust_cow_refinc(frame: u32);
    fn krust_cow_refdec(frame: u32) -> u32;
    fn krust_pmm_alloc_frame() -> usize;
    fn krust_pmm_free_frame(frame: usize);
    fn krust_pmm_get_total_frames() -> usize;
    fn krust_serial_writestring(s: *const u8);
    fn krust_serial_putchar(c: u8);
    fn krust_vga_writestring(s: *const u8);
    fn krust_vga_putchar(c: u8);
    fn krust_vga_writestring_color(s: *const u8, color: u8);
    fn krust_isr_register_handler(vec: u8, handler: unsafe extern "C" fn(*mut Registers));
    fn krust_vmm_find(head: *mut VMA, addr: u64) -> *mut VMA;
    fn krust_sched_current() -> *mut Task;
    fn krust_sched_exit(code: u32);
}

unsafe fn table_alloc() -> *mut u64 {
    let frame = krust_pmm_alloc_frame();
    if frame == !0usize {
        return ptr::null_mut();
    }
    let phys = (frame * PAGE_SIZE as usize) as *mut u64;
    memset(phys as *mut u8, 0, PAGE_SIZE as usize);
    phys
}

unsafe fn active_pml4() -> *mut u64 {
    read_cr3() as *mut u64
}

unsafe fn cow_resolve(addr: u64, _err: u64) -> bool {
    let cr3 = read_cr3();
    let p4 = cr3 as *mut u64;
    let i4 = pml4_i(addr) as usize;
    let i3 = pdp_i(addr) as usize;
    let i2 = pd_i(addr) as usize;
    let i1 = pt_i(addr) as usize;

    if (*p4.add(i4) & 1) == 0 { return false; }
    let p3 = (*p4.add(i4) & PTE_MASK) as *mut u64;
    if (*p3.add(i3) & 1) == 0 { return false; }
    let p2 = (*p3.add(i3) & PTE_MASK) as *mut u64;
    if (*p2.add(i2) & 1) == 0 { return false; }
    if (*p2.add(i2) & PS_2MB) != 0 { return false; }
    let p1 = (*p2.add(i2) & PTE_MASK) as *mut u64;
    let ent = *p1.add(i1);
    if (ent & 1) == 0 { return false; }
    if (ent & COW_BIT) == 0 { return false; }

    let phys = ent & PTE_MASK;
    let frame = (phys / 4096) as u32;
    let refs = krust_cow_refdec(frame);

    if refs == 0 {
        let new_flags = (ent & FLAGS_MASK) & !COW_BIT;
        let new_flags = new_flags | 0x2;
        *p1.add(i1) = (phys & PTE_MASK) | (new_flags & FLAGS_MASK) | 1;
        core::arch::asm!("invlpg [{}]", in(reg) addr);
        return true;
    }

    let new_frame = krust_pmm_alloc_frame();
    if new_frame == !0usize { return false; }
    let new_page = (new_frame * 4096) as *mut u8;
    memcpy(new_page, phys as *const u8, 4096);
    *p1.add(i1) = ((new_page as u64) & PTE_MASK) | 0x7;
    core::arch::asm!("invlpg [{}]", in(reg) addr);
    true
}

#[no_mangle]
pub unsafe extern "C" fn cow_resolve_page(target_cr3: u64, addr: u64) -> bool {
    let p4 = target_cr3 as *mut u64;
    let i4 = pml4_i(addr) as usize;
    let i3 = pdp_i(addr) as usize;
    let i2 = pd_i(addr) as usize;
    let i1 = pt_i(addr) as usize;

    if (*p4.add(i4) & 1) == 0 { return false; }
    let p3 = (*p4.add(i4) & PTE_MASK) as *mut u64;
    if (*p3.add(i3) & 1) == 0 { return false; }
    let p2 = (*p3.add(i3) & PTE_MASK) as *mut u64;
    if (*p2.add(i2) & 1) == 0 { return false; }
    if (*p2.add(i2) & PS_2MB) != 0 { return false; }
    let p1 = (*p2.add(i2) & PTE_MASK) as *mut u64;
    let ent = *p1.add(i1);
    if (ent & 1) == 0 { return false; }
    if (ent & COW_BIT) == 0 { return false; }

    let phys = ent & PTE_MASK;
    let frame = (phys / 4096) as u32;
    let refs = krust_cow_refdec(frame);

    if refs == 0 {
        let new_flags = (ent & FLAGS_MASK) & !COW_BIT;
        let new_flags = new_flags | 0x2;
        *p1.add(i1) = (phys & PTE_MASK) | (new_flags & FLAGS_MASK) | 1;
        core::arch::asm!("invlpg [{}]", in(reg) addr);
        return true;
    }

    let new_frame = krust_pmm_alloc_frame();
    if new_frame == !0usize { return false; }
    let new_page = (new_frame * 4096) as *mut u8;
    memcpy(new_page, phys as *const u8, 4096);
    *p1.add(i1) = ((new_page as u64) & PTE_MASK) | 0x7;
    core::arch::asm!("invlpg [{}]", in(reg) addr);
    true
}

unsafe fn vma_lazy_alloc(addr: u64, err: u64) -> bool {
    let t = krust_sched_current();
    if t.is_null() || (*t).vma_list.is_null() { return false; }

    let vma = krust_vmm_find((*t).vma_list, addr);
    if vma.is_null() { return false; }

    if (err & 2) != 0 && ((*vma).flags & PROT_WRITE) == 0 { return false; }
    if (err & 2) == 0 && ((*vma).flags & PROT_READ) == 0 { return false; }

    let aligned = addr & !0xFFF;
    let frame = krust_pmm_alloc_frame();
    if frame == !0usize { return false; }
    let phys = (frame * 4096) as *mut u64;
    memset(phys as *mut u8, 0, PAGE_SIZE as usize);

    let mut pte_flags = PAGE_PRESENT | PAGE_USER;
    if ((*vma).flags & PROT_WRITE) != 0 { pte_flags |= PAGE_WRITE; }
    if ((*vma).flags & PROT_EXEC) == 0 { pte_flags |= PAGE_NX; }

    krust_paging_map_page(aligned, phys as u64, pte_flags);
    (*t).pages += 1;
    true
}

unsafe fn alloc_heap_pages() {
    let mut off = 0;
    while off < HEAP_INITIAL {
        let frame = krust_pmm_alloc_frame();
        if frame == !0usize {
            krust_serial_writestring(b"heap OOM\n\0".as_ptr());
            break;
        }
        let phys = (frame * 4096) as u64;
        krust_paging_map_page(HEAP_VADDR + off, phys, 3);
        if HEAP_PHYS == 0 { HEAP_PHYS = phys; }
        off += 4096;
    }
    krust_serial_writestring(b"heap: mapped \0".as_ptr());
    serial_puthex(HEAP_INITIAL / 1024);
    krust_serial_writestring(b" KB at 0x\0".as_ptr());
    serial_puthex(HEAP_VADDR);
    krust_serial_putchar(b'\n');
}

#[no_mangle]
pub unsafe extern "C" fn krust_paging_init() {
    PML4 = read_cr3() as *mut u64;

    krust_serial_writestring(b"paging: PML4=0x\0".as_ptr());
    serial_puthex(PML4 as u64);
    krust_serial_putchar(b'\n');

    let total_frames = krust_pmm_get_total_frames() as u32;
    if krust_cow_init(total_frames) == 0 {
        krust_serial_writestring(b"cow: refcount table initialized\n\0".as_ptr());
    } else {
        krust_serial_writestring(b"cow: init failed (no memory)\n\0".as_ptr());
    }

    krust_isr_register_handler(14, krust_page_fault_handler);

    HEAP_PHYS = 0;
    alloc_heap_pages();

    krust_vga_writestring_color(b"Paging: 4-level paging active\n\0".as_ptr(), 0x02);
    krust_serial_writestring(b"paging: init done\n\0".as_ptr());
}

#[no_mangle]
pub unsafe extern "C" fn krust_paging_map_page(virt: u64, phys: u64, flags: u64) -> bool {
    let i4 = pml4_i(virt) as usize;
    let i3 = pdp_i(virt) as usize;
    let i2 = pd_i(virt) as usize;
    let i1 = pt_i(virt) as usize;

    let t4 = active_pml4();
    if (*t4.add(i4) & 1) == 0 {
        let nt = table_alloc();
        if nt.is_null() { return false; }
        *t4.add(i4) = (nt as u64) | 3;
    }
    let t3 = (*t4.add(i4) & PTE_MASK) as *mut u64;
    if (*t3.add(i3) & 1) == 0 {
        let nt = table_alloc();
        if nt.is_null() { return false; }
        *t3.add(i3) = (nt as u64) | 3;
    }
    let t2 = (*t3.add(i3) & PTE_MASK) as *mut u64;
    if (*t2.add(i2) & 1) == 0 {
        let nt = table_alloc();
        if nt.is_null() { return false; }
        *t2.add(i2) = (nt as u64) | 3;
    }
    if (*t2.add(i2) & PS_2MB) != 0 {
        let base = *t2.add(i2) & PTE_MASK;
        let old_flags = (*t2.add(i2)) & !PS_2MB & !PTE_MASK & !1;
        let nt = table_alloc();
        if nt.is_null() { return false; }
        for j in 0..512u64 {
            *nt.add(j as usize) = (base + j * 4096) | old_flags | 1;
        }
        *t2.add(i2) = (nt as u64) | 3;
    }
    let t1 = (*t2.add(i2) & PTE_MASK) as *mut u64;
    *t1.add(i1) = (phys & PTE_MASK) | (flags & FLAGS_MASK) | 1;
    core::arch::asm!("invlpg [{}]", in(reg) virt);
    true
}

#[no_mangle]
pub unsafe extern "C" fn krust_paging_map_page_in(pml4_paddr: u64, virt: u64, phys: u64, flags: u64) -> bool {
    let i4 = pml4_i(virt) as usize;
    let i3 = pdp_i(virt) as usize;
    let i2 = pd_i(virt) as usize;
    let i1 = pt_i(virt) as usize;

    let t4 = pml4_paddr as *mut u64;
    if (*t4.add(i4) & 1) == 0 {
        let nt = table_alloc();
        if nt.is_null() { return false; }
        *t4.add(i4) = (nt as u64) | 3;
    }
    let t3 = (*t4.add(i4) & PTE_MASK) as *mut u64;
    if (*t3.add(i3) & 1) == 0 {
        let nt = table_alloc();
        if nt.is_null() { return false; }
        *t3.add(i3) = (nt as u64) | 3;
    }
    let t2 = (*t3.add(i3) & PTE_MASK) as *mut u64;
    if (*t2.add(i2) & 1) == 0 {
        let nt = table_alloc();
        if nt.is_null() { return false; }
        *t2.add(i2) = (nt as u64) | 3;
    }
    if (*t2.add(i2) & PS_2MB) != 0 {
        let base = *t2.add(i2) & PTE_MASK;
        let old_flags = (*t2.add(i2)) & !PS_2MB & !PTE_MASK & !1;
        let nt = table_alloc();
        if nt.is_null() { return false; }
        for j in 0..512u64 {
            *nt.add(j as usize) = (base + j * 4096) | old_flags | 1;
        }
        *t2.add(i2) = (nt as u64) | 3;
    }
    let t1 = (*t2.add(i2) & PTE_MASK) as *mut u64;
    *t1.add(i1) = (phys & PTE_MASK) | (flags & FLAGS_MASK) | 1;
    true
}

unsafe fn tlb_flush_all() {
    let cr3: u64;
    core::arch::asm!("mov {}, cr3", out(reg) cr3);
    core::arch::asm!("mov cr3, {}", in(reg) cr3);
}

#[no_mangle]
pub unsafe extern "C" fn krust_tlb_flush_range(virt: u64, count: u64) {
    for i in 0..count {
        let addr = virt + i * 4096;
        core::arch::asm!("invlpg [{}]", in(reg) addr);
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_paging_unmap_page(virt: u64) {
    let i4 = pml4_i(virt) as usize;
    let i3 = pdp_i(virt) as usize;
    let i2 = pd_i(virt) as usize;
    let i1 = pt_i(virt) as usize;

    let t4 = active_pml4();
    if (*t4.add(i4) & 1) == 0 { return; }
    let t3 = (*t4.add(i4) & PTE_MASK) as *mut u64;
    if (*t3.add(i3) & 1) == 0 { return; }
    let t2 = (*t3.add(i3) & PTE_MASK) as *mut u64;
    if (*t2.add(i2) & 1) == 0 { return; }
    if (*t2.add(i2) & PS_2MB) != 0 {
        *t2.add(i2) = 0;
        core::arch::asm!("invlpg [{}]", in(reg) virt);
        return;
    }
    let t1 = (*t2.add(i2) & PTE_MASK) as *mut u64;
    *t1.add(i1) = 0;
    core::arch::asm!("invlpg [{}]", in(reg) virt);
}

#[no_mangle]
pub unsafe extern "C" fn krust_paging_get_phys(virt: u64) -> u64 {
    let i4 = pml4_i(virt) as usize;
    let i3 = pdp_i(virt) as usize;
    let i2 = pd_i(virt) as usize;
    let i1 = pt_i(virt) as usize;

    let t4 = active_pml4();
    if (*t4.add(i4) & 1) == 0 { return !0; }
    let t3 = (*t4.add(i4) & PTE_MASK) as *mut u64;
    if (*t3.add(i3) & 1) == 0 { return !0; }
    let t2 = (*t3.add(i3) & PTE_MASK) as *mut u64;
    if (*t2.add(i2) & 1) == 0 { return !0; }
    if (*t2.add(i2) & PS_2MB) != 0 {
        return (*t2.add(i2) & PTE_MASK) | (virt & 0x1FFFFF);
    }
    let t1 = (*t2.add(i2) & PTE_MASK) as *mut u64;
    if (*t1.add(i1) & 1) == 0 { return !0; }
    (*t1.add(i1) & PTE_MASK) | (virt & 0xFFF)
}

#[no_mangle]
pub unsafe extern "C" fn krust_paging_page_directory() -> *mut u64 {
    PML4
}

#[no_mangle]
pub unsafe extern "C" fn krust_paging_clone_kernel_dir() -> *mut u64 {
    let np = table_alloc();
    if np.is_null() { return ptr::null_mut(); }
    for i in 256..512 {
        *np.add(i) = *PML4.add(i);
    }

    // Deep-copy lower-half identity mapping, splitting 2MB huge pages into 4KB.
    // This ensures kernel code (at 0x100000), heap, and kernel stacks are
    // accessible when CR3 switches to the task's page table, while allowing
    // user pages to be mapped without conflicting with2MB huge page entries.
    if (*PML4.add(0) & 1) != 0 {
        let src_pdp = (*PML4.add(0) & PTE_MASK) as *const u64;
        let new_pdp = table_alloc();
        if !new_pdp.is_null() {
            *np.add(0) = (new_pdp as u64) | ((*PML4.add(0)) & FLAGS_MASK) | 1;

            for i3 in 0..512usize {
                if (*src_pdp.add(i3) & 1) == 0 { continue; }
                if (*src_pdp.add(i3) & PS_2MB) != 0 { continue; }

                let src_pd = (*src_pdp.add(i3) & PTE_MASK) as *const u64;
                let new_pd = table_alloc();
                if new_pd.is_null() { continue; }
                *new_pdp.add(i3) = (new_pd as u64) | ((*src_pdp.add(i3)) & FLAGS_MASK) | 1;

                for i2 in 0..512usize {
                    if (*src_pd.add(i2) & 1) == 0 { continue; }

                    if (*src_pd.add(i2) & PS_2MB) != 0 {
                        let base = *src_pd.add(i2) & PTE_MASK;
                        let flags = (*src_pd.add(i2) & FLAGS_MASK) & !PS_2MB;
                        let new_pt = table_alloc();
                        if new_pt.is_null() { continue; }
                        for i1 in 0..512u64 {
                            *new_pt.add(i1 as usize) = (base + i1 * 0x1000) | flags | 1;
                        }
                        *new_pd.add(i2) = (new_pt as u64) | flags | 1;
                    } else {
                        let src_pt = (*src_pd.add(i2) & PTE_MASK) as *const u64;
                        let new_pt = table_alloc();
                        if new_pt.is_null() { continue; }
                        *new_pd.add(i2) = (new_pt as u64) | ((*src_pd.add(i2)) & FLAGS_MASK) | 1;
                        for i1 in 0..512 {
                            *new_pt.add(i1) = *src_pt.add(i1);
                        }
                    }
                }
            }
        }
    }

    np
}

#[no_mangle]
pub unsafe extern "C" fn krust_paging_copy_user_pages(src: *mut u64, dst: *mut u64) {
    for i4 in 0..256 {
        if (*src.add(i4) & 1) == 0 { continue; }
        let s3 = (*src.add(i4) & PTE_MASK) as *mut u64;
        if (*dst.add(i4) & 1) == 0 {
            let nt = table_alloc();
            if nt.is_null() { return; }
            *dst.add(i4) = (nt as u64) | 7;
        }
        let d3 = (*dst.add(i4) & PTE_MASK) as *mut u64;
        for i3 in 0..512 {
            if (*s3.add(i3) & 1) == 0 { continue; }
            let s2 = (*s3.add(i3) & PTE_MASK) as *mut u64;
            if (*d3.add(i3) & 1) == 0 {
                let nt = table_alloc();
                if nt.is_null() { return; }
                *d3.add(i3) = (nt as u64) | 7;
            }
            let d2 = (*d3.add(i3) & PTE_MASK) as *mut u64;
            for i2 in 0..512 {
                if (*s2.add(i2) & 1) == 0 { continue; }
                if (*s2.add(i2) & PS_2MB) != 0 {
                    let base = *s2.add(i2) & PTE_MASK;
                    let fl = (*s2.add(i2) & FLAGS_MASK) & !PS_2MB;
                    let s_pt = table_alloc();
                    let d_pt = table_alloc();
                    if s_pt.is_null() || d_pt.is_null() { return; }
                    for i1 in 0..512u64 {
                        let paddr = base + i1 * 4096;
                        let frame = (paddr / 4096) as u32;
                        krust_cow_refinc(frame);
                        let cow = paddr | (fl & !2) | COW_BIT | 1;
                        *s_pt.add(i1 as usize) = cow;
                        *d_pt.add(i1 as usize) = cow;
                    }
                    *s2.add(i2) = (s_pt as u64) | (fl & !PS_2MB) | 1;
                    *d2.add(i2) = (d_pt as u64) | (fl & !PS_2MB) | 1;
                    continue;
                }
                let s1 = (*s2.add(i2) & PTE_MASK) as *mut u64;
                if (*d2.add(i2) & 1) == 0 {
                    let nt = table_alloc();
                    if nt.is_null() { return; }
                    *d2.add(i2) = (nt as u64) | 7;
                }
                let d1 = (*d2.add(i2) & PTE_MASK) as *mut u64;
                for i1 in 0..512 {
                    if (*s1.add(i1) & 1) == 0 { continue; }
                    let paddr = *s1.add(i1) & PTE_MASK;
                    let frame = (paddr / 4096) as u32;
                    let flags = *s1.add(i1) & FLAGS_MASK;
                    if (flags & PAGE_WRITE) != 0 && (flags & PAGE_USER) != 0 {
                        krust_cow_refinc(frame);
                        let cow_ent = (paddr & PTE_MASK) | (flags & !PAGE_WRITE) | COW_BIT | 1;
                        *s1.add(i1) = cow_ent;
                        *d1.add(i1) = cow_ent;
                    } else {
                        *d1.add(i1) = *s1.add(i1);
                    }
                }
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_paging_free_user_pages(pml4_paddr: u64) {
    let t4 = pml4_paddr as *mut u64;
    for i4 in 0..256 {
        if (*t4.add(i4) & 1) == 0 { continue; }
        let t3 = (*t4.add(i4) & PTE_MASK) as *mut u64;
        for i3 in 0..512 {
            if (*t3.add(i3) & 1) == 0 { continue; }
            let t2 = (*t3.add(i3) & PTE_MASK) as *mut u64;
            for i2 in 0..512 {
                if (*t2.add(i2) & 1) == 0 { continue; }
                if (*t2.add(i2) & PS_2MB) != 0 { continue; }
                let t1 = (*t2.add(i2) & PTE_MASK) as *mut u64;
                for i1 in 0..512 {
                    if (*t1.add(i1) & 1) == 0 { continue; }
                    let phys = *t1.add(i1) & PTE_MASK;
                    let frame = phys as usize / 4096;
                    if (*t1.add(i1) & COW_BIT) != 0 {
                        if krust_cow_refdec(frame as u32) == 0 {
                            krust_pmm_free_frame(frame);
                        }
                    } else {
                        krust_pmm_free_frame(frame);
                    }
                }
                if (*t2.add(i2) & 1) != 0 {
                    krust_pmm_free_frame((*t2.add(i2) & PTE_MASK) as usize / 4096);
                }
            }
            if (*t3.add(i3) & 1) != 0 {
                krust_pmm_free_frame((*t3.add(i3) & PTE_MASK) as usize / 4096);
            }
        }
        if (*t4.add(i4) & 1) != 0 {
            krust_pmm_free_frame((*t4.add(i4) & PTE_MASK) as usize / 4096);
        }
    }
    krust_pmm_free_frame(pml4_paddr as usize / 4096);
}

#[no_mangle]
pub unsafe extern "C" fn krust_paging_free_page_tables(pml4_paddr: u64) {
    let t4 = pml4_paddr as *mut u64;
    for i4 in 0..256 {
        if (*t4.add(i4) & 1) == 0 { continue; }
        let t3 = (*t4.add(i4) & PTE_MASK) as *mut u64;
        for i3 in 0..512 {
            if (*t3.add(i3) & 1) == 0 { continue; }
            let t2 = (*t3.add(i3) & PTE_MASK) as *mut u64;
            for i2 in 0..512 {
                if (*t2.add(i2) & 1) == 0 { continue; }
                if (*t2.add(i2) & PS_2MB) != 0 { continue; }
                let t1 = (*t2.add(i2) & PTE_MASK) as *mut u64;
                krust_pmm_free_frame(t1 as usize / 4096);
            }
            krust_pmm_free_frame(t2 as usize / 4096);
        }
        krust_pmm_free_frame(t3 as usize / 4096);
    }
    krust_pmm_free_frame(pml4_paddr as usize / 4096);
}

#[no_mangle]
pub unsafe extern "C" fn krust_page_fault_handler(r: *mut Registers) {
    let addr: u64;
    core::arch::asm!("mov {}, cr2", out(reg) addr);

    crate::ns16550::krust_ns16550_write_str(b"*** PF addr=\0".as_ptr());
    serial_puthex(addr);
    crate::ns16550::krust_ns16550_write_str(b" rip=\0".as_ptr());
    serial_puthex((*r).rip);
    crate::ns16550::krust_ns16550_write_str(b" err=\0".as_ptr());
    serial_puthex((*r).err_code);
    crate::ns16550::krust_ns16550_write_str(b"\n\0".as_ptr());

    let err = (*r).err_code;
    let current = krust_sched_current();

    if (err & 0x7) == 0x6 && !current.is_null() {
        if cow_resolve(addr, err) { return; }
    }

    if (err & 1) == 0 {
        if crate::swap::swap_handle_fault(addr) { return; }
        if vma_lazy_alloc(addr, err) { return; }

        // Try to free memory by evicting a page to swap before OOM killing
        if crate::swap::swap_evict_any() {
            if vma_lazy_alloc(addr, err) { return; }
        }

        if !current.is_null() {
            crate::scheduler::krust_oom_kill();
            if vma_lazy_alloc(addr, err) { return; }
        }
    }

    crate::ns16550::krust_ns16550_write_str(b"*** PAGE FAULT addr=\0".as_ptr());
    serial_puthex(addr);
    crate::ns16550::krust_ns16550_write_str(b" rip=\0".as_ptr());
    serial_puthex((*r).rip);
    crate::ns16550::krust_ns16550_write_str(b" err=\0".as_ptr());
    serial_puthex(err);
    crate::ns16550::krust_ns16550_write_str(b" rsp=\0".as_ptr());
    serial_puthex((*r).rsp);
    crate::ns16550::krust_ns16550_write_str(b"\n\0".as_ptr());

    krust_vga_putchar(b'\n');
    krust_vga_writestring(b"PAGE FAULT at 0x\0".as_ptr());
    vga_puthex(addr);
    krust_vga_writestring(b" RIP=0x\0".as_ptr());
    vga_puthex((*r).rip);
    krust_vga_writestring(b" RSP=0x\0".as_ptr());
    vga_puthex((*r).rsp);
    krust_vga_putchar(b'\n');
    krust_vga_writestring(b"  \0".as_ptr());
    if (err & 1) != 0 {
        krust_vga_writestring(b"protection\0".as_ptr());
    } else {
        krust_vga_writestring(b"not-present\0".as_ptr());
    }
    krust_vga_putchar(b' ');
    if (err & 2) != 0 {
        krust_vga_writestring(b"write\0".as_ptr());
    } else {
        krust_vga_writestring(b"read\0".as_ptr());
    }
    krust_vga_putchar(b' ');
    if (err & 4) != 0 {
        krust_vga_writestring(b"user\0".as_ptr());
    } else {
        krust_vga_writestring(b"supervisor\0".as_ptr());
    }
    if (err & 8) != 0 {
        krust_vga_writestring(b" reserved\0".as_ptr());
    }
    krust_vga_putchar(b'\n');

    if !current.is_null() {
        krust_serial_writestring(b"pagefault: kill task \0".as_ptr());
        serial_putdec((*current).id);
        krust_serial_putchar(b'\n');

        (*current).sig_pending |= 1 << 11;
        krust_sched_deliver_signals(r);

        if (*current).state == TaskState::EXITED {
            let code = (*current).exit_code;
            krust_sched_exit(code);
        }
        return;
    }

    core::arch::asm!("cli");
    loop { core::arch::asm!("hlt"); }
}

#[no_mangle]
pub unsafe extern "C" fn krust_paging_pd_to_phys(pd: *mut u64) -> u64 {
    pd as u64
}

#[no_mangle]
pub unsafe extern "C" fn krust_paging_identity_map(_s: u64, _e: u64) {}

#[no_mangle]
pub unsafe extern "C" fn krust_map_mmio(phys: u64, size: u64) -> u64 {
    let start = MMIO_NEXT;
    let mut off = 0;
    while off < size {
        if !krust_paging_map_page(MMIO_NEXT + off, phys + off, 3) {
            return 0;
        }
        off += 4096;
    }
    MMIO_NEXT += (size + 4095) & !4095;
    start
}

#[no_mangle]
pub unsafe extern "C" fn krust_page_size() -> u64 {
    PAGE_SIZE
}
