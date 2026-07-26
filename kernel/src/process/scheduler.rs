use core::ptr;
use core::sync::atomic::{AtomicU32, Ordering};
use crate::heap::{krust_free, krust_malloc};
use crate::util::config;
use config::{PAGE_SIZE, USTACK_VADDR, USTACK_PAGES, BRK_INITIAL};

// ─── Types mirrored from C++ ───────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, PartialEq)]
pub enum TaskState {
    READY = 0,
    RUNNING = 1,
    BLOCKED = 2,
    WAITING = 3,
    EXITED = 4,
}

impl TaskState {
    pub fn as_cpp_name(&self) -> &'static str {
        match self {
            Self::READY => "TaskState::READY",
            Self::RUNNING => "TaskState::RUNNING", 
            Self::BLOCKED => "TaskState::BLOCKED",
            Self::WAITING => "TaskState::WAITING",
            Self::EXITED => "TaskState::EXITED",
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct TaskContext {
    pub rsp: u64,
    pub cr3: u64,
}

const NSIG: usize = config::NSIG;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct SigHandler {
    pub handler_addr: u64,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct VMA {
    pub start: u64,
    pub end: u64,
    pub flags: u64,
    pub next: *mut VMA,
}

#[repr(C)]
pub struct VNode {
    pub name: [u8; 64],
    pub type_: u8,
    pub size: u32,
    pub parent: *mut VNode,
    pub children: *mut VNode,
    pub next: *mut VNode,
    pub data: *mut u8,
    pub dev_read: Option<extern "C" fn(*mut VNode, *mut u8, u32, u32) -> i32>,
    pub dev_write: Option<extern "C" fn(*mut VNode, *const u8, u32, u32) -> i32>,
    pub link_target: [u8; 256],
    pub uid: u16,
    pub gid: u16,
    pub mode: u16,
}

pub const MAX_FDS: usize = config::MAX_FDS;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct FDEntry {
    pub node: *mut VNode,
    pub offset: u32,
    pub flags: u32,
    pub used: bool,
    pub refcount: u32,
}

#[repr(C)]
pub struct Registers {
    pub r15: u64, pub r14: u64, pub r13: u64, pub r12: u64,
    pub r11: u64, pub r10: u64, pub r9: u64, pub r8: u64,
    pub rdi: u64, pub rsi: u64, pub rbp: u64, pub rsp: u64,
    pub rbx: u64, pub rdx: u64, pub rcx: u64, pub rax: u64,
    pub int_no: u64, pub err_code: u64,
    pub rip: u64, pub cs: u64, pub rflags: u64, pub user_rsp: u64, pub ss: u64,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Task {
    pub id: u32,
    pub ppid: u32,
    pub pgid: u32,
    pub sid: u32,
    pub state: TaskState,
    pub ctx: TaskContext,
    pub kstack: *mut u64,
    pub ustack: *mut u64,
    pub pages: u32,
    pub stdin_fd: i32,
    pub stdout_fd: i32,
    pub stderr_fd: i32,
    pub fd_table: *mut FDEntry,
    pub exit_code: u32,
    pub fpu_state: *mut u8,
    pub cwd: [u8; 128],
    pub next: *mut Task,
    pub parent: *mut Task,
    pub child_head: *mut Task,
    pub child_tail: *mut Task,
    pub sibling_next: *mut Task,
    pub sig_handlers: [SigHandler; 32],
    pub sig_pending: u32,
    pub sig_blocked: u32,
    pub has_child_exit: bool,
    pub vma_list: *mut VMA,
    pub program_brk: u64,
    pub priority: u32,
    pub sleep_until: u32,
    pub uid: u16,
    pub gid: u16,
    pub oom_score_adj: i16,
    pub env_count: u32,
    pub env_keys: [*mut u8; 32],
    pub env_vals: [*mut u8; 32],
}

// ─── Constants ─────────────────────────────────────────────────

const USTACK_SIZE: u32 = config::USTACK_PAGES * 4096;
const SIGKILL: i32 = 9;

// ─── kernel_stack_ptr (per-CPU via gs:[0]) ────────────────────

#[no_mangle]
pub static mut kernel_stack_ptr: u64 = 0;

// ─── C++ sync stub (no-op, C++ code removed) ──────────────────

#[no_mangle]
pub unsafe extern "C" fn krust_sched_sync_cpp() {
    // No-op: C++ Scheduler::current removed
}

// ─── External (asm) functions ──────────────────────────────────

extern "C" {
    fn context_switch(old_rsp: *mut u64, new_rsp: u64);
    fn yield_task();
    fn task_resume(ctx: *const TaskContext);
}

// ─── External (C++ bridge) functions ───────────────────────────

extern "C" {
    fn krust_pmm_alloc_frame() -> usize;
    fn krust_pmm_free_frame(frame: usize);
    fn krust_paging_clone_kernel_dir() -> *mut u64;
    fn krust_paging_page_directory() -> *mut u64;
    fn krust_paging_map_page(virt: u64, phys: u64, flags: u64) -> bool;
    fn krust_paging_unmap_page(virt: u64);
    fn krust_paging_get_phys(virt: u64) -> u64;
    fn krust_paging_free_user_pages(pml4: u64);
    fn krust_paging_copy_user_pages(src: *mut u64, dst: *mut u64);
    fn krust_vmm_find(head: *mut VMA, addr: u64) -> *mut VMA;
    fn krust_vmm_add(head: *mut *mut VMA, start: u64, end: u64, flags: u64) -> *mut VMA;
    fn krust_vmm_remove(head: *mut *mut VMA, start: u64, end: u64) -> i32;
    fn krust_vmm_free_all(head: *mut *mut VMA);
    fn krust_vmm_has_overlap(head: *mut VMA, start: u64, end: u64) -> i32;
    fn krust_tss_set_kernel_stack(rsp: u64);
    fn krust_elf_load(data: *const u8, size: u32, entry: *mut u64) -> i32;
    fn krust_vfs_resolve(path: *const u8) -> *mut VNode;
    fn krust_page_size() -> u64;
}

// ─── Global state ────────────────────────────────────────────────

static mut TASKS: [Task; 128] = [Task {
    id: 0, ppid: 0, pgid: 0, sid: 0, state: TaskState::READY, ctx: TaskContext { rsp: 0, cr3: 0 },
    kstack: ptr::null_mut(), ustack: ptr::null_mut(), pages: 0,
    stdin_fd: 0, stdout_fd: 0, stderr_fd: 0, fd_table: ptr::null_mut(), exit_code: 0,
    fpu_state: ptr::null_mut(),
    cwd: [0u8; 128],
    next: ptr::null_mut(), parent: ptr::null_mut(),
    child_head: ptr::null_mut(), child_tail: ptr::null_mut(),
    sibling_next: ptr::null_mut(),
    sig_handlers: [SigHandler { handler_addr: 0 }; 32],
    sig_pending: 0, sig_blocked: 0,
    has_child_exit: false,
    vma_list: ptr::null_mut(), program_brk: 0,
    priority: 0, sleep_until: 0,
    uid: 0, gid: 0, oom_score_adj: 0,
    env_count: 0,
    env_keys: [ptr::null_mut(); 32],
    env_vals: [ptr::null_mut(); 32],
}; 128];

// Per-CPU current task pointers
const MAX_CPUS_SCHED: usize = config::MAX_CPUS;

// Per-CPU current task pointers
static mut PER_CPU_CURRENT: [*mut Task; MAX_CPUS_SCHED] = [ptr::null_mut(); MAX_CPUS_SCHED];

// Global ready queue protected by SpinLock
static READY_LOCK: crate::spinlock::SpinLock<ReadyQueue> = crate::spinlock::SpinLock::new(ReadyQueue {
    head: ptr::null_mut(),
    tail: ptr::null_mut(),
});

struct ReadyQueue {
    head: *mut Task,
    tail: *mut Task,
}
unsafe impl Send for ReadyQueue {}

// Sleep queue also needs locking
static SLEEP_LOCK: crate::spinlock::SpinLock<SleepQueue> = crate::spinlock::SpinLock::new(SleepQueue {
    head: ptr::null_mut(),
    count: 0,
});

struct SleepQueue {
    head: *mut Task,
    count: usize,
}
unsafe impl Send for SleepQueue {}

static NEXT_ID: AtomicU32 = AtomicU32::new(0);
static mut FREE_IDS: [u32; 128] = [0u32; 128];
static mut FREE_ID_COUNT: usize = 0;
static mut SCHED_HAS_FPU: bool = false;

unsafe fn recycle_task_id(id: u32) {
    if id == 0 || id >= config::MAX_TASKS { return; }
    if FREE_ID_COUNT < 128 {
        FREE_IDS[FREE_ID_COUNT] = id;
        FREE_ID_COUNT += 1;
    }
}

unsafe fn alloc_task_id() -> Option<u32> {
    if FREE_ID_COUNT > 0 {
        FREE_ID_COUNT -= 1;
        let id = FREE_IDS[FREE_ID_COUNT];
        if id < config::MAX_TASKS && (*TASKS.as_ptr().add(id as usize)).state == TaskState::EXITED {
            return Some(id);
        }
    }
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    if id >= config::MAX_TASKS {
        NEXT_ID.fetch_sub(1, Ordering::Relaxed);
        return None;
    }
    Some(id)
}

/// Get current CPU index from LAPIC ID
unsafe fn current_cpu_id() -> usize {
    let apic_id = crate::smp::krust_smp_current_cpu_id() as usize;
    if apic_id < MAX_CPUS_SCHED { apic_id } else { 0 }
}

/// Get/set current task for this CPU
unsafe fn get_current() -> *mut Task {
    PER_CPU_CURRENT[current_cpu_id()]
}

unsafe fn set_current(t: *mut Task) {
    PER_CPU_CURRENT[current_cpu_id()] = t;
}
// ─── Helpers ─────────────────────────────────────────────────────

unsafe fn frame_to_ptr(frame: usize) -> *mut u64 {
    (frame * PAGE_SIZE as usize) as *mut u64
}

unsafe fn alloc_frame() -> *mut u64 {
    let f = krust_pmm_alloc_frame();
    if f == !0 { ptr::null_mut() } else { frame_to_ptr(f) }
}

unsafe fn free_frame(p: *mut u64) {
    if !p.is_null() {
        krust_pmm_free_frame(p as usize / PAGE_SIZE as usize);
    }
}

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

// ─── Enqueue / Dequeue ───────────────────────────────────────────

unsafe fn enqueue(t: *mut Task) {
    let mut q = READY_LOCK.lock();
    (*t).next = ptr::null_mut();
    if q.head.is_null() {
        q.head = t;
        q.tail = t;
    } else {
        (*q.tail).next = t;
        q.tail = t;
    }
}

unsafe fn dequeue() -> *mut Task {
    let mut q = READY_LOCK.lock();
    if q.head.is_null() {
        return ptr::null_mut();
    }
    let t = q.head;
    q.head = (*t).next;
    if q.head.is_null() {
        q.tail = ptr::null_mut();
    }
    (*t).next = ptr::null_mut();
    t
}

unsafe fn sleep_enqueue(t: *mut Task, ticks: u32) {
    let mut sq = SLEEP_LOCK.lock();
    (*t).sleep_until = ticks;
    (*t).state = TaskState::BLOCKED;
    (*t).next = sq.head;
    sq.head = t;
    sq.count += 1;
}

unsafe fn sleep_wake_all(current_ticks: u32) {
    let mut sq = SLEEP_LOCK.lock();
    let mut prev: *mut Task = ptr::null_mut();
    let mut curr = sq.head;
    while !curr.is_null() {
        let next = (*curr).next;
        if (*curr).sleep_until <= current_ticks {
            (*curr).state = TaskState::READY;
            (*curr).sleep_until = 0;
            (*curr).next = ptr::null_mut();
            // enqueue directly under the same lock context
            {
                let mut q = READY_LOCK.lock();
                if q.head.is_null() {
                    q.head = curr;
                    q.tail = curr;
                } else {
                    (*q.tail).next = curr;
                    q.tail = curr;
                }
            }
            if !prev.is_null() {
                (*prev).next = next;
            } else {
                sq.head = next;
            }
            sq.count -= 1;
        } else {
            prev = curr;
        }
        curr = next;
    }
}

unsafe fn add_child(parent: *mut Task, child: *mut Task) {
    (*child).parent = parent;
    (*child).sibling_next = ptr::null_mut();
    if (*parent).child_head.is_null() {
        (*parent).child_head = child;
        (*parent).child_tail = child;
    } else {
        (*(*parent).child_tail).sibling_next = child;
        (*parent).child_tail = child;
    }
}

unsafe fn remove_child(parent: *mut Task, child: *mut Task) {
    let mut prev: *mut Task = ptr::null_mut();
    let mut curr = (*parent).child_head;
    while !curr.is_null() {
        if curr == child {
            if !prev.is_null() {
                (*prev).sibling_next = (*child).sibling_next;
            } else {
                (*parent).child_head = (*child).sibling_next;
            }
            if (*child).sibling_next.is_null() {
                (*parent).child_tail = prev;
            }
            (*child).parent = ptr::null_mut();
            (*child).sibling_next = ptr::null_mut();
            return;
        }
        prev = curr;
        curr = (*curr).sibling_next;
    }
}

unsafe fn fpu_save_current() {
    if SCHED_HAS_FPU && !get_current().is_null() && !(*get_current()).fpu_state.is_null() {
        krust_fpu_save((*get_current()).fpu_state);
    }
}

unsafe fn fpu_restore_current() {
    if SCHED_HAS_FPU && !get_current().is_null() && !(*get_current()).fpu_state.is_null() {
        krust_fpu_restore((*get_current()).fpu_state);
    }
}

unsafe fn setup_task_common(t: *mut Task, id: u32, kstack: *mut u64, parent: *mut Task) {
    (*t).id = id;
    (*t).ppid = if parent.is_null() { 0 } else { (*parent).id };
    (*t).pgid = if parent.is_null() { id } else { (*parent).pgid };
    (*t).sid = if parent.is_null() { id } else { (*parent).sid };
    (*t).state = TaskState::READY;
    (*t).kstack = kstack;
    (*t).pages = 1;
    (*t).stdin_fd = -1;
    (*t).stdout_fd = -1;
    (*t).stderr_fd = -1;
    (*t).fd_table = krust_malloc((core::mem::size_of::<FDEntry>() * MAX_FDS) as u32) as *mut FDEntry;
    if !(*t).fd_table.is_null() {
        memset((*t).fd_table as *mut u8, 0, core::mem::size_of::<FDEntry>() * MAX_FDS);
    }
    (*t).exit_code = 0;
    (*t).cwd[0] = b'/';
    (*t).cwd[1] = 0;
    (*t).parent = parent;
    (*t).child_head = ptr::null_mut();
    (*t).child_tail = ptr::null_mut();
    (*t).sibling_next = ptr::null_mut();
    (*t).has_child_exit = false;
    (*t).sig_pending = 0;
    (*t).sig_blocked = 0;
    for i in 0..NSIG {
        (*t).sig_handlers[i].handler_addr = 0;
    }
    (*t).vma_list = ptr::null_mut();
    (*t).program_brk = BRK_INITIAL;
    (*t).priority = 0;
    (*t).sleep_until = 0;
    if parent.is_null() {
        (*t).uid = 0;
        (*t).gid = 0;
    } else {
        (*t).uid = (*parent).uid;
        (*t).gid = (*parent).gid;
    }
    (*t).oom_score_adj = 0;
    if SCHED_HAS_FPU {
        let f = krust_pmm_alloc_frame();
        if f != !0 {
            (*t).fpu_state = frame_to_ptr(f) as *mut u8;
            memset((*t).fpu_state, 0, PAGE_SIZE as usize);
        }
    }
}

unsafe fn copy_fd_table(src: *mut FDEntry) -> *mut FDEntry {
    if src.is_null() { return ptr::null_mut(); }
    let dst = krust_malloc((core::mem::size_of::<FDEntry>() * MAX_FDS) as u32) as *mut FDEntry;
    if dst.is_null() { return ptr::null_mut(); }
    for i in 0..MAX_FDS {
        (*dst.add(i)) = (*src.add(i));
        if (*dst.add(i)).used && !(*dst.add(i)).node.is_null() {
            (*dst.add(i)).refcount += 1;
        }
    }
    dst
}

unsafe fn free_fd_table(table: *mut FDEntry) {
    if table.is_null() { return; }
    for i in 0..MAX_FDS {
        if (*table.add(i)).used && !(*table.add(i)).node.is_null() {
            if (*table.add(i)).refcount <= 1 {
                (*table.add(i)).used = false;
                (*table.add(i)).node = ptr::null_mut();
            }
        }
    }
    krust_free(table as *mut u8);
}

/// Get the current process's FD table, or null
#[no_mangle]
pub unsafe extern "C" fn krust_sched_current_fd_table() -> *mut FDEntry {
    let cur = get_current();
    if cur.is_null() { ptr::null_mut() } else { (*cur).fd_table }
}

unsafe fn allocate_user_stack(kstack: *mut u64, ustack_out: *mut *mut u64, user_rsp_out: *mut u64) -> i32 {
    let ustack_base = USTACK_VADDR - (USTACK_PAGES as u64 - 1) * PAGE_SIZE;
    let mut allocated: u32 = 0;
    for i in 0..USTACK_PAGES {
        if i == 0 {
            continue; // Guard page: leave unmapped
        }
        let frame = krust_pmm_alloc_frame();
        if frame == !0 {
            for j in 1..i {
                let fphys = krust_paging_get_phys(ustack_base + j as u64 * PAGE_SIZE);
                if fphys != !0 {
                    krust_pmm_free_frame(fphys as usize / PAGE_SIZE as usize);
                }
                krust_paging_unmap_page(ustack_base + j as u64 * PAGE_SIZE);
            }
            free_frame(kstack);
            return -1;
        }
        memset(frame_to_ptr(frame) as *mut u8, 0, PAGE_SIZE as usize);
        krust_paging_map_page(ustack_base + i as u64 * PAGE_SIZE,
                             (frame as u64) * PAGE_SIZE,
                             0x1 | 0x2 | 0x4); // PRESENT | WRITE | USER
        allocated += 1;
    }
    *ustack_out = ustack_base as *mut u64;
    *user_rsp_out = USTACK_VADDR + PAGE_SIZE;
    0
}

unsafe fn allocate_user_stack_in(pml4_paddr: u64, ustack_out: *mut *mut u64, user_rsp_out: *mut u64) -> i32 {
    let ustack_base = USTACK_VADDR - (USTACK_PAGES as u64 - 1) * PAGE_SIZE;
    let mut frames = [0u32; 16];
    for i in 0..USTACK_PAGES {
        if i == 0 {
            continue; // Guard page: leave unmapped
        }
        let frame = krust_pmm_alloc_frame();
        if frame == !0 {
            for j in 1..i as usize {
                krust_pmm_free_frame(frames[j] as usize);
            }
            return -1;
        }
        frames[i as usize] = frame as u32;
        memset(frame_to_ptr(frame) as *mut u8, 0, PAGE_SIZE as usize);
        crate::paging::krust_paging_map_page_in(pml4_paddr,
                             ustack_base + i as u64 * PAGE_SIZE,
                             (frame as u64) * PAGE_SIZE,
                             0x1 | 0x2 | 0x4); // PRESENT | WRITE | USER
    }
    *ustack_out = ustack_base as *mut u64;
    *user_rsp_out = USTACK_VADDR + PAGE_SIZE;
    0
}

unsafe fn read_cr3() -> u64 {
    let val: u64;
    core::arch::asm!("mov {}, cr3", out(reg) val);
    val
}

unsafe fn write_cr3(val: u64) {
    core::arch::asm!("mov cr3, {}", in(reg) val);
}

unsafe fn build_user_frame(sp_top: *mut u64, entry: u64, user_rsp: u64, argc: i32, argv_ptr: u64) -> *mut u64 {
    let mut sp = sp_top;
    sp = sp.sub(1); *sp = 0x23;                // ss
    sp = sp.sub(1); *sp = user_rsp;            // user rsp
    sp = sp.sub(1); *sp = 0x202;               // rflags
    sp = sp.sub(1); *sp = 0x1B;                // cs
    sp = sp.sub(1); *sp = entry;               // rip
    sp = sp.sub(1); *sp = 0;                   // err_code
    sp = sp.sub(1); *sp = 0;                   // int_no
    sp = sp.sub(1); *sp = argc as u64;         // rax
    sp = sp.sub(1); *sp = 0;                   // rcx
    sp = sp.sub(1); *sp = 0;                   // rdx
    sp = sp.sub(1); *sp = argv_ptr;            // rbx
    sp = sp.sub(1); *sp = 0;                   // rsp (dummy)
    sp = sp.sub(1); *sp = 0;                   // rbp
    sp = sp.sub(1); *sp = 0;                   // rsi
    sp = sp.sub(1); *sp = 0;                   // rdi
    sp = sp.sub(1); *sp = 0;                   // r8
    sp = sp.sub(1); *sp = 0;                   // r9
    sp = sp.sub(1); *sp = 0;                   // r10
    sp = sp.sub(1); *sp = 0;                   // r11
    sp = sp.sub(1); *sp = 0;                   // r12
    sp = sp.sub(1); *sp = 0;                   // r13
    sp = sp.sub(1); *sp = 0;                   // r14
    sp = sp.sub(1); *sp = 0;                   // r15
    sp
}

// ─── Extern "C" API ─────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn krust_sched_init() {
    let idle = &mut TASKS[0];
    idle.id = 0;
    idle.state = TaskState::RUNNING;
    idle.kstack = ptr::null_mut();
    idle.ctx.rsp = 0;
    set_current(idle);
    krust_sched_sync_cpp();
    NEXT_ID.store(1, Ordering::Relaxed);
    SCHED_HAS_FPU = krust_has_fxsr() != 0;
}

/// Create an idle task for an AP and set it as current for that CPU.
/// Idle tasks have kstack = null, which the scheduler uses to identify them.
#[no_mangle]
pub unsafe extern "C" fn krust_sched_create_idle_for_cpu(cpu_id: u32) {
    let id = match alloc_task_id() { Some(id) => id, None => return };
    let t = &mut TASKS[id as usize];
    (*t).id = id;
    (*t).ppid = 0;
    (*t).state = TaskState::RUNNING;
    (*t).kstack = ptr::null_mut(); // marks as idle
    (*t).ctx.rsp = 0;
    (*t).ctx.cr3 = 0;
    (*t).stdin_fd = -1;
    (*t).stdout_fd = -1;
    (*t).stderr_fd = -1;
    (*t).exit_code = 0;
    (*t).cwd[0] = b'/';
    (*t).cwd[1] = 0;
    (*t).parent = ptr::null_mut();
    (*t).child_head = ptr::null_mut();
    (*t).child_tail = ptr::null_mut();
    (*t).sibling_next = ptr::null_mut();
    (*t).has_child_exit = false;
    (*t).sig_pending = 0;
    (*t).sig_blocked = 0;
    (*t).vma_list = ptr::null_mut();
    (*t).program_brk = BRK_INITIAL;
    (*t).priority = 0;
    (*t).sleep_until = 0;
    (*t).uid = 0;
    (*t).gid = 0;
    (*t).oom_score_adj = 0;
    (*t).fpu_state = ptr::null_mut();
    for i in 0..NSIG {
        (*t).sig_handlers[i].handler_addr = 0;
    }

    // Set as current for this AP's CPU index
    if (cpu_id as usize) < MAX_CPUS_SCHED {
        PER_CPU_CURRENT[cpu_id as usize] = t;
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_enqueue(t: *mut Task) {
    enqueue(t);
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_dequeue() -> *mut Task {
    dequeue()
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_yield() {
    yield_task();
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_current_tid() -> i32 {
    if get_current().is_null() { -1 } else { (*get_current()).id as i32 }
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_get_pid() -> i32 {
    if get_current().is_null() { -1 } else { (*get_current()).id as i32 }
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_get_ppid() -> i32 {
    if get_current().is_null() { -1 } else { (*get_current()).ppid as i32 }
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_current() -> *mut Task {
    get_current()
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_current_vma_list() -> *mut VMA {
    if get_current().is_null() { ptr::null_mut() } else { (*get_current()).vma_list }
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_current_set_vma_list(vma: *mut VMA) {
    if !get_current().is_null() {
        (*get_current()).vma_list = vma;
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_current_program_brk() -> u64 {
    if get_current().is_null() { 0 } else { (*get_current()).program_brk }
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_current_set_program_brk(brk: u64) {
    if !get_current().is_null() {
        (*get_current()).program_brk = brk;
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_sleep_ticks(ms: u32) {
    if get_current().is_null() || (*get_current()).id == 0 { return; }
    let tick_ms = crate::pittimer::TICK_MS.load(core::sync::atomic::Ordering::Relaxed);
    let ticks = if tick_ms > 0 { ms / tick_ms } else { ms };
    let target = crate::pittimer::krust_pittimer_get_ticks() + ticks;
    sleep_enqueue(get_current(), target);
    yield_task();
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_on_tick() {
    let ticks = crate::pittimer::krust_pittimer_get_ticks();
    sleep_wake_all(ticks);
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_current_stdout_fd() -> i32 {
    if get_current().is_null() { -1 } else { (*get_current()).stdout_fd }
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_current_stdin_fd() -> i32 {
    if get_current().is_null() { -1 } else { (*get_current()).stdin_fd }
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_current_cwd() -> *mut u8 {
    if get_current().is_null() { ptr::null_mut() } else { (*get_current()).cwd.as_mut_ptr() }
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_get_task(id: u32) -> *mut Task {
    if id < config::MAX_TASKS { &mut TASKS[id as usize] } else { ptr::null_mut() }
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_get_next_id() -> u32 {
    NEXT_ID.load(Ordering::Relaxed)
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_max_tasks() -> u32 {
    config::MAX_TASKS
}

// ─── NodeType constants (mirrored from C++) ───────────────────

const NODE_FILE: u8 = 0;
const NODE_DIR: u8 = 1;
const NODE_DEVICE: u8 = 2;

// ─── Create functions ─────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn krust_sched_create_elf(entry: u64, argc: i32, argv: *const *const u8,
                                                 stdin_fd: i32, stdout_fd: i32) -> i32 {
    let id = match alloc_task_id() { Some(id) => id, None => return -1 };

    let kstack = alloc_frame();
    if kstack.is_null() { return -1; }
    memset(kstack as *mut u8, 0, PAGE_SIZE as usize);

    let t = &mut TASKS[id as usize];
    setup_task_common(t, id, kstack, ptr::null_mut());
    (*t).stdin_fd = stdin_fd;
    (*t).stdout_fd = stdout_fd;

    let sp_top = (kstack as u64 + PAGE_SIZE) as *mut u64;

    // Pre-copy argv data into kernel buffer while still in the caller's page table
    let mut str_size: u64 = 0;
    let mut argv_buf = [ptr::null::<u8>(); 16];
    let mut argv_lens = [0u64; 16];
    let mut real_argc = argc;
    if argc > 0 && !argv.is_null() && (argc as usize) < 16 {
        core::arch::asm!("stac");
        for i in 0..argc as usize {
            let s = *argv.add(i);
            if s.is_null() {
                core::arch::asm!("clac");
                real_argc = i as i32;
                break;
            }
            let mut len: u64 = 0;
            while ptr::read_volatile(s.add(len as usize)) != 0 { len += 1; }
            argv_buf[i] = s;
            argv_lens[i] = len;
            str_size += len + 1;
        }
        core::arch::asm!("clac");
    } else {
        real_argc = 0;
    }

    let task_pml4 = krust_paging_clone_kernel_dir();
    if task_pml4.is_null() {
        free_frame(kstack);
        return -1;
    }

    // Map user stack pages directly in the task's page table (no CR3 switch needed)
    let mut ustack_base: *mut u64 = ptr::null_mut();
    let mut user_rsp: u64 = 0;
    if allocate_user_stack_in(task_pml4 as u64, &mut ustack_base, &mut user_rsp) != 0 {
        free_frame(kstack);
        return -1;
    }
    (*t).ustack = ustack_base;

    // Switch to task page table to write argv data to the user stack
    let old_cr3 = read_cr3();
    write_cr3(task_pml4 as u64);

    let mut argv_ptr: u64 = 0;
    if real_argc > 0 {
        let argv_size = (real_argc as u64 + 1) * core::mem::size_of::<u64>() as u64;
        let total = str_size + argv_size;
        if total <= USTACK_SIZE as u64 - 16 {
            let str_area = ustack_base as *mut u8;
            let argv_arr = (ustack_base as u64 + str_size) as *mut u64;
            let mut str_off: u64 = 0;
            for i in 0..real_argc as usize {
                let s = argv_buf[i];
                let len = argv_lens[i];
                *argv_arr.add(i) = str_area as u64 + str_off;
                memcpy(str_area.add(str_off as usize), s, len as usize);
                *str_area.add(str_off as usize + len as usize) = 0;
                str_off += len + 1;
            }
            *argv_arr.add(real_argc as usize) = 0;
            argv_ptr = argv_arr as u64;
        }
    }

    write_cr3(old_cr3);

    let sp = build_user_frame(sp_top, entry, user_rsp, real_argc, argv_ptr);

    memset(&mut (*t).ctx as *mut _ as *mut u8, 0, core::mem::size_of::<TaskContext>());
    (*t).ctx.rsp = sp as u64;
    (*t).ctx.cr3 = task_pml4 as u64;

    let kstack_top = sp_top as u64;
    krust_tss_set_kernel_stack(kstack_top);
    crate::smp::smp_set_kernel_stack(kstack_top);
    enqueue(t);
    id as i32
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_create_init(elf_data: *const u8, elf_size: u32) -> i32 {
    let id = match alloc_task_id() { Some(id) => id, None => return -1 };

    let kstack = alloc_frame();
    if kstack.is_null() { return -1; }
    memset(kstack as *mut u8, 0, PAGE_SIZE as usize);

    let t = &mut TASKS[id as usize];
    setup_task_common(t, id, kstack, ptr::null_mut());
    (*t).stdin_fd = -1;
    (*t).stdout_fd = -1;
    (*t).stderr_fd = -1;

    let sp_top = (kstack as u64 + PAGE_SIZE) as *mut u64;

    let task_pml4 = krust_paging_clone_kernel_dir();
    if task_pml4.is_null() {
        free_frame(kstack);
        return -1;
    }

    // Switch to new page table to load ELF
    let old_cr3 = read_cr3();
    write_cr3(task_pml4 as u64);

    let mut entry: u64 = 0;
    let load_ok = krust_elf_load(elf_data, elf_size, &mut entry);
    if load_ok != 0 {
        write_cr3(old_cr3);
        free_frame(kstack);
        free_frame(task_pml4 as *mut u64);
        return -1;
    }

    let mut ustack_base: *mut u64 = ptr::null_mut();
    let mut user_rsp: u64 = 0;
    if allocate_user_stack(kstack, &mut ustack_base, &mut user_rsp) != 0 {
        write_cr3(old_cr3);
        free_frame(kstack);
        free_frame(task_pml4 as *mut u64);
        return -1;
    }
    (*t).ustack = ustack_base;

    write_cr3(old_cr3);

    let sp = build_user_frame(sp_top, entry, user_rsp, 0, 0);

    memset(&mut (*t).ctx as *mut _ as *mut u8, 0, core::mem::size_of::<TaskContext>());
    (*t).ctx.rsp = sp as u64;
    (*t).ctx.cr3 = task_pml4 as u64;

    let kstack_top = sp_top as u64;
    krust_tss_set_kernel_stack(kstack_top);
    crate::smp::smp_set_kernel_stack(kstack_top);
    enqueue(t);
    id as i32
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_create(entry: extern "C" fn(), stdin_fd: i32, stdout_fd: i32) -> i32 {
    let id = match alloc_task_id() { Some(id) => id, None => return -1 };

    let kstack = alloc_frame();
    if kstack.is_null() {
        return -1;
    }
    memset(kstack as *mut u8, 0, PAGE_SIZE as usize);

    let t = &mut TASKS[id as usize];
    setup_task_common(t, id, kstack, ptr::null_mut());
    (*t).stdin_fd = stdin_fd;
    (*t).stdout_fd = stdout_fd;

    let sp_top = (kstack as u64 + PAGE_SIZE) as *mut u64;

    let mut sp = sp_top;
    // kernel-mode frame: ss, rsp(dummy), rflags, cs, rip, err_code, int_no, rax...r15
    sp = sp.sub(1); *sp = 0x10;                // ss
    sp = sp.sub(1); *sp = 0;                   // rsp (dummy)
    sp = sp.sub(1); *sp = 0x202;               // rflags
    sp = sp.sub(1); *sp = 0x08;                // cs
    sp = sp.sub(1); *sp = entry as u64;        // rip
    sp = sp.sub(1); *sp = 0;                   // err_code
    sp = sp.sub(1); *sp = 0;                   // int_no
    sp = sp.sub(1); *sp = 0;                   // rax
    sp = sp.sub(1); *sp = 0;                   // rcx
    sp = sp.sub(1); *sp = 0;                   // rdx
    sp = sp.sub(1); *sp = 0;                   // rbx
    sp = sp.sub(1); *sp = 0;                   // rsp (dummy)
    sp = sp.sub(1); *sp = 0;                   // rbp
    sp = sp.sub(1); *sp = 0;                   // rsi
    sp = sp.sub(1); *sp = 0;                   // rdi
    sp = sp.sub(1); *sp = 0;                   // r8
    sp = sp.sub(1); *sp = 0;                   // r9
    sp = sp.sub(1); *sp = 0;                   // r10
    sp = sp.sub(1); *sp = 0;                   // r11
    sp = sp.sub(1); *sp = 0;                   // r12
    sp = sp.sub(1); *sp = 0;                   // r13
    sp = sp.sub(1); *sp = 0;                   // r14
    sp = sp.sub(1); *sp = 0;                   // r15

    memset(&mut (*t).ctx as *mut _ as *mut u8, 0, core::mem::size_of::<TaskContext>());
    (*t).ctx.rsp = sp as u64;
    (*t).ctx.cr3 = krust_paging_page_directory() as u64;

    enqueue(t);
    id as i32
}

// ─── FORK ─────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn krust_sched_fork(r: *mut Registers) -> i32 {
    if r.is_null() { return -1; }
    let parent = get_current();
    if parent.is_null() || parent.is_null() { return -1; }
    let child_id = match alloc_task_id() { Some(id) => id, None => return -1 };

    let child_kstack = alloc_frame();
    if child_kstack.is_null() {
        return -1;
    }
    memset(child_kstack as *mut u8, 0, PAGE_SIZE as usize);

    let child = &mut TASKS[child_id as usize];
    setup_task_common(child, child_id, child_kstack, parent);

    // Copy kernel stack contents
    let parent_kstack_start = (*parent).kstack as u64;
    let parent_r_offset = r as u64 - parent_kstack_start;
    let child_r_addr = child_kstack as u64 + parent_r_offset;
    let child_r = child_r_addr as *mut Registers;
    memcpy(child_kstack as *mut u8, (*parent).kstack as *const u8, PAGE_SIZE as usize);

    (*child_r).rax = 0;  // child returns 0

    // Clone page tables
    let child_pml4 = krust_paging_clone_kernel_dir();
    if child_pml4.is_null() {
        free_frame(child_kstack);
        return -1;
    }

    // Copy user pages
    krust_paging_copy_user_pages((*parent).ctx.cr3 as *mut u64, child_pml4);

    // Copy task properties
    (*child).stdin_fd = (*parent).stdin_fd;
    (*child).stdout_fd = (*parent).stdout_fd;
    (*child).stderr_fd = (*parent).stderr_fd;
    (*child).fd_table = copy_fd_table((*parent).fd_table);
    (*child).ustack = (*parent).ustack;
    (*child).pages = (*parent).pages;
    // Copy cwd
    let mut ci = 0;
    while ci < 127 && (*parent).cwd[ci] != 0 {
        (*child).cwd[ci] = (*parent).cwd[ci];
        ci += 1;
    }
    (*child).cwd[ci] = 0;
    (*child).ppid = (*parent).id;

    // Copy VMA list
    (*child).vma_list = ptr::null_mut();
    (*child).program_brk = (*parent).program_brk;
    let mut v = (*parent).vma_list;
    while !v.is_null() {
        krust_vmm_add(&mut (*child).vma_list, (*v).start, (*v).end, (*v).flags);
        v = (*v).next;
    }

    // Copy signal handlers
    for i in 0..NSIG {
        (*child).sig_handlers[i].handler_addr = (*parent).sig_handlers[i].handler_addr;
    }
    (*child).sig_blocked = (*parent).sig_blocked;

    // Set up context for scheduler
    memset(&mut (*child).ctx as *mut _ as *mut u8, 0, core::mem::size_of::<TaskContext>());
    (*child).ctx.rsp = child_r_addr;
    (*child).ctx.cr3 = child_pml4 as u64;

    add_child(parent, child);

    let kstack_top = (*parent).kstack as u64 + PAGE_SIZE;
    krust_tss_set_kernel_stack(kstack_top);
    crate::smp::smp_set_kernel_stack(kstack_top);
    enqueue(child);

    child_id as i32
}

// ─── EXECVE ───────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn krust_sched_execve(r: *mut Registers, path: *const u8,
                                             argc: i32, argv: *const *const u8) -> i32 {
    let mut node = krust_vfs_resolve(path);
    if node.is_null() || (*node).type_ != NODE_FILE {
        // Try /bin/<name>.elf
        core::arch::asm!("stac");
        let p_len = {
            let mut i = 0;
            while ptr::read_volatile(path.add(i)) != 0 { i += 1; }
            i
        };
        core::arch::asm!("clac");
        if p_len < 55 {
            let mut elf_path = [0u8; 64];
            let prefix = b"/bin/";
            let suffix = b".elf";
            let mut ei = 0;
            for c in prefix { elf_path[ei] = *c; ei += 1; }
            let mut pi = 0;
            core::arch::asm!("stac");
            while pi < p_len { elf_path[ei] = ptr::read_volatile(path.add(pi)); ei += 1; pi += 1; }
            core::arch::asm!("clac");
            for c in suffix { elf_path[ei] = *c; ei += 1; }
            elf_path[ei] = 0;
            node = krust_vfs_resolve(elf_path.as_ptr());
        }
        if node.is_null() || (*node).type_ != NODE_FILE { return -1; }
    }

    let mut entry: u64 = 0;
    if krust_elf_load((*node).data, (*node).size, &mut entry) != 0 { return -1; }

    // Pre-copy argv from old user memory BEFORE freeing pages
    let mut argv_buf = [ptr::null::<u8>(); 16];
    let mut argv_lens = [0u64; 16];
    let mut real_argc = argc;
    let mut str_size: u64 = 0;
    if argc > 0 && !argv.is_null() && (argc as usize) < 16 {
        core::arch::asm!("stac");
        for i in 0..argc as usize {
            let s = *argv.add(i);
            if s.is_null() {
                core::arch::asm!("clac");
                real_argc = i as i32;
                break;
            }
            let mut len: u64 = 0;
            while ptr::read_volatile(s.add(len as usize)) != 0 { len += 1; }
            argv_buf[i] = s;
            argv_lens[i] = len;
            str_size += len + 1;
        }
        core::arch::asm!("clac");
    } else {
        real_argc = 0;
    }

    // Free old user pages
    krust_paging_free_user_pages((*get_current()).ctx.cr3);
    krust_vmm_free_all(&mut (*get_current()).vma_list);
    (*get_current()).program_brk = BRK_INITIAL;

    // Close non-standard FDs (>= 3) on exec
    {
        let fd_table = (*get_current()).fd_table;
        if !fd_table.is_null() {
            for i in 3..MAX_FDS {
                if (*fd_table.add(i)).used {
                    crate::vfs::krust_vfs_close(i as i32);
                }
            }
        }
    }

    let new_pml4 = krust_paging_clone_kernel_dir();
    if new_pml4.is_null() { return -1; }

    let old_cr3 = read_cr3();
    write_cr3(new_pml4 as u64);

    let mut entry2: u64 = 0;
    let load_ok = krust_elf_load((*node).data, (*node).size, &mut entry2);
    if load_ok != 0 {
        write_cr3(old_cr3);
        crate::paging::krust_paging_free_page_tables(new_pml4 as u64);
        return -1;
    }

    let ustack_base = USTACK_VADDR - (USTACK_PAGES as u64 - 1) * PAGE_SIZE;
    let user_rsp = USTACK_VADDR + PAGE_SIZE;
    for i in 0..USTACK_PAGES {
        if i == 0 {
            continue; // Guard page: leave unmapped
        }
        let frame = krust_pmm_alloc_frame();
        if frame == !0 {
            for j in 1..i as usize {
                let fphys = krust_paging_get_phys(ustack_base + j as u64 * PAGE_SIZE);
                if fphys != !0 {
                    krust_pmm_free_frame(fphys as usize / PAGE_SIZE as usize);
                }
                krust_paging_unmap_page(ustack_base + j as u64 * PAGE_SIZE);
            }
            write_cr3(old_cr3);
            crate::paging::krust_paging_free_page_tables(new_pml4 as u64);
            return -1;
        }
        memset(frame_to_ptr(frame) as *mut u8, 0, PAGE_SIZE as usize);
        krust_paging_map_page(ustack_base + i as u64 * PAGE_SIZE,
                             (frame as u64) * PAGE_SIZE,
                             0x1 | 0x2 | 0x4);
    }

    let mut argv_ptr: u64 = 0;
    if real_argc > 0 {
        let argv_size = (real_argc as u64 + 1) * core::mem::size_of::<u64>() as u64;
        let total = str_size + argv_size;
        if total <= USTACK_SIZE as u64 - 16 {
            let str_area = ustack_base as *mut u8;
            let argv_arr = (ustack_base + str_size) as *mut u64;
            let mut str_off: u64 = 0;
            for i in 0..real_argc as usize {
                let s = argv_buf[i];
                let len = argv_lens[i];
                *argv_arr.add(i) = str_area as u64 + str_off;
                memcpy(str_area.add(str_off as usize), s, len as usize);
                *str_area.add(str_off as usize + len as usize) = 0;
                str_off += len + 1;
            }
            *argv_arr.add(real_argc as usize) = 0;
            argv_ptr = argv_arr as u64;
        }
    }

    (*get_current()).ctx.cr3 = new_pml4 as u64;
    write_cr3(new_pml4 as u64);

    let sp_top = ((*get_current()).kstack as u64 + PAGE_SIZE) as *mut u64;
    let mut sp = sp_top;
    sp = sp.sub(1); *sp = 0x23;
    sp = sp.sub(1); *sp = user_rsp;
    sp = sp.sub(1); *sp = 0x202;
    sp = sp.sub(1); *sp = 0x1B;
    sp = sp.sub(1); *sp = entry2;
    sp = sp.sub(1); *sp = 0;
    sp = sp.sub(1); *sp = 0;
    sp = sp.sub(1); *sp = real_argc as u64;
    sp = sp.sub(1); *sp = 0;
    sp = sp.sub(1); *sp = 0;
    sp = sp.sub(1); *sp = argv_ptr;
    sp = sp.sub(1); *sp = 0;
    sp = sp.sub(1); *sp = 0;
    sp = sp.sub(1); *sp = 0;
    sp = sp.sub(1); *sp = 0;
    sp = sp.sub(1); *sp = 0;
    sp = sp.sub(1); *sp = 0;
    sp = sp.sub(1); *sp = 0;
    sp = sp.sub(1); *sp = 0;
    sp = sp.sub(1); *sp = 0;
    sp = sp.sub(1); *sp = 0;
    sp = sp.sub(1); *sp = 0;

    (*get_current()).ctx.rsp = sp as u64;
    (*get_current()).ustack = ustack_base as *mut u64;
    (*get_current()).pages = USTACK_PAGES;

    for i in 0..NSIG {
        if (*get_current()).sig_handlers[i].handler_addr != 1 {
            (*get_current()).sig_handlers[i].handler_addr = 0;
        }
    }
    (*get_current()).sig_pending = 0;

    (*r).rip = entry2;
    (*r).user_rsp = user_rsp;
    (*r).rax = 0;
    (*r).cs = 0x1B;
    (*r).ss = 0x23;
    (*r).rflags = 0x202;

    0
}

// ─── WAITPID ──────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn krust_sched_waitpid(pid: i32, status: *mut i32) -> i32 {
    krust_sched_waitpid_flags(pid, status, 0)
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_waitpid_flags(pid: i32, status: *mut i32, flags: u32) -> i32 {
    if get_current().is_null() { return -1; }

    let wnohang = (flags & 1) != 0;

    let child: *mut Task;
    if pid > 0 {
        if pid as u32 >= NEXT_ID.load(Ordering::Relaxed) { return -1; }
        child = &mut TASKS[pid as usize];
        if (*child).parent != get_current() { return -1; }
    } else if pid == -1 {
        let mut found: *mut Task = (*get_current()).child_head;
        if found.is_null() {
            if wnohang { return 0; }
            return -1;
        }
        while !found.is_null() && (*found).state != TaskState::EXITED {
            found = (*found).sibling_next;
        }
        if found.is_null() { found = (*get_current()).child_head; }
        child = found;
    } else {
        return -1;
    }

    // Block until child exits
    while (*child).state != TaskState::EXITED {
        if wnohang { return 0; }
        (*get_current()).state = TaskState::WAITING;
        yield_task();
        (*get_current()).state = TaskState::RUNNING;
    }

    if !status.is_null() {
        *status = (*child).exit_code as i32;
    }

    remove_child(get_current(), child);

    if !(*child).vma_list.is_null() {
        krust_vmm_free_all(&mut (*child).vma_list);
    }
    free_fd_table((*child).fd_table);
    (*child).fd_table = ptr::null_mut();
    if (*child).ctx.cr3 != 0 {
        krust_paging_free_user_pages((*child).ctx.cr3);
    }
    if !(*child).kstack.is_null() {
        free_frame((*child).kstack);
        (*child).kstack = ptr::null_mut();
    }
    if !(*child).fpu_state.is_null() {
        free_frame((*child).fpu_state as *mut u64);
        (*child).fpu_state = ptr::null_mut();
    }

    recycle_task_id((*child).id);

    (*child).exit_code as i32
}

// ─── SIGNALS ──────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn krust_sched_kill(pid: i32, sig: i32) -> i32 {
    if sig < 0 || sig as usize >= NSIG { return -1; }
    if pid < 0 || pid as u32 >= NEXT_ID.load(Ordering::Relaxed) { return -1; }

    let current = get_current();
    let t = &mut TASKS[pid as usize];
    if (*t).state == TaskState::EXITED { return -1; }

    if !current.is_null() && (*current).uid != 0 && (*current).uid != (*t).uid {
        return -1;
    }

    if sig == SIGKILL {
        (*t).exit_code = 0x100 + SIGKILL as u32;
        (*t).state = TaskState::EXITED;
        (*t).has_child_exit = true;
        if !(*t).parent.is_null() {
            (*(*t).parent).has_child_exit = true;
        }
        return 0;
    }

    (*t).sig_pending |= 1 << sig;
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_sigaction(sig: i32, handler: u64, old_handler: *mut u64) -> i32 {
    if sig < 0 || sig as usize >= NSIG || get_current().is_null() { return -1; }
    if sig == SIGKILL { return -1; }

    if handler != 0 && handler != 1 {
        if handler > 0x00007FFFFFFFFFFF || handler < 0x1000 {
            return -1;
        }
    }

    if !old_handler.is_null() {
        *old_handler = (*get_current()).sig_handlers[sig as usize].handler_addr;
    }
    (*get_current()).sig_handlers[sig as usize].handler_addr = handler;
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_sigreturn(r: *mut Registers) -> i32 {
    if get_current().is_null() { return -1; }

    let user_sp = (*r).user_rsp as *mut u64;
    let mut sigframe = [0u64; 7];

    // copy from user
    core::arch::asm!("stac");
    for i in 0..7 {
        sigframe[i] = ptr::read_volatile(user_sp.add(i));
    }
    core::arch::asm!("clac");

    let new_rip = sigframe[1];
    let new_cs = sigframe[2];
    let new_rsp = sigframe[4];
    let new_ss = sigframe[5];

    if new_rip < 0x1000 || new_rip > 0x00007FFFFFFFFFFF {
        return -1;
    }
    if new_cs != 0x23 && new_cs != 0x1B {
        return -1;
    }
    if new_ss != 0x2B && new_ss != 0x23 {
        return -1;
    }
    if new_rsp < 0x1000 || new_rsp > 0x00007FFFFFFFFFFF {
        return -1;
    }

    (*r).rip = new_rip;
    (*r).cs = new_cs;
    (*r).rflags = sigframe[3] & 0xFFFFFFFFFFFFFCFF;
    (*r).user_rsp = new_rsp;
    (*r).ss = new_ss;
    (*r).rax = sigframe[0];

    if sigframe[0] > 0 && (sigframe[0] as usize) < NSIG {
        (*get_current()).sig_pending &= !(1 << sigframe[0]);
    }

    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_deliver_signals(r: *mut Registers) {
    if get_current().is_null() || (*get_current()).sig_pending == 0 { return; }

    for sig in 1..NSIG {
        if (*get_current()).sig_pending & (1 << sig) == 0 { continue; }
        if (*get_current()).sig_blocked & (1 << sig) != 0 { continue; }

        let handler = (*get_current()).sig_handlers[sig].handler_addr;

        if handler == 1 {
            // Ignore
            (*get_current()).sig_pending &= !(1 << sig);
            continue;
        }

        if handler == 0 {
            // Default action
            if sig == SIGKILL as usize || sig == 11 || sig == 15 {
                (*get_current()).exit_code = 0x100 + sig as u32;
                (*get_current()).state = TaskState::EXITED;
                if !(*get_current()).parent.is_null() {
                    (*(*get_current()).parent).has_child_exit = true;
                }
                return;
            }
            (*get_current()).sig_pending &= !(1 << sig);
            continue;
        }

        // Custom handler: push sigframe on user stack, call handler
        let old_usp = (*r).user_rsp;
        let new_usp = old_usp - core::mem::size_of::<u64>() as u64 * 7;
        if new_usp < 0x1000 {
            (*get_current()).exit_code = 0x100 + sig as u32;
            (*get_current()).state = TaskState::EXITED;
            if !(*get_current()).parent.is_null() {
                (*(*get_current()).parent).has_child_exit = true;
            }
            return;
        }

        let sigframe: [u64; 7] = [
            sig as u64,
            (*r).rip,
            (*r).cs,
            (*r).rflags,
            old_usp,
            (*r).ss,
            0,
        ];
        // copy to user
        core::arch::asm!("stac");
        for i in 0..7 {
            ptr::write_volatile((new_usp as *mut u64).add(i), sigframe[i]);
        }
        core::arch::asm!("clac");

        (*get_current()).sig_pending &= !(1 << sig);
        (*r).user_rsp = new_usp;
        (*r).rip = handler;
        (*r).rdi = sig as u64;
        return;
    }
}

// ─── EXIT ──────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn krust_sched_exit(code: u32) {
    if get_current().is_null() || (*get_current()).id == 0 { return; }

    fpu_save_current();
    (*get_current()).exit_code = code;
    (*get_current()).state = TaskState::EXITED;

    if !(*get_current()).parent.is_null() {
        (*(*get_current()).parent).has_child_exit = true;
    }

    // Reparent children to idle
    let mut child = (*get_current()).child_head;
    while !child.is_null() {
        (*child).parent = &mut TASKS[0];
        child = (*child).sibling_next;
    }
    (*get_current()).child_head = ptr::null_mut();
    (*get_current()).child_tail = ptr::null_mut();

    let mut next = dequeue();
    if next.is_null() {
        next = &mut TASKS[0];
        if (*next).ctx.rsp == 0 {
            // No idle context — halt
            unsafe { core::arch::asm!("cli"); }
            loop { unsafe { core::arch::asm!("hlt"); } }
        }
    }
    set_current(next);
    krust_sched_sync_cpp();
    (*next).state = TaskState::RUNNING;

    if !(*next).kstack.is_null() {
        let kst = (*next).kstack as u64 + PAGE_SIZE;
        krust_tss_set_kernel_stack(kst);
        crate::smp::smp_set_kernel_stack(kst);
    }

    fpu_restore_current();
    task_resume(&(*next).ctx);
}

// ─── PREEMPT / YIELD ──────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn krust_sched_preempt(r: *mut Registers) {
    crate::apic_hw::krust_apic_eoi();
    if get_current().is_null() { return; }

    sleep_wake_all(crate::pittimer::krust_pittimer_get_ticks());
    krust_sched_deliver_signals(r);
    fpu_save_current();
    let prev = get_current();

    // Idle tasks have kstack == null (BSP idle id=0, AP idle has unique id)
    let is_idle = (*prev).kstack.is_null();

    if is_idle {
        (*prev).ctx.rsp = r as u64;
        let next = dequeue();
        if !next.is_null() {
            set_current(next);
            krust_sched_sync_cpp();
            (*next).state = TaskState::RUNNING;
            if !(*next).kstack.is_null() {
                let kst = (*next).kstack as u64 + PAGE_SIZE;
                krust_tss_set_kernel_stack(kst);
                crate::smp::smp_set_kernel_stack(kst);
            }
            fpu_restore_current();
            task_resume(&(*next).ctx);
        }
        fpu_restore_current();
        return;
    }

    (*prev).ctx.rsp = r as u64;
    (*prev).state = TaskState::READY;
    enqueue(prev);

    let next = dequeue();
    if next.is_null() {
        set_current(prev);
        krust_sched_sync_cpp();
        (*prev).state = TaskState::RUNNING;
        fpu_restore_current();
        return;
    }

    set_current(next);
    krust_sched_sync_cpp();
    (*next).state = TaskState::RUNNING;
    if !(*next).kstack.is_null() {
        let kst = (*next).kstack as u64 + PAGE_SIZE;
        krust_tss_set_kernel_stack(kst);
        crate::smp::smp_set_kernel_stack(kst);
    }
    fpu_restore_current();
    // Notify other CPUs that new tasks may be available
    crate::smp::smp_reschedule_all();
    task_resume(&(*next).ctx);
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_yield_handler(r: *mut Registers) {
    if get_current().is_null() { return; }

    sleep_wake_all(crate::pittimer::krust_pittimer_get_ticks());
    krust_sched_deliver_signals(r);
    fpu_save_current();
    let prev = get_current();

    if (*prev).id == 0 {
        (*prev).ctx.rsp = r as u64;
        let next = dequeue();
        if !next.is_null() {
            set_current(next);
            krust_sched_sync_cpp();
            (*next).state = TaskState::RUNNING;
            if !(*next).kstack.is_null() {
                let kst = (*next).kstack as u64 + PAGE_SIZE;
                krust_tss_set_kernel_stack(kst);
                crate::smp::smp_set_kernel_stack(kst);
            }
            fpu_restore_current();
            crate::smp::smp_reschedule_all();
            task_resume(&(*next).ctx);
        }
        fpu_restore_current();
        return;
    }

    (*prev).ctx.rsp = r as u64;
    (*prev).state = TaskState::READY;
    enqueue(prev);

    let next = dequeue();
    if next.is_null() {
        set_current(prev);
        krust_sched_sync_cpp();
        (*prev).state = TaskState::RUNNING;
        fpu_restore_current();
        return;
    }
    set_current(next);
    krust_sched_sync_cpp();
    (*next).state = TaskState::RUNNING;

    if !(*next).kstack.is_null() {
        let kst = (*next).kstack as u64 + PAGE_SIZE;
        krust_tss_set_kernel_stack(kst);
        crate::smp::smp_set_kernel_stack(kst);
    }
    fpu_restore_current();
    crate::smp::smp_reschedule_all();
    task_resume(&(*next).ctx);
}

// ─── yield_handler_c (called from switch.asm) ──────────────────

#[no_mangle]
pub unsafe extern "C" fn yield_handler_c(r: *mut Registers) {
    krust_sched_yield_handler(r);
}

// ─── Accessors for C++ ─────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn krust_sched_get_task_field_stdout_fd(id: u32) -> i32 {
    if id < config::MAX_TASKS { TASKS[id as usize].stdout_fd } else { -1 }
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_get_task_field_stdin_fd(id: u32) -> i32 {
    if id < config::MAX_TASKS { TASKS[id as usize].stdin_fd } else { -1 }
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_get_task_field_cwd(id: u32) -> *mut u8 {
    if id < config::MAX_TASKS { TASKS[id as usize].cwd.as_mut_ptr() } else { ptr::null_mut() }
}

// ─── Thread/Clone syscalls ─────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn krust_sched_clone(r: *mut Registers, _flags: u64, stack: u64, ptid: *mut u32, tls: u64, ctid: *mut u32) -> i32 {
    if r.is_null() { return -1; }
    let parent = get_current();
    if parent.is_null() { return -1; }
    let child_id = match alloc_task_id() { Some(id) => id, None => return -1 };

    let child_kstack = alloc_frame();
    if child_kstack.is_null() {
        return -1;
    }
    memset(child_kstack as *mut u8, 0, PAGE_SIZE as usize);

    let child = &mut TASKS[child_id as usize];
    setup_task_common(child, child_id, child_kstack, parent);

    // Copy kernel stack contents
    let parent_kstack_start = (*parent).kstack as u64;
    let parent_r_offset = r as u64 - parent_kstack_start;
    let child_r_addr = child_kstack as u64 + parent_r_offset;
    let child_r = child_r_addr as *mut Registers;
    memcpy(child_kstack as *mut u8, (*parent).kstack as *const u8, PAGE_SIZE as usize);

    (*child_r).rax = 0;  // child returns 0

    // Clone page tables
    let child_pml4 = krust_paging_clone_kernel_dir();
    if child_pml4.is_null() {
        free_frame(child_kstack);
        return -1;
    }

    // Copy user pages
    krust_paging_copy_user_pages((*parent).ctx.cr3 as *mut u64, child_pml4);

    // Copy task properties
    (*child).stdin_fd = (*parent).stdin_fd;
    (*child).stdout_fd = (*parent).stdout_fd;
    (*child).stderr_fd = (*parent).stderr_fd;
    (*child).fd_table = copy_fd_table((*parent).fd_table);
    (*child).ustack = (*parent).ustack;
    (*child).pages = (*parent).pages;
    
    // Copy cwd
    let mut ci = 0;
    while ci < 127 && (*parent).cwd[ci] != 0 {
        (*child).cwd[ci] = (*parent).cwd[ci];
        ci += 1;
    }
    (*child).cwd[ci] = 0;
    (*child).ppid = (*parent).id;

    // Copy VMA list
    (*child).vma_list = ptr::null_mut();
    (*child).program_brk = (*parent).program_brk;
    let mut v = (*parent).vma_list;
    while !v.is_null() {
        krust_vmm_add(&mut (*child).vma_list, (*v).start, (*v).end, (*v).flags);
        v = (*v).next;
    }

    // Copy signal handlers
    for i in 0..NSIG {
        (*child).sig_handlers[i].handler_addr = (*parent).sig_handlers[i].handler_addr;
    }
    (*child).sig_blocked = (*parent).sig_blocked;

    // Handle flags
    if _flags & 0x1000 != 0 { // CLONE_VM - share memory (thread)
        (*child).ctx.cr3 = (*parent).ctx.cr3;
        // Don't free user pages on exit for threads
    } else {
        (*child).ctx.cr3 = child_pml4 as u64;
    }

    if stack != 0 {
        (*child_r).user_rsp = stack;
        (*child).ustack = stack as *mut u64;
    }

    // Set up context for scheduler
    let saved_cr3 = (*child).ctx.cr3;
    memset(&mut (*child).ctx as *mut _ as *mut u8, 0, core::mem::size_of::<TaskContext>());
    (*child).ctx.rsp = child_r_addr;
    (*child).ctx.cr3 = saved_cr3;

    add_child(parent, child);

    let kstack_top = (*parent).kstack as u64 + PAGE_SIZE;
    krust_tss_set_kernel_stack(kstack_top);
    crate::smp::smp_set_kernel_stack(kstack_top);
    enqueue(child);

    // Set thread IDs
    if !ptid.is_null() {
        *ptid = child_id;
    }
    if !ctid.is_null() {
        *ctid = child_id;
    }

    child_id as i32
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_thread_create(entry: extern "C" fn(u64), arg: u64, stack: u64, _flags: u64) -> i32 {
    let id = match alloc_task_id() { Some(id) => id, None => return -1 };

    let kstack = alloc_frame();
    if kstack.is_null() {
        return -1;
    }
    memset(kstack as *mut u8, 0, PAGE_SIZE as usize);

    let t = &mut TASKS[id as usize];
    setup_task_common(t, id, kstack, ptr::null_mut());
    (*t).stdin_fd = -1;
    (*t).stdout_fd = -1;
    (*t).stderr_fd = -1;

    let sp_top = (kstack as u64 + PAGE_SIZE) as *mut u64;
    
    // Build user-mode stack frame for thread entry
    let mut sp = sp_top;
    sp = sp.sub(1); *sp = 0x23;                // ss
    sp = sp.sub(1); *sp = stack;                // user rsp
    sp = sp.sub(1); *sp = 0x202;               // rflags
    sp = sp.sub(1); *sp = 0x1B;                // cs
    sp = sp.sub(1); *sp = entry as u64;        // rip
    sp = sp.sub(1); *sp = 0;                   // err_code
    sp = sp.sub(1); *sp = 0;                   // int_no
    sp = sp.sub(1); *sp = arg;                 // rdi (first arg)
    sp = sp.sub(1); *sp = 0;                   // rcx
    sp = sp.sub(1); *sp = 0;                   // rdx
    sp = sp.sub(1); *sp = 0;                   // rbx
    sp = sp.sub(1); *sp = stack;               // rsp (dummy)
    sp = sp.sub(1); *sp = 0;                   // rbp
    sp = sp.sub(1); *sp = 0;                   // rsi
    sp = sp.sub(1); *sp = 0;                   // rdi
    sp = sp.sub(1); *sp = 0;                   // r8
    sp = sp.sub(1); *sp = 0;                   // r9
    sp = sp.sub(1); *sp = 0;                   // r10
    sp = sp.sub(1); *sp = 0;                   // r11
    sp = sp.sub(1); *sp = 0;                   // r12
    sp = sp.sub(1); *sp = 0;                   // r13
    sp = sp.sub(1); *sp = 0;                   // r14
    sp = sp.sub(1); *sp = 0;                   // r15

    memset(&mut (*t).ctx as *mut _ as *mut u8, 0, core::mem::size_of::<TaskContext>());
    (*t).ctx.rsp = sp as u64;
    (*t).ctx.cr3 = krust_paging_page_directory() as u64;

    (*t).ustack = stack as *mut u64;

    let kstack_top = sp_top as u64;
    krust_tss_set_kernel_stack(kstack_top);
    crate::smp::smp_set_kernel_stack(kstack_top);
    enqueue(t);
    id as i32
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_thread_join(tid: u32) -> i32 {
    if get_current().is_null() { return -1; }
    if tid >= NEXT_ID.load(Ordering::Relaxed) { return -1; }
    
    let target = &mut TASKS[tid as usize];
    if (*target).state == TaskState::EXITED {
        return 0; // Already exited
    }

    // Wait for thread to exit
    while (*target).state != TaskState::EXITED {
        (*get_current()).state = TaskState::WAITING;
        yield_task();
        (*get_current()).state = TaskState::RUNNING;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_sched_thread_detach(tid: u32) -> i32 {
    if tid >= NEXT_ID.load(Ordering::Relaxed) { return -1; }
    let target = &mut TASKS[tid as usize];
    // Mark as detached - will be cleaned up on exit
    (*target).ppid = 0; // Reparent to init
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_has_fxsr() -> i32 {
    let ecx: u32;
    let eax_in: u32 = 1;
    let ecx_in: u32 = 0;
    core::arch::asm!("cpuid", inout("eax") eax_in => _, inout("ecx") ecx_in => ecx);
    if ecx & (1 << 24) != 0 { 1 } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn krust_fpu_save(state: *mut u8) {
    core::arch::asm!("fxsave [{}]", in(reg) state);
}

#[no_mangle]
pub unsafe extern "C" fn krust_fpu_restore(state: *mut u8) {
    core::arch::asm!("fxrstor [{}]", in(reg) state);
}

// ─── OOM killer ─────────────────────────────────────────────────

unsafe fn oom_score(task: *mut Task) -> i64 {
    if (*task).state == TaskState::EXITED { return -1; }
    if (*task).id == 0 { return -1; }
    let pages = (*task).pages as i64;
    let adj = (*task).oom_score_adj as i64;
    let mut score = pages * 100 + (adj + 1000);
    if (*task).uid == 0 { score -= 500; }
    score
}

unsafe fn eager_cleanup_task(task: *mut Task) {
    if !(*task).fd_table.is_null() {
        for i in 0..MAX_FDS {
            if (*task).fd_table.add(i).read().used {
                (*task).fd_table.add(i).write(crate::scheduler::FDEntry {
                    node: ptr::null_mut(),
                    offset: 0,
                    flags: 0,
                    used: false,
                    refcount: 0,
                });
            }
        }
    }
    if !(*task).vma_list.is_null() {
        let mut vma = (*task).vma_list;
        while !vma.is_null() {
            let next = (*vma).next;
            krust_free(vma as *mut u8);
            vma = next;
        }
        (*task).vma_list = ptr::null_mut();
    }
    // Don't free kstack if task is currently running on any CPU
    let mut running = false;
    for cpu in 0..MAX_CPUS_SCHED {
        if PER_CPU_CURRENT[cpu] == task {
            running = true;
            break;
        }
    }
    if !running && !(*task).kstack.is_null() {
        crate::mm::pmm::krust_pmm_free_frame((*task).kstack as usize / PAGE_SIZE as usize);
        (*task).kstack = ptr::null_mut();
    }
    if !(*task).fpu_state.is_null() {
        crate::mm::pmm::krust_pmm_free_frame((*task).fpu_state as usize / PAGE_SIZE as usize);
        (*task).fpu_state = ptr::null_mut();
    }
    // Free user page tables and physical pages
    if (*task).ctx.cr3 != 0 {
        crate::paging::krust_paging_free_user_pages((*task).ctx.cr3);
        (*task).ctx.cr3 = 0;
    }
    (*task).pages = 0;
}

#[no_mangle]
pub unsafe extern "C" fn krust_oom_kill() -> i32 {
    let next_id = NEXT_ID.load(Ordering::Relaxed);
    let mut best_idx: i32 = -1;
    let mut best_score: i64 = -1;
    let current_task = get_current();

    for i in 1..next_id {
        let t = &mut TASKS[i as usize];
        // Never kill idle task (id=0) or the currently running task
        if t as *mut Task == current_task { continue; }
        let s = oom_score(t);
        if s > best_score {
            best_score = s;
            best_idx = i as i32;
        }
    }

    if best_idx < 0 {
        return -1;
    }

    let victim = &mut TASKS[best_idx as usize];
    (*victim).exit_code = 0x100 + 9;
    (*victim).state = TaskState::EXITED;
    // Reparent children to init (task 1)
    let mut child = (*victim).child_head;
    while !child.is_null() {
        let next = (*child).sibling_next;
        (*child).parent = &mut TASKS[1];
        (*child).sibling_next = TASKS[1].child_head;
        TASKS[1].child_head = child;
        child = next;
    }
    (*victim).child_head = ptr::null_mut();
    (*victim).child_tail = ptr::null_mut();
    if !(*victim).parent.is_null() {
        (*(*victim).parent).has_child_exit = true;
    }

    eager_cleanup_task(victim);

    crate::serial::krust_serial_writestring(b"oom killed\n\0".as_ptr());

    best_idx
}
