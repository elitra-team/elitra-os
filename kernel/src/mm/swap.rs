use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use crate::heap::{krust_malloc, krust_free};
use crate::klib::{krust_memset, krust_memcpy};
use crate::spinlock::SpinLock;

const PAGE_SIZE: usize = 4096;
const SECTOR_SIZE: usize = 512;
const SECTORS_PER_PAGE: usize = PAGE_SIZE / SECTOR_SIZE;
const MAX_SWAP_SLOTS: usize = 4096;
const LOW_WATERMARK: usize = 64;

const SWAP_MAGIC: u32 = 0x53574150;
const SWAP_PTE_MARKER: u64 = 0x400; // bit 10 — must not conflict with COW_BIT (0x200, bit 9)
const PTE_MASK: u64 = 0x000FFFFFFFFFF000;

static SWAP_READY: AtomicBool = AtomicBool::new(false);
static SWAP_DRIVE: AtomicUsize = AtomicUsize::new(0);
static SWAP_LBA_START: AtomicUsize = AtomicUsize::new(0);
static SWAP_TOTAL_SLOTS: AtomicUsize = AtomicUsize::new(0);

static SWAP_BITMAP_LOCK: SpinLock<()> = SpinLock::new(());
static mut SWAP_USED: [bool; MAX_SWAP_SLOTS] = [false; MAX_SWAP_SLOTS];
static mut SWAP_BITMAP: *mut u8 = core::ptr::null_mut();
static mut SWAP_BITMAP_WORDS: usize = 0;

unsafe fn bitmap_test(slot: usize) -> bool {
    let bm = SWAP_BITMAP;
    if bm.is_null() || slot / 8 >= SWAP_BITMAP_WORDS * 8 {
        return false;
    }
    core::ptr::read_volatile(bm.add(slot / 8)) & (1 << (slot % 8)) != 0
}

unsafe fn bitmap_set(slot: usize) {
    let bm = SWAP_BITMAP;
    if bm.is_null() { return; }
    let addr = bm.add(slot / 8);
    let old = core::ptr::read_volatile(addr);
    core::ptr::write_volatile(addr, old | (1 << (slot % 8)));
    SWAP_USED[slot] = true;
}

unsafe fn bitmap_clear(slot: usize) {
    let bm = SWAP_BITMAP;
    if bm.is_null() { return; }
    let addr = bm.add(slot / 8);
    let old = core::ptr::read_volatile(addr);
    core::ptr::write_volatile(addr, old & !(1 << (slot % 8)));
    SWAP_USED[slot] = false;
}

#[no_mangle]
pub unsafe extern "C" fn krust_swap_init(drive: i32, lba_start: u32, total_sectors: u32) -> i32 {
    if total_sectors < SECTORS_PER_PAGE as u32 * 2 {
        return -1;
    }
    let swap_sectors = total_sectors - SECTORS_PER_PAGE as u32;
    let total_slots = swap_sectors / SECTORS_PER_PAGE as u32;
    if total_slots == 0 || total_slots as usize > MAX_SWAP_SLOTS {
        return -1;
    }

    let bitmap_words = (total_slots as usize + 63) / 64;
    let bitmap_size = bitmap_words * 8;
    let bitmap = krust_malloc(bitmap_size as u32);
    if bitmap.is_null() {
        return -1;
    }
    krust_memset(bitmap, 0x00, bitmap_size);

    SWAP_DRIVE.store(drive as usize, Ordering::Relaxed);
    SWAP_LBA_START.store(lba_start as usize, Ordering::Relaxed);
    SWAP_TOTAL_SLOTS.store(total_slots as usize, Ordering::Relaxed);
    SWAP_BITMAP = bitmap;
    SWAP_BITMAP_WORDS = bitmap_words;

    for i in 0..total_slots as usize {
        SWAP_USED[i] = false;
    }

    SWAP_READY.store(true, Ordering::Relaxed);
    0
}

unsafe fn swap_alloc_slot() -> i32 {
    let _guard = SWAP_BITMAP_LOCK.lock();
    let total = SWAP_TOTAL_SLOTS.load(Ordering::Relaxed);
    for i in 0..total {
        if !bitmap_test(i) {
            bitmap_set(i);
            return i as i32;
        }
    }
    -1
}

unsafe fn swap_free_slot(slot: usize) {
    let _guard = SWAP_BITMAP_LOCK.lock();
    let total = SWAP_TOTAL_SLOTS.load(Ordering::Relaxed);
    if slot < total {
        bitmap_clear(slot);
    }
}

unsafe fn swap_io(drive: i32, lba: u32, count: u8, buf: *mut u8, write: bool) -> bool {
    if write {
        crate::ata_pio::krust_ata_write(drive, lba, count, buf as *const u8)
    } else {
        crate::ata_pio::krust_ata_read(drive, lba, count, buf)
    }
}

pub unsafe fn swap_read_page(slot: u32, buf: *mut u8) -> bool {
    if !SWAP_READY.load(Ordering::Relaxed) { return false; }
    let total = SWAP_TOTAL_SLOTS.load(Ordering::Relaxed);
    if slot as usize >= total { return false; }
    let drive = SWAP_DRIVE.load(Ordering::Relaxed) as i32;
    let lba_start = SWAP_LBA_START.load(Ordering::Relaxed) as u32;
    let data_start = lba_start + SECTORS_PER_PAGE as u32;
    let lba = data_start + slot * SECTORS_PER_PAGE as u32;
    swap_io(drive, lba, SECTORS_PER_PAGE as u8, buf, false)
}

pub unsafe fn swap_write_page(slot: u32, buf: *const u8) -> bool {
    if !SWAP_READY.load(Ordering::Relaxed) { return false; }
    let total = SWAP_TOTAL_SLOTS.load(Ordering::Relaxed);
    if slot as usize >= total { return false; }
    let drive = SWAP_DRIVE.load(Ordering::Relaxed) as i32;
    let lba_start = SWAP_LBA_START.load(Ordering::Relaxed) as u32;
    let data_start = lba_start + SECTORS_PER_PAGE as u32;
    let lba = data_start + slot * SECTORS_PER_PAGE as u32;
    swap_io(drive, lba, SECTORS_PER_PAGE as u8, buf as *mut u8, true)
}

pub fn swap_is_pte(pte: u64) -> bool {
    (pte & 1) == 0 && (pte & SWAP_PTE_MARKER) != 0 && pte != 0
}

pub fn swap_pte_to_slot(pte: u64) -> u32 {
    ((pte >> 12) & 0xFFFFF) as u32
}

pub fn swap_slot_to_pte(slot: u32) -> u64 {
    SWAP_PTE_MARKER | ((slot as u64 & 0xFFFFF) << 12)
}

pub unsafe fn swap_evict_page(vaddr: u64, target_cr3: u64) -> bool {
    if !SWAP_READY.load(Ordering::Relaxed) { return false; }

    let slot = swap_alloc_slot();
    if slot < 0 { return false; }

    let i4 = ((vaddr >> 39) & 0x1FF) as usize;
    let i3 = ((vaddr >> 30) & 0x1FF) as usize;
    let i2 = ((vaddr >> 21) & 0x1FF) as usize;
    let i1 = ((vaddr >> 12) & 0x1FF) as usize;

    let p4 = target_cr3 as *mut u64;
    if (*p4.add(i4) & 1) == 0 { swap_free_slot(slot as usize); return false; }
    let p3 = (*p4.add(i4) & PTE_MASK) as *mut u64;
    if (*p3.add(i3) & 1) == 0 { swap_free_slot(slot as usize); return false; }
    let p2 = (*p3.add(i3) & PTE_MASK) as *mut u64;
    if (*p2.add(i2) & 1) == 0 { swap_free_slot(slot as usize); return false; }
    if (*p2.add(i2) & 0x80) != 0 { swap_free_slot(slot as usize); return false; }
    let p1 = (*p2.add(i2) & PTE_MASK) as *mut u64;
    let old_pte = *p1.add(i1);
    if (old_pte & 1) == 0 { swap_free_slot(slot as usize); return false; }

    let cow_bit: u64 = 0x200;
    if (old_pte & cow_bit) != 0 {
        let saved_cr3 = {
            let v: u64;
            core::arch::asm!("mov {}, cr3", out(reg) v);
            v
        };
        if target_cr3 != saved_cr3 {
            core::arch::asm!("mov cr3, {}", in(reg) target_cr3);
        }
        crate::paging::cow_resolve_page(target_cr3, vaddr);
        if target_cr3 != saved_cr3 {
            core::arch::asm!("mov cr3, {}", in(reg) saved_cr3);
        }
        let refreshed_pte = *p1.add(i1);
        if (refreshed_pte & cow_bit) != 0 {
            swap_free_slot(slot as usize);
            return false;
        }
    }

    let phys = old_pte & PTE_MASK;
    let buf = krust_malloc(PAGE_SIZE as u32);
    if buf.is_null() { swap_free_slot(slot as usize); return false; }
    krust_memcpy(buf, phys as *const u8, PAGE_SIZE);

    if !swap_write_page(slot as u32, buf) {
        krust_free(buf);
        swap_free_slot(slot as usize);
        return false;
    }

    let orig_flags = old_pte & 0xFF0;
    *p1.add(i1) = swap_slot_to_pte(slot as u32) | (orig_flags & !0x1);
    core::arch::asm!("invlpg [{}]", in(reg) vaddr);

    krust_free(buf);

    let frame = phys as usize / PAGE_SIZE;
    crate::pmm::krust_pmm_free_frame(frame);

    true
}

unsafe fn swap_in_from_pte(pte: u64, vaddr: u64) -> bool {
    if !swap_is_pte(pte) { return false; }

    let slot = swap_pte_to_slot(pte);
    let frame = crate::pmm::krust_pmm_alloc_frame();
    if frame == !0usize { return false; }

    let phys = (frame * PAGE_SIZE) as *mut u8;
    krust_memset(phys, 0, PAGE_SIZE);

    if !swap_read_page(slot, phys) {
        crate::pmm::krust_pmm_free_frame(frame);
        return false;
    }

    let orig_flags = (pte & 0xFF0) & !SWAP_PTE_MARKER;
    let cr3: u64;
    core::arch::asm!("mov {}, cr3", out(reg) cr3);
    let p4 = cr3 as *mut u64;
    let i4 = ((vaddr >> 39) & 0x1FF) as usize;
    let i3 = ((vaddr >> 30) & 0x1FF) as usize;
    let i2 = ((vaddr >> 21) & 0x1FF) as usize;
    let i1 = ((vaddr >> 12) & 0x1FF) as usize;

    if (*p4.add(i4) & 1) == 0 { crate::pmm::krust_pmm_free_frame(frame); return false; }
    let p3 = (*p4.add(i4) & PTE_MASK) as *mut u64;
    if (*p3.add(i3) & 1) == 0 { crate::pmm::krust_pmm_free_frame(frame); return false; }
    let p2 = (*p3.add(i3) & PTE_MASK) as *mut u64;
    if (*p2.add(i2) & 1) == 0 { crate::pmm::krust_pmm_free_frame(frame); return false; }
    if (*p2.add(i2) & 0x80) != 0 { crate::pmm::krust_pmm_free_frame(frame); return false; }
    let p1 = (*p2.add(i2) & PTE_MASK) as *mut u64;

    *p1.add(i1) = (phys as u64 & PTE_MASK) | orig_flags | 1;
    core::arch::asm!("invlpg [{}]", in(reg) vaddr);

    swap_free_slot(slot as usize);
    true
}

pub unsafe fn swap_handle_fault(vaddr: u64) -> bool {
    if !SWAP_READY.load(Ordering::Relaxed) { return false; }
    let aligned = vaddr & !0xFFF;

    let cr3: u64;
    core::arch::asm!("mov {}, cr3", out(reg) cr3);
    let p4 = cr3 as *mut u64;
    let i4 = ((aligned >> 39) & 0x1FF) as usize;
    let i3 = ((aligned >> 30) & 0x1FF) as usize;
    let i2 = ((aligned >> 21) & 0x1FF) as usize;
    let i1 = ((aligned >> 12) & 0x1FF) as usize;

    if (*p4.add(i4) & 1) == 0 { return false; }
    let p3 = (*p4.add(i4) & PTE_MASK) as *mut u64;
    if (*p3.add(i3) & 1) == 0 { return false; }
    let p2 = (*p3.add(i3) & PTE_MASK) as *mut u64;
    if (*p2.add(i2) & 1) == 0 { return false; }
    if (*p2.add(i2) & 0x80) != 0 { return false; }
    let p1 = (*p2.add(i2) & PTE_MASK) as *mut u64;
    let pte = *p1.add(i1);

    swap_in_from_pte(pte, aligned)
}

pub unsafe fn swap_evict_process(task: *mut crate::scheduler::Task) -> bool {
    if task.is_null() { return false; }
    if (*task).state == crate::scheduler::TaskState::EXITED { return false; }
    if (*task).id == 0 { return false; }

    let vma = (*task).vma_list;
    if vma.is_null() { return false; }

    let task_cr3 = (*task).ctx.cr3;
    let mut cur = vma;
    while !cur.is_null() {
        let mut vaddr = (*cur).start & !0xFFF;
        while vaddr < (*cur).end {
            let i4 = ((vaddr >> 39) & 0x1FF) as usize;
            let i3 = ((vaddr >> 30) & 0x1FF) as usize;
            let i2 = ((vaddr >> 21) & 0x1FF) as usize;
            let i1 = ((vaddr >> 12) & 0x1FF) as usize;

            let mut has_page = false;
            let p4 = task_cr3 as *mut u64;
            if (*p4.add(i4) & 1) != 0 {
                let p3 = (*p4.add(i4) & PTE_MASK) as *mut u64;
                if (*p3.add(i3) & 1) != 0 {
                    let p2 = (*p3.add(i3) & PTE_MASK) as *mut u64;
                    if (*p2.add(i2) & 1) != 0 && (*p2.add(i2) & 0x80) == 0 {
                        let p1 = (*p2.add(i2) & PTE_MASK) as *mut u64;
                        let pte = *p1.add(i1);
                        if (pte & 1) != 0 {
                            has_page = true;
                        }
                    }
                }
            }

            if has_page {
                if swap_evict_page(vaddr, task_cr3) {
                    return true;
                }
            }
            vaddr += PAGE_SIZE as u64;
        }
        cur = (*cur).next;
    }
    false
}

pub unsafe fn swap_evict_any() -> bool {
    if !SWAP_READY.load(Ordering::Relaxed) { return false; }
    let next_id = crate::scheduler::krust_sched_get_next_id();
    for i in 1..next_id {
        let task = crate::scheduler::krust_sched_get_task(i);
        if !task.is_null() {
            if swap_evict_process(task) {
                return true;
            }
        }
    }
    false
}

#[no_mangle]
pub unsafe extern "C" fn krust_swap_is_ready() -> i32 {
    if SWAP_READY.load(Ordering::Relaxed) { 1 } else { 0 }
}
