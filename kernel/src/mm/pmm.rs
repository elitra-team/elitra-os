use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};

const PAGE_SIZE: usize = 4096;
const FRAMES_MAX: usize = 1048576; // 4GB

static BITMAP: AtomicPtr<u8> = AtomicPtr::new(core::ptr::null_mut());
static BITMAP_SIZE: AtomicUsize = AtomicUsize::new(0);
static TOTAL_FRAMES: AtomicUsize = AtomicUsize::new(0);
static FIRST_FREE: AtomicUsize = AtomicUsize::new(0);
static PMM_LOCK: AtomicBool = AtomicBool::new(false);

unsafe fn bitmap_test(bit: usize) -> bool {
    let bm = BITMAP.load(Ordering::Relaxed);
    if bm.is_null() || bit / 8 >= BITMAP_SIZE.load(Ordering::Relaxed) {
        return true;
    }
    core::ptr::read_volatile(bm.add(bit / 8)) & (1 << (bit % 8)) != 0
}

unsafe fn bitmap_set(bit: usize) {
    let bm = BITMAP.load(Ordering::Relaxed);
    if bm.is_null() || bit / 8 >= BITMAP_SIZE.load(Ordering::Relaxed) {
        return;
    }
    let addr = bm.add(bit / 8);
    let old = core::ptr::read_volatile(addr);
    core::ptr::write_volatile(addr, old | (1 << (bit % 8)));
}

unsafe fn bitmap_clear(bit: usize) {
    let bm = BITMAP.load(Ordering::Relaxed);
    if bm.is_null() || bit / 8 >= BITMAP_SIZE.load(Ordering::Relaxed) {
        return;
    }
    let addr = bm.add(bit / 8);
    let old = core::ptr::read_volatile(addr);
    core::ptr::write_volatile(addr, old & !(1 << (bit % 8)));
}

unsafe fn find_first_free() -> usize {
    let total = TOTAL_FRAMES.load(Ordering::Relaxed);
    let start = FIRST_FREE.load(Ordering::Relaxed);
    for i in start..total {
        if !bitmap_test(i) {
            return i;
        }
    }
    !0
}

#[no_mangle]
pub unsafe extern "C" fn krust_pmm_init(bitmap: *mut u8, bitmap_size: usize, total_frames: usize) {
    BITMAP.store(bitmap, Ordering::Relaxed);
    BITMAP_SIZE.store(bitmap_size, Ordering::Relaxed);
    TOTAL_FRAMES.store(total_frames, Ordering::Relaxed);
    FIRST_FREE.store(0, Ordering::Relaxed);
    for i in 0..bitmap_size {
        core::ptr::write_volatile(bitmap.add(i), 0xFF);
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_pmm_mark_used(bitmap: *mut u8, start: usize, end: usize) {
    BITMAP.store(bitmap, Ordering::Relaxed);
    for i in start..end {
        bitmap_set(i);
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_pmm_free_frames(bitmap: *mut u8, start_frame: usize, count: usize) {
    BITMAP.store(bitmap, Ordering::Relaxed);
    for i in start_frame..(start_frame + count) {
        bitmap_clear(i);
    }
    let prev = FIRST_FREE.load(Ordering::Relaxed);
    if start_frame < prev {
        FIRST_FREE.store(start_frame, Ordering::Relaxed);
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_pmm_alloc_frame() -> usize {
    let flags: u64;
    core::arch::asm!("pushfq; pop {}; cli", out(reg) flags, options(nostack));
    while PMM_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    let frame = find_first_free();
    if frame == !0 {
        PMM_LOCK.store(false, Ordering::Release);
        core::arch::asm!("push {f}; popfq", f = in(reg) flags, options(nostack));
        return !0;
    }
    bitmap_set(frame);
    FIRST_FREE.store(frame + 1, Ordering::Relaxed);
    PMM_LOCK.store(false, Ordering::Release);
    core::arch::asm!("push {f}; popfq", f = in(reg) flags, options(nostack));
    frame
}

#[no_mangle]
pub unsafe extern "C" fn krust_pmm_free_frame(frame: usize) {
    let flags: u64;
    core::arch::asm!("pushfq; pop {}; cli", out(reg) flags, options(nostack));
    while PMM_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    bitmap_clear(frame);
    let prev = FIRST_FREE.load(Ordering::Relaxed);
    if frame < prev {
        FIRST_FREE.store(frame, Ordering::Relaxed);
    }
    PMM_LOCK.store(false, Ordering::Release);
    core::arch::asm!("push {f}; popfq", f = in(reg) flags, options(nostack));
}

#[no_mangle]
pub unsafe extern "C" fn krust_pmm_get_bitmap() -> *mut u8 {
    BITMAP.load(Ordering::Relaxed)
}

#[no_mangle]
pub unsafe extern "C" fn krust_pmm_get_total_frames() -> usize {
    TOTAL_FRAMES.load(Ordering::Relaxed)
}

#[no_mangle]
pub unsafe extern "C" fn krust_mm_cpp_pmm_init(mem_upper_kb: u32, placement_addr: u64) {
    let total_mem = (mem_upper_kb as u64 + 1024) * 1024;
    let total_frames_val = (total_mem / PAGE_SIZE as u64) as usize;
    let bitmap_size_val = (total_frames_val + 7) / 8;

    let bitmap_addr = placement_addr as *mut u8;

    TOTAL_FRAMES.store(total_frames_val, Ordering::Relaxed);
    BITMAP_SIZE.store(bitmap_size_val, Ordering::Relaxed);
    BITMAP.store(bitmap_addr, Ordering::Relaxed);
    FIRST_FREE.store(0, Ordering::Relaxed);

    for i in 0..bitmap_size_val {
        core::ptr::write_volatile(bitmap_addr.add(i), 0xFF);
    }

    let used_frames = ((placement_addr + bitmap_size_val as u64 + PAGE_SIZE as u64 - 1) / PAGE_SIZE as u64) as usize;
    for i in used_frames..total_frames_val {
        bitmap_clear(i);
    }

    FIRST_FREE.store(used_frames, Ordering::Relaxed);
}

#[no_mangle]
pub unsafe extern "C" fn krust_pmm_free_count() -> usize {
    let total = TOTAL_FRAMES.load(Ordering::Relaxed);
    let bm = BITMAP.load(Ordering::Relaxed);
    if bm.is_null() { return 0; }
    let mut count: usize = 0;
    for i in 0..total {
        if !bitmap_test(i) {
            count += 1;
        }
    }
    count
}
