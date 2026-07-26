use core::ptr;
use crate::scheduler::{VNode, FDEntry, MAX_FDS};
use crate::heap::{krust_malloc, krust_free};
use crate::klib::{krust_strncpy, krust_memcpy, krust_memset, krust_strcmp};

const NODE_FILE: u8 = 0;
const NODE_DIR: u8 = 1;
const NODE_DEVICE: u8 = 2;
const NODE_SYMLINK: u8 = 3;

const MAX_PIPES: usize = 16;
const PIPE_BUF_SIZE: usize = 4096;
const PIPE_FD_START: i32 = 32;

#[repr(C)]
pub struct FileStat {
    pub type_: u8,
    pub size: u32,
    pub name: [u8; 64],
    pub uid: u16,
    pub gid: u16,
    pub mode: u16,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct PipeBuffer {
    buf: [u8; PIPE_BUF_SIZE],
    head: u32,
    tail: u32,
    read_open: bool,
    write_open: bool,
    used: bool,
    readers: u32,
    writers: u32,
    read_fd: i32,
    write_fd: i32,
}

static mut ROOT: *mut VNode = ptr::null_mut();
static mut PIPES: [PipeBuffer; MAX_PIPES] = [PipeBuffer {
    buf: [0u8; PIPE_BUF_SIZE],
    head: 0,
    tail: 0,
    read_open: false,
    write_open: false,
    used: false,
    readers: 0,
    writers: 0,
    read_fd: 0,
    write_fd: 0,
}; MAX_PIPES];
static mut NEXT_PIPE_FD: i32 = PIPE_FD_START;

unsafe fn current_fd_table() -> *mut FDEntry {
    crate::scheduler::krust_sched_current_fd_table()
}

pub unsafe fn fd_ref() -> &'static mut [FDEntry; MAX_FDS] {
    let p = current_fd_table();
    if p.is_null() {
        static mut DUMMY: [FDEntry; MAX_FDS] = [FDEntry {
            node: ptr::null_mut(), offset: 0, flags: 0, used: false, refcount: 0,
        }; MAX_FDS];
        &mut DUMMY
    } else {
        &mut *(p as *mut [FDEntry; MAX_FDS])
    }
}

unsafe fn alloc_fd() -> i32 {
    let table = current_fd_table();
    if table.is_null() { return -1; }
    for i in 0..MAX_FDS {
        if !(*table.add(i)).used {
            return i as i32;
        }
    }
    -1
}

unsafe fn create_node_raw(name: *const u8, node_type: u8) -> *mut VNode {
    let node = krust_malloc(core::mem::size_of::<VNode>() as u32) as *mut VNode;
    if node.is_null() {
        return ptr::null_mut();
    }
    krust_memset(node as *mut u8, 0, core::mem::size_of::<VNode>());
    krust_strncpy((*node).name.as_mut_ptr(), name, 63);
    (*node).type_ = node_type;
    let current = crate::scheduler::krust_sched_current();
    if !current.is_null() {
        (*node).uid = (*current).uid;
        (*node).gid = (*current).gid;
    }
    match node_type {
        NODE_FILE    => { (*node).mode = 0o100644; }
        NODE_DIR     => { (*node).mode = 0o040755; }
        NODE_DEVICE  => { (*node).mode = 0o020666; }
        NODE_SYMLINK => { (*node).mode = 0o120777; }
        _ => {}
    }
    node
}

unsafe fn check_permission(node: *mut VNode, want: u16) -> bool {
    if node.is_null() { return false; }
    let current = crate::scheduler::krust_sched_current();
    if !current.is_null() && (*current).uid == 0 {
        return true;
    }
    if current.is_null() { return true; }
    let nuid = (*node).uid;
    let ngid = (*node).gid;
    let nmode = (*node).mode;
    let tuid = (*current).uid;
    let tgid = (*current).gid;
    let bits = if tuid == nuid {
        (nmode >> 6) & 7
    } else if tgid == ngid {
        (nmode >> 3) & 7
    } else {
        nmode & 7
    };
    (bits & want) == want
}

unsafe fn find_child(dir: *mut VNode, name: *const u8) -> *mut VNode {
    if dir.is_null() || name.is_null() {
        return ptr::null_mut();
    }
    if krust_strcmp(name, b".\0".as_ptr()) == 0 {
        return dir;
    }
    if krust_strcmp(name, b"..\0".as_ptr()) == 0 {
        return if (*dir).parent.is_null() { dir } else { (*dir).parent };
    }
    let mut c = (*dir).children;
    while !c.is_null() {
        if krust_strcmp((*c).name.as_ptr(), name) == 0 {
            return c;
        }
        c = (*c).next;
    }
    ptr::null_mut()
}

unsafe fn add_child(parent: *mut VNode, child: *mut VNode) {
    (*child).next = (*parent).children;
    (*parent).children = child;
}

unsafe fn resolve_path(base: *mut VNode, path: *const u8) -> *mut VNode {
    resolve_path_inner(base, path, 0)
}

unsafe fn resolve_path_inner(base: *mut VNode, path: *const u8, depth: u32) -> *mut VNode {
    if depth > 5 {
        return ptr::null_mut();
    }
    if base.is_null() || path.is_null() {
        return ptr::null_mut();
    }
    let mut p = path;
    while *p == b'/' {
        p = p.add(1);
    }
    if *p == 0 {
        return base;
    }
    let mut segment = [0u8; 64];
    let mut i: usize = 0;
    while *p != 0 && *p != b'/' {
        if i >= 63 {
            return ptr::null_mut();
        }
        segment[i] = *p;
        i += 1;
        p = p.add(1);
    }
    segment[i] = 0;

    let child = find_child(base, segment.as_ptr());
    if child.is_null() {
        return ptr::null_mut();
    }

    if (*child).type_ == NODE_SYMLINK {
        let target = (*child).link_target.as_ptr();
        let mut buf = [0u8; 512];
        let mut len: usize = 0;
        let mut t = target;
        while *t != 0 && len < 255 {
            buf[len] = *t;
            len += 1;
            t = t.add(1);
        }
        while *p != 0 && len < 510 {
            buf[len] = *p;
            len += 1;
            p = p.add(1);
        }
        buf[len] = 0;
        if (*child).link_target[0] == b'/' {
            return resolve_path_inner(ROOT, buf.as_ptr(), depth + 1);
        } else {
            return resolve_path_inner(base, buf.as_ptr(), depth + 1);
        }
    }

    resolve_path_inner(child, p, depth)
}

unsafe fn resolve_parent(path: *const u8) -> *mut VNode {
    let mut buf = [0u8; 256];
    krust_strncpy(buf.as_mut_ptr(), path, 255);
    let mut last_slash: *mut u8 = ptr::null_mut();
    let mut p = buf.as_mut_ptr();
    while *p != 0 {
        if *p == b'/' {
            last_slash = p;
        }
        p = p.add(1);
    }
    if last_slash.is_null() {
        return ROOT;
    }
    *last_slash = 0;
    resolve_path(ROOT, buf.as_ptr())
}

unsafe fn extract_name<'a>(path: &'a *const u8) -> *const u8 {
    let mut name = *path;
    let mut p = *path;
    while *p != 0 {
        if *p == b'/' {
            name = p.add(1);
        }
        p = p.add(1);
    }
    name
}

// alloc_fd is defined once at line 71

unsafe fn pipe_from_fd(fd: i32, is_read: bool) -> *mut PipeBuffer {
    for i in 0..MAX_PIPES {
        if !PIPES[i].used {
            continue;
        }
        if is_read && PIPES[i].read_fd == fd {
            return &mut PIPES[i];
        }
        if !is_read && PIPES[i].write_fd == fd {
            return &mut PIPES[i];
        }
    }
    ptr::null_mut()
}

pub unsafe fn pipe_has_data(fd: i32) -> bool {
    let p = pipe_from_fd(fd, true);
    if p.is_null() { return false; }
    (*p).head != (*p).tail
}

pub unsafe fn pipe_can_write(fd: i32) -> bool {
    let p = pipe_from_fd(fd, false);
    if p.is_null() { return false; }
    let next = ((*p).head + 1) % PIPE_BUF_SIZE as u32;
    next != (*p).tail
}

pub unsafe fn is_pipe_fd(fd: i32) -> bool {
    !pipe_from_fd(fd, true).is_null() || !pipe_from_fd(fd, false).is_null()
}

fn count_nodes_recursive(node: *mut VNode) -> u32 {
    if node.is_null() {
        return 0;
    }
    let mut count = 1u32;
    unsafe {
        let mut child = (*node).children;
        while !child.is_null() {
            count += count_nodes_recursive(child);
            child = (*child).next;
        }
    }
    count
}

// ─── Public API ───────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_init() {
    krust_memset(PIPES.as_mut_ptr() as *mut u8, 0, core::mem::size_of::<PipeBuffer>() * MAX_PIPES);
    NEXT_PIPE_FD = PIPE_FD_START;

    ROOT = create_node_raw(b"\0".as_ptr(), NODE_DIR);
    (*ROOT).parent = ROOT;
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_resolve(path: *const u8) -> *mut VNode {
    if path.is_null() || *path == 0 || ROOT.is_null() {
        return ROOT;
    }
    resolve_path(ROOT, path)
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_create_dir(path: *const u8) -> i32 {
    if path.is_null() || *path == 0 {
        return -1;
    }
    let parent = resolve_parent(path);
    if parent.is_null() || (*parent).type_ != NODE_DIR {
        return -1;
    }
    if !check_permission(parent, 6) {
        return -1;
    }
    let name = extract_name(&path);
    if *name == 0 {
        return -1;
    }
    if !find_child(parent, name).is_null() {
        return -1;
    }
    let node = create_node_raw(name, NODE_DIR);
    if node.is_null() {
        return -1;
    }
    (*node).parent = parent;
    add_child(parent, node);
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_create_file(path: *const u8, data: *const u8, size: u32) -> i32 {
    if path.is_null() || *path == 0 {
        return -1;
    }
    let parent = resolve_parent(path);
    if parent.is_null() || (*parent).type_ != NODE_DIR {
        return -1;
    }
    if !check_permission(parent, 2) { return -1; } // need write on parent
    let name = extract_name(&path);
    if *name == 0 {
        return -1;
    }
    let node = create_node_raw(name, NODE_FILE);
    if node.is_null() {
        return -1;
    }
    (*node).parent = parent;
    (*node).size = size;
    if size > 0 && !data.is_null() {
        (*node).data = krust_malloc(size);
        if (*node).data.is_null() {
            krust_free(node as *mut u8);
            return -1;
        }
        krust_memcpy((*node).data, data, size as usize);
    }
    add_child(parent, node);
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_create_device(
    path: *const u8,
    dev_read: Option<extern "C" fn(*mut VNode, *mut u8, u32, u32) -> i32>,
    dev_write: Option<extern "C" fn(*mut VNode, *const u8, u32, u32) -> i32>,
) -> i32 {
    if path.is_null() || *path == 0 {
        return -1;
    }
    let parent = resolve_parent(path);
    if parent.is_null() || (*parent).type_ != NODE_DIR {
        return -1;
    }
    let name = extract_name(&path);
    if *name == 0 {
        return -1;
    }
    if !find_child(parent, name).is_null() {
        return -1;
    }
    let node = create_node_raw(name, NODE_DEVICE);
    if node.is_null() {
        return -1;
    }
    (*node).parent = parent;
    (*node).dev_read = dev_read;
    (*node).dev_write = dev_write;
    add_child(parent, node);
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_remove_node(path: *const u8) -> i32 {
    if path.is_null() || *path == 0 {
        return -1;
    }
    let node = krust_vfs_resolve(path);
    if node.is_null() || node == ROOT {
        return -1;
    }
    let parent = (*node).parent;
    if parent.is_null() {
        return -1;
    }

    let mut pp = &mut (*parent).children;
    while !(*pp).is_null() {
        if *pp == node {
            *pp = (*node).next;
            break;
        }
        pp = &mut (**pp).next;
    }

    let mut child = (*node).children;
    while !child.is_null() {
        let next = (*child).next;
        if !(*child).data.is_null() {
            krust_free((*child).data);
        }
        krust_free(child as *mut u8);
        child = next;
    }

    if !(*node).data.is_null() {
        krust_free((*node).data);
    }
    krust_free(node as *mut u8);
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_write_file(path: *const u8, data: *const u8, size: u32) -> i32 {
    if path.is_null() || *path == 0 {
        return -1;
    }
    let node = krust_vfs_resolve(path);
    if !node.is_null() && (*node).type_ == NODE_FILE {
        if !check_permission(node, 2) { return -1; } // need write on file
        if !(*node).data.is_null() {
            krust_free((*node).data);
            (*node).data = ptr::null_mut();
        }
        (*node).size = 0;
        if size > 0 && !data.is_null() {
            (*node).data = krust_malloc(size);
            if (*node).data.is_null() {
                return -1;
            }
            krust_memcpy((*node).data, data, size as usize);
        }
        (*node).size = size;
        return 0;
    }

    if krust_vfs_create_file(path, data, size) != 0 {
        return -1;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_open(path: *const u8) -> i32 {
    if path.is_null() || *path == 0 {
        return -1;
    }
    let node = krust_vfs_resolve(path);
    if node.is_null() || ((*node).type_ != NODE_FILE && (*node).type_ != NODE_DEVICE) {
        return -1;
    }
    if !check_permission(node, 4) {
        return -1;
    }
    let fd = alloc_fd();
    if fd < 0 {
        return -1;
    }
    fd_ref()[fd as usize].used = true;
    fd_ref()[fd as usize].node = node;
    fd_ref()[fd as usize].offset = 0;
    fd_ref()[fd as usize].flags = 0;
    fd_ref()[fd as usize].refcount = 1;
    fd
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_open_write(path: *const u8) -> i32 {
    if path.is_null() || *path == 0 {
        return -1;
    }
    let node = krust_vfs_resolve(path);
    if !node.is_null() && (*node).type_ == NODE_FILE {
        if !check_permission(node, 2) {
            return -1;
        }
        if !(*node).data.is_null() {
            krust_free((*node).data);
            (*node).data = ptr::null_mut();
        }
        (*node).size = 0;
    } else if node.is_null() {
        let parent = resolve_parent(path);
        if parent.is_null() || (*parent).type_ != NODE_DIR {
            return -1;
        }
        if !check_permission(parent, 2) {
            return -1;
        }
        let name = extract_name(&path);
        if *name == 0 {
            return -1;
        }
        let new_node = create_node_raw(name, NODE_FILE);
        if new_node.is_null() {
            return -1;
        }
        (*new_node).parent = parent;
        add_child(parent, new_node);
        let fd = alloc_fd();
        if fd < 0 {
            return -1;
        }
        fd_ref()[fd as usize].used = true;
        fd_ref()[fd as usize].node = new_node;
        fd_ref()[fd as usize].offset = 0;
        fd_ref()[fd as usize].flags = 0;
        fd_ref()[fd as usize].refcount = 1;
        return fd;
    } else {
        return -1;
    }

    let fd = alloc_fd();
    if fd < 0 {
        return -1;
    }
    fd_ref()[fd as usize].used = true;
    fd_ref()[fd as usize].node = node;
    fd_ref()[fd as usize].offset = 0;
    fd_ref()[fd as usize].flags = 0;
    fd_ref()[fd as usize].refcount = 1;
    fd
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_read(fd: i32, buf: *mut u8, size: u32) -> i32 {
    if fd < 0 || fd >= MAX_FDS as i32 || !fd_ref()[fd as usize].used {
        return -1;
    }
    let f = &mut fd_ref()[fd as usize];
    let node = (*f).node;

    if !check_permission(node, 4) {
        return -1;
    }

    if (*node).type_ == NODE_DEVICE {
        if let Some(read_fn) = (*node).dev_read {
            let n = read_fn(node, buf, size, (*f).offset);
            if n > 0 {
                (*f).offset += n as u32;
            }
            return n;
        }
        return 0;
    }

    if (*f).offset >= (*node).size {
        return 0;
    }
    let avail = (*node).size - (*f).offset;
    let to_read = if size < avail { size } else { avail };
    krust_memcpy(buf, (*node).data.add((*f).offset as usize), to_read as usize);
    (*f).offset += to_read;
    to_read as i32
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_write_fd(fd: i32, data: *const u8, size: u32) -> i32 {
    if fd < 0 || fd >= MAX_FDS as i32 || !fd_ref()[fd as usize].used {
        return -1;
    }
    let f = &mut fd_ref()[fd as usize];
    let node = (*f).node;

    if !check_permission(node, 2) {
        return -1;
    }

    if (*node).type_ == NODE_DEVICE {
        if let Some(write_fn) = (*node).dev_write {
            let n = write_fn(node, data, size, (*f).offset);
            if n > 0 {
                (*f).offset += n as u32;
            }
            return n;
        }
        return size as i32;
    }

    if (*node).type_ != NODE_FILE {
        return -1;
    }

    let new_size = (*f).offset + size;
    if new_size > (*node).size {
        let new_data = krust_malloc(new_size);
        if new_data.is_null() {
            return -1;
        }
        if !(*node).data.is_null() {
            krust_memcpy(new_data, (*node).data, (*node).size as usize);
            krust_free((*node).data);
        }
        krust_memset(new_data.add((*node).size as usize), 0, (new_size - (*node).size) as usize);
        (*node).data = new_data;
        (*node).size = new_size;
    }
    krust_memcpy((*node).data.add((*f).offset as usize), data, size as usize);
    (*f).offset += size;
    size as i32
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_close(fd: i32) -> i32 {
    if fd < 0 || fd >= MAX_FDS as i32 || !fd_ref()[fd as usize].used {
        return -1;
    }
    if fd_ref()[fd as usize].refcount > 1 {
        fd_ref()[fd as usize].refcount -= 1;
    } else {
        fd_ref()[fd as usize].used = false;
        fd_ref()[fd as usize].node = ptr::null_mut();
        fd_ref()[fd as usize].offset = 0;
        fd_ref()[fd as usize].refcount = 0;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_mkdir(path: *const u8) -> i32 {
    krust_vfs_create_dir(path)
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_unlink(path: *const u8) -> i32 {
    if path.is_null() || *path == 0 {
        return -1;
    }
    let parent = resolve_parent(path);
    if parent.is_null() {
        return -1;
    }
    if !check_permission(parent, 6) {
        return -1;
    }
    let name = extract_name(&path);
    let node = find_child(parent, name);
    if node.is_null() {
        return -1;
    }
    if (*node).type_ != NODE_FILE && (*node).type_ != NODE_SYMLINK {
        return -1;
    }
    let mut pp = &mut (*parent).children;
    while !(*pp).is_null() {
        if *pp == node {
            *pp = (*node).next;
            break;
        }
        pp = &mut (**pp).next;
    }
    if !(*node).data.is_null() {
        krust_free((*node).data);
    }
    krust_free(node as *mut u8);
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_symlink(target: *const u8, linkpath: *const u8) -> i32 {
    if target.is_null() || *target == 0 || linkpath.is_null() || *linkpath == 0 {
        return -1;
    }
    let parent = resolve_parent(linkpath);
    if parent.is_null() || (*parent).type_ != NODE_DIR {
        return -1;
    }
    if !check_permission(parent, 2) { return -1; } // need write on parent
    let name = extract_name(&linkpath);
    if *name == 0 {
        return -1;
    }
    if !find_child(parent, name).is_null() {
        return -1;
    }
    let node = create_node_raw(name, NODE_SYMLINK);
    if node.is_null() {
        return -1;
    }
    (*node).parent = parent;
    let mut i: usize = 0;
    while *target.add(i) != 0 && i < 255 {
        (*node).link_target[i] = *target.add(i);
        i += 1;
    }
    (*node).link_target[i] = 0;
    add_child(parent, node);
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_readlink(path: *const u8, buf: *mut u8, bufsize: u32) -> i32 {
    if path.is_null() || buf.is_null() || bufsize == 0 {
        return -1;
    }
    let parent = resolve_parent(path);
    if parent.is_null() {
        return -1;
    }
    let name = extract_name(&path);
    let node = find_child(parent, name);
    if node.is_null() || (*node).type_ != NODE_SYMLINK {
        return -1;
    }
    let mut i: u32 = 0;
    while i < bufsize - 1 && (*node).link_target[i as usize] != 0 {
        ptr::write_volatile(buf.add(i as usize), (*node).link_target[i as usize]);
        i += 1;
    }
    ptr::write_volatile(buf.add(i as usize), 0);
    i as i32
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_rmdir(path: *const u8) -> i32 {
    if path.is_null() || *path == 0 {
        return -1;
    }
    let node = krust_vfs_resolve(path);
    if node.is_null() || (*node).type_ != NODE_DIR || node == ROOT {
        return -1;
    }
    let parent = (*node).parent;
    if !parent.is_null() && !check_permission(parent, 2) { return -1; } // need write on parent
    if !(*node).children.is_null() {
        return -1;
    }
    krust_vfs_remove_node(path)
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_rename(old_path: *const u8, new_path: *const u8) -> i32 {
    if old_path.is_null() || *old_path == 0 || new_path.is_null() || *new_path == 0 {
        return -1;
    }
    let old_node = krust_vfs_resolve(old_path);
    if old_node.is_null() || old_node == ROOT {
        return -1;
    }
    let old_parent = (*old_node).parent;
    if old_parent.is_null() {
        return -1;
    }
    if !check_permission(old_parent, 6) {
        return -1;
    }

    let mut pp = &mut (*old_parent).children;
    while !(*pp).is_null() {
        if *pp == old_node {
            *pp = (*old_node).next;
            break;
        }
        pp = &mut (**pp).next;
    }

    let new_parent = resolve_parent(new_path);
    if new_parent.is_null() {
        add_child(old_parent, old_node);
        return -1;
    }
    if !check_permission(new_parent, 6) {
        add_child(old_parent, old_node);
        return -1;
    }

    let new_name = extract_name(&new_path);
    krust_strncpy((*old_node).name.as_mut_ptr(), new_name, 63);
    (*old_node).parent = new_parent;
    (*old_node).next = ptr::null_mut();
    add_child(new_parent, old_node);
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_chmod(path: *const u8, mode: u16) -> i32 {
    if path.is_null() || *path == 0 {
        return -1;
    }
    let node = krust_vfs_resolve(path);
    if node.is_null() || node == ROOT {
        return -1;
    }
    let current = crate::scheduler::krust_sched_current();
    if !current.is_null() && (*current).uid != 0 && (*current).uid != (*node).uid {
        return -1;
    }
    (*node).mode = ((*node).mode & 0o170000) | (mode & 0o7777);
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_chown(path: *const u8, uid: u16, gid: u16) -> i32 {
    if path.is_null() || *path == 0 {
        return -1;
    }
    let node = krust_vfs_resolve(path);
    if node.is_null() || node == ROOT {
        return -1;
    }
    let current = crate::scheduler::krust_sched_current();
    if !current.is_null() && (*current).uid != 0 {
        return -1;
    }
    (*node).uid = uid;
    (*node).gid = gid;
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_stat(path: *const u8, stat: *mut FileStat) -> i32 {
    if path.is_null() || stat.is_null() {
        return -1;
    }
    let node = krust_vfs_resolve(path);
    if node.is_null() {
        return -1;
    }
    (*stat).type_ = (*node).type_;
    (*stat).size = (*node).size;
    krust_strncpy((*stat).name.as_mut_ptr(), (*node).name.as_ptr(), 63);
    (*stat).uid = (*node).uid;
    (*stat).gid = (*node).gid;
    (*stat).mode = (*node).mode;
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_lseek(fd: i32, offset: i64, whence: i32) -> i64 {
    if fd < 0 || fd >= MAX_FDS as i32 || !fd_ref()[fd as usize].used {
        return -1;
    }
    let f = &mut fd_ref()[fd as usize];
    let new_offset: i64 = match whence {
        0 => offset,
        1 => (*f).offset as i64 + offset,
        2 => (*(*f).node).size as i64 + offset,
        _ => return -1,
    };
    if new_offset < 0 {
        return -1;
    }
    (*f).offset = new_offset as u32;
    new_offset
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_dup(oldfd: i32) -> i32 {
    if oldfd < 0 || oldfd >= MAX_FDS as i32 || !fd_ref()[oldfd as usize].used {
        return -1;
    }
    let fd = alloc_fd();
    if fd < 0 {
        return -1;
    }
    fd_ref()[fd as usize].used = true;
    fd_ref()[fd as usize].node = fd_ref()[oldfd as usize].node;
    fd_ref()[fd as usize].offset = fd_ref()[oldfd as usize].offset;
    fd_ref()[fd as usize].flags = fd_ref()[oldfd as usize].flags;
    fd_ref()[fd as usize].refcount = 1;
    fd_ref()[oldfd as usize].refcount += 1;
    fd
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_dup2(oldfd: i32, newfd: i32) -> i32 {
    if oldfd < 0 || oldfd >= MAX_FDS as i32 || !fd_ref()[oldfd as usize].used {
        return -1;
    }
    if newfd < 0 || newfd >= MAX_FDS as i32 {
        return -1;
    }
    if oldfd == newfd {
        return newfd;
    }
    if fd_ref()[newfd as usize].used {
        fd_ref()[newfd as usize].used = false;
        fd_ref()[newfd as usize].node = ptr::null_mut();
        fd_ref()[newfd as usize].offset = 0;
        fd_ref()[newfd as usize].refcount = 0;
    }
    fd_ref()[newfd as usize].used = true;
    fd_ref()[newfd as usize].node = fd_ref()[oldfd as usize].node;
    fd_ref()[newfd as usize].offset = fd_ref()[oldfd as usize].offset;
    fd_ref()[newfd as usize].flags = fd_ref()[oldfd as usize].flags;
    fd_ref()[newfd as usize].refcount = 1;
    fd_ref()[oldfd as usize].refcount += 1;
    newfd
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_fcntl(fd: i32, cmd: i32, arg: u64) -> i64 {
    if fd < 0 || fd >= MAX_FDS as i32 || !fd_ref()[fd as usize].used {
        return -1;
    }
    match cmd {
        0 => krust_vfs_dup(fd) as i64,
        1 => fd_ref()[fd as usize].flags as i64,
        2 => {
            fd_ref()[fd as usize].flags = arg as u32;
            0
        }
        _ => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_ioctl(_fd: i32, _request: u32, _arg: u64) -> i64 {
    -1
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_poll(
    fds_ptr: *mut PollFdEntry, nfds: u64, timeout: i32,
) -> i64 {
    let nfds_val = nfds as usize;
    if fds_ptr.is_null() || nfds_val == 0 { return 0; }
    let mut count: i64 = 0;
    for i in 0..nfds_val {
        let fd = (*fds_ptr.add(i)).fd;
        if fd < 0 { continue; }
        (*fds_ptr.add(i)).revents = 0;
        if fd >= MAX_FDS as i32 || !fd_ref()[fd as usize].used {
            (*fds_ptr.add(i)).revents |= 0x20; // POLLNVAL
            count += 1;
            continue;
        }
        let node = fd_ref()[fd as usize].node;
        if !node.is_null() && (*node).type_ == 2 {
            (*fds_ptr.add(i)).revents |= 0x1; // POLLIN — devices always readable
            count += 1;
        } else {
            (*fds_ptr.add(i)).revents |= 0x1; // POLLIN for files
            count += 1;
        }
    }
    count
}

#[repr(C)]
pub struct PollFdEntry {
    fd: i32,
    events: u16,
    revents: u16,
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_select(
    nfds: i32,
    readfds: *mut u8,
    writefds: *mut u8,
    _exceptfds: *mut u8,
    _timeout: *mut u8,
) -> i64 {
    if nfds <= 0 || nfds > 1024 { return 0; }
    let bitmap_size = ((nfds as u32 + 7) / 8) as usize;
    let mut count: i64 = 0;

    if !readfds.is_null() {
        for byte_idx in 0..bitmap_size {
            let bits = *readfds.add(byte_idx);
            for bit in 0..8 {
                let fd = (byte_idx * 8 + bit) as i32;
                if fd >= nfds { break; }
                if (bits & (1 << bit)) != 0 {
                    if fd >= 0 && fd < MAX_FDS as i32 && fd_ref()[fd as usize].used {
                        count += 1;
                    } else {
                        *readfds.add(byte_idx) &= !(1 << bit);
                    }
                }
            }
        }
    }
    if !writefds.is_null() {
        for byte_idx in 0..bitmap_size {
            let bits = *writefds.add(byte_idx);
            for bit in 0..8 {
                let fd = (byte_idx * 8 + bit) as i32;
                if fd >= nfds { break; }
                if (bits & (1 << bit)) != 0 {
                    if fd >= 0 && fd < MAX_FDS as i32 && fd_ref()[fd as usize].used {
                        count += 1;
                    } else {
                        *writefds.add(byte_idx) &= !(1 << bit);
                    }
                }
            }
        }
    }
    count
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_pipe_create(fds: *mut i32) -> i32 {
    if fds.is_null() {
        return -1;
    }
    let mut pi: i32 = -1;
    for i in 0..MAX_PIPES {
        if !PIPES[i].used {
            pi = i as i32;
            break;
        }
    }
    if pi < 0 {
        return -1;
    }
    let p = &mut PIPES[pi as usize];
    krust_memset(p as *mut PipeBuffer as *mut u8, 0, core::mem::size_of::<PipeBuffer>());
    p.used = true;
    p.read_open = true;
    p.write_open = true;
    p.read_fd = NEXT_PIPE_FD;
    NEXT_PIPE_FD += 1;
    p.write_fd = NEXT_PIPE_FD;
    NEXT_PIPE_FD += 1;
    p.readers = 1;
    p.writers = 1;
    *fds = p.read_fd;
    *fds.add(1) = p.write_fd;
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_pipe_read(fd: i32, buf: *mut u8, size: u32) -> i32 {
    let p = pipe_from_fd(fd, true);
    if p.is_null() {
        return -1;
    }
    let mut total: u32 = 0;
    let mut retries = 0u32;
    while total < size {
        if (*p).head != (*p).tail {
            *buf.add(total as usize) = (*p).buf[(*p).tail as usize];
            (*p).tail = ((*p).tail + 1) % PIPE_BUF_SIZE as u32;
            total += 1;
            retries = 0;
        } else if !(*p).write_open {
            break;
        } else {
            if retries > 200 {
                break;
            }
            retries += 1;
            crate::scheduler::krust_sched_yield();
        }
    }
    total as i32
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_pipe_write(fd: i32, data: *const u8, size: u32) -> i32 {
    let p = pipe_from_fd(fd, false);
    if p.is_null() {
        return -1;
    }
    let mut total: u32 = 0;
    let mut retries = 0u32;
    while total < size {
        let next = ((*p).head + 1) % PIPE_BUF_SIZE as u32;
        if next != (*p).tail {
            (*p).buf[(*p).head as usize] = *data.add(total as usize);
            (*p).head = next;
            total += 1;
            retries = 0;
        } else if !(*p).read_open {
            break;
        } else {
            if retries > 200 {
                break;
            }
            retries += 1;
            crate::scheduler::krust_sched_yield();
        }
    }
    total as i32
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_pipe_close(fd: i32) {
    for i in 0..MAX_PIPES {
        if !PIPES[i].used {
            continue;
        }
        if PIPES[i].read_fd == fd || PIPES[i].write_fd == fd {
            if PIPES[i].read_fd == fd {
                PIPES[i].read_open = false;
                if PIPES[i].readers > 0 {
                    PIPES[i].readers -= 1;
                }
            }
            if PIPES[i].write_fd == fd {
                PIPES[i].write_open = false;
                if PIPES[i].writers > 0 {
                    PIPES[i].writers -= 1;
                }
            }
            if PIPES[i].readers == 0 && PIPES[i].writers == 0 {
                PIPES[i].used = false;
            }
            return;
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_root_node() -> *mut VNode {
    ROOT
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_node_count() -> u32 {
    count_nodes_recursive(ROOT)
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_chdir(path: *const u8) -> i32 {
    if path.is_null() || *path == 0 { return -1; }
    let node = krust_vfs_resolve(path);
    if node.is_null() || (*node).type_ != NODE_DIR { return -1; }
    if !check_permission(node, 1) { return -1; } // need execute on dir
    let current = crate::scheduler::krust_sched_current();
    if current.is_null() { return -1; }
    let mut i = 0;
    while i < 127 {
        let c = ptr::read_volatile(path.add(i));
        (*current).cwd[i] = c;
        if c == 0 { break; }
        i += 1;
    }
    (*current).cwd[127] = 0;
    0
}

#[no_mangle]
pub unsafe extern "C" fn krust_vfs_getcwd(buf: *mut u8, size: u32) -> i32 {
    if buf.is_null() || size == 0 { return -1; }
    let current = crate::scheduler::krust_sched_current();
    if current.is_null() { return -1; }
    let mut i: u32 = 0;
    while i < size - 1 && (*current).cwd[i as usize] != 0 {
        ptr::write_volatile(buf.add(i as usize), (*current).cwd[i as usize]);
        i += 1;
    }
    ptr::write_volatile(buf.add(i as usize), 0);
    i as i32
}
