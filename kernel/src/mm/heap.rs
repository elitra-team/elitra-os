use core::mem::size_of;
use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering};

const HEAP_START: u64 = 0x40000000;
const HEAP_SIZE: u64 = 0x10000000;

#[repr(C, packed)]
struct HeapBlock {
    size: usize,
    free: bool,
    next: *mut HeapBlock,
}

static mut HEAP_HEAD: *mut HeapBlock = ptr::null_mut();
static LOCK: AtomicBool = AtomicBool::new(false);

struct SpinLock {
    flags: u64,
}

impl SpinLock {
    fn acquire() -> SpinLock {
        let flags: u64;
        unsafe {
            core::arch::asm!("pushfq; pop {}; cli", out(reg) flags, options(nostack));
        }
        while LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        SpinLock { flags }
    }
}

impl Drop for SpinLock {
    fn drop(&mut self) {
        LOCK.store(false, Ordering::Release);
        unsafe {
            core::arch::asm!(
                "push {flags}",
                "popfq",
                flags = in(reg) self.flags,
                options(nostack),
            );
        }
    }
}

unsafe fn find_block(prev: *mut *mut HeapBlock, size: usize) -> *mut HeapBlock {
    let mut current = HEAP_HEAD;
    *prev = ptr::null_mut();
    while !current.is_null() {
        if (*current).free && (*current).size >= size {
            return current;
        }
        *prev = current;
        current = (*current).next;
    }
    ptr::null_mut()
}

unsafe fn split_block(block: *mut HeapBlock, size: usize) {
    if (*block).size < size + size_of::<HeapBlock>() + 16 {
        return;
    }
    let new_block = (block as usize + size_of::<HeapBlock>() + size) as *mut HeapBlock;
    (*new_block).size = (*block).size - size - size_of::<HeapBlock>();
    (*new_block).free = true;
    (*new_block).next = (*block).next;
    (*block).size = size;
    (*block).next = new_block;
}

unsafe fn merge_adjacent() {
    let mut current = HEAP_HEAD;
    while !current.is_null() && !(*current).next.is_null() {
        if (*current).free && (*(*current).next).free {
            (*current).size += size_of::<HeapBlock>() + (*(*current).next).size;
            (*current).next = (*(*current).next).next;
        } else {
            current = (*current).next;
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_heap_init() {
    HEAP_HEAD = HEAP_START as *mut HeapBlock;
    (*HEAP_HEAD).size = HEAP_SIZE as usize - size_of::<HeapBlock>();
    (*HEAP_HEAD).free = true;
    (*HEAP_HEAD).next = ptr::null_mut();
}

#[no_mangle]
pub unsafe extern "C" fn krust_heap_alloc(size: u32) -> *mut u8 {
    let _guard = SpinLock::acquire();
    if HEAP_HEAD.is_null() {
        return ptr::null_mut();
    }
    let mut size = size as usize;
    if size == 0 {
        size = 1;
    }
    size = (size + 15) & !15;
    let mut prev: *mut HeapBlock = ptr::null_mut();
    let block = find_block(&mut prev, size);
    if block.is_null() {
        return ptr::null_mut();
    }
    split_block(block, size);
    (*block).free = false;
    (block as usize + size_of::<HeapBlock>()) as *mut u8
}

#[no_mangle]
pub unsafe extern "C" fn krust_heap_free(ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }
    let _guard = SpinLock::acquire();
    let block = (ptr as usize - size_of::<HeapBlock>()) as *mut HeapBlock;
    if (*block).free {
        return;
    }
    (*block).free = true;
    merge_adjacent();
}

#[no_mangle]
pub unsafe extern "C" fn krust_heap_realloc(ptr: *mut u8, size: u32) -> *mut u8 {
    if ptr.is_null() {
        return krust_heap_alloc(size);
    }
    if size == 0 {
        krust_heap_free(ptr);
        return ptr::null_mut();
    }
    let _guard = SpinLock::acquire();
    let block = (ptr as usize - size_of::<HeapBlock>()) as *mut HeapBlock;
    if (*block).size >= size as usize {
        split_block(block, size as usize);
        return ptr;
    }
    drop(_guard);
    let new_ptr = krust_heap_alloc(size);
    if !new_ptr.is_null() {
        let old_block = (ptr as usize - size_of::<HeapBlock>()) as *mut HeapBlock;
        let copy_size = core::cmp::min((*old_block).size, size as usize);
        ptr::copy_nonoverlapping(ptr, new_ptr, copy_size);
        krust_heap_free(ptr);
    }
    new_ptr
}

#[no_mangle]
pub unsafe extern "C" fn krust_malloc(size: u32) -> *mut u8 {
    krust_heap_alloc(size)
}

#[no_mangle]
pub unsafe extern "C" fn krust_free(ptr: *mut u8) {
    krust_heap_free(ptr)
}

#[no_mangle]
pub unsafe extern "C" fn krust_realloc(ptr: *mut u8, size: u32) -> *mut u8 {
    krust_heap_realloc(ptr, size)
}
