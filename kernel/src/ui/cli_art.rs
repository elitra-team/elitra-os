// ═══════════════════════════════════════════════════════════════════
// CLI Art — Styled terminal output for Elitra OS shell
// ═══════════════════════════════════════════════════════════════════

use core::ptr;

extern "C" {
    fn krust_mm_info(total_kb: *mut u32, free_kb: *mut u32);
    fn krust_pittimer_get_ticks() -> u32;
    fn krust_sched_get_next_id() -> u32;
    fn krust_sched_get_task(id: u32) -> *mut crate::scheduler::Task;
    fn krust_strlen(s: *const u8) -> usize;
    fn krust_uitoa(num: u32, buf: *mut u8);
}

// ─── Low-level helpers ──────────────────────────────────────────

unsafe fn tw(s: &[u8]) {
    crate::terminal::krust_terminal_writestring(s.as_ptr());
}

unsafe fn tput(c: u8) {
    crate::terminal::krust_terminal_putchar(c);
}

unsafe fn set_color(c: u8) {
    crate::terminal::krust_terminal_set_color(c);
}

unsafe fn t_uint(n: u32) {
    let mut buf = [0u8; 16];
    krust_uitoa(n, buf.as_mut_ptr());
    tw(&buf);
}

// ─── Box-drawing constants (CP437) ─────────────────────────────

// Single line
const TL: u8 = 218;   // ┌
const TR: u8 = 191;   // ┐
const BL: u8 = 192;   // └
const BR: u8 = 217;   // ┘
const H:  u8 = 196;   // ─
const V:  u8 = 179;   // │
const LT: u8 = 195;   // ├
const RT: u8 = 180;   // ┤
const TT: u8 = 194;   // ┬
const BT: u8 = 193;   // ┴
const CR: u8 = 197;   // ┼

// Double line
const DTL: u8 = 201;  // ╔
const DTR: u8 = 187;  // ╗
const DBL: u8 = 200;  // ╚
const DBR: u8 = 188;  // ╝
const DH:  u8 = 205;  // ═
const DV:  u8 = 186;  // ║

// Blocks
const BLOCK: u8 = 219; // █

// ─── Box-drawing primitives ────────────────────────────────────

unsafe fn put_cp437(c: u8) {
    tput(c);
}

unsafe fn tw_cp437_line(chars: &[u8]) {
    for &c in chars { put_cp437(c); }
}

unsafe fn tw_section_header(label: &[u8], width: u32) {
    tw(b"  \0");
    for _ in 0..3 { put_cp437(DH); }
    tput(b' ');
    for &c in label { tput(c); }
    tput(b' ');
    let used = 3 + 1 + label.len() as u32 + 1;
    for _ in used..width { put_cp437(DH); }
    tput(b'\n');
}

unsafe fn box_line_single(width: u32) {
    put_cp437(TL);
    for _ in 0..width { put_cp437(H); }
    put_cp437(TR);
}

unsafe fn box_bottom_single(width: u32) {
    put_cp437(BL);
    for _ in 0..width { put_cp437(H); }
    put_cp437(BR);
}

unsafe fn box_line_double(width: u32) {
    put_cp437(DTL);
    for _ in 0..width { put_cp437(DH); }
    put_cp437(DTR);
}

unsafe fn box_bottom_double(width: u32) {
    put_cp437(DBL);
    for _ in 0..width { put_cp437(DH); }
    put_cp437(DBR);
}

unsafe fn box_row_single(text: &[u8], width: u32, color: u8) {
    set_color(color);
    put_cp437(V);
    set_color(color);
    tput(b' ');
    let len = text.len();
    let pad = if len + 2 > width as usize { 0 } else { width as usize - len - 2 };
    tw(text);
    for _ in 0..pad { tput(b' '); }
    tput(b' ');
    put_cp437(V);
}

unsafe fn box_row_double(text: &[u8], width: u32, color: u8) {
    set_color(color);
    put_cp437(DV);
    tput(b' ');
    let len = text.len();
    let pad = if len + 2 > width as usize { 0 } else { width as usize - len - 2 };
    tw(text);
    for _ in 0..pad { tput(b' '); }
    tput(b' ');
    put_cp437(DV);
}

unsafe fn box_row_empty(width: u32) {
    put_cp437(V);
    for _ in 0..width { tput(b' '); }
    put_cp437(V);
}

unsafe fn box_row_empty_double(width: u32) {
    put_cp437(DV);
    for _ in 0..width { tput(b' '); }
    put_cp437(DV);
}

// ─── Memory usage bar ──────────────────────────────────────────

unsafe fn memory_bar(used: u32, total: u32, bar_width: u32) {
    let fill = if total > 0 {
        (used as u64 * bar_width as u64 / total as u64) as u32
    } else {
        0
    };
    tput(b'[');
    for i in 0..bar_width {
        if i < fill {
            put_cp437(BLOCK);
        } else {
            tput(b' ');
        }
    }
    tput(b']');
}

// ─── Uptime formatter ──────────────────────────────────────────

unsafe fn format_uptime(ticks: u32) {
    let secs = ticks / 100;
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h < 10 { tput(b'0'); }
    t_uint(h);
    tput(b':');
    if m < 10 { tput(b'0'); }
    t_uint(m);
    tput(b':');
    if s < 10 { tput(b'0'); }
    t_uint(s);
}

// ═══════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════

/// Print the styled splash screen with framed info box
#[no_mangle]
pub unsafe fn cli_art_print_splash() {
    // Banner art (Elitra in block characters)
    let bw: u32 = 62;

    // Top border
    set_color(0x0B); // cyan
    box_line_double(bw);
    tput(b'\n');

    // Empty row
    box_row_empty_double(bw);
    tput(b'\n');

    // E
    set_color(0x0B);
    put_cp437(DV);
    tput(b' ');
    set_color(0x0F); // bright white
    tw_cp437_line(&[0,0,0,0,0, 219,219,223,223,219, 32, 219,223,223, 32, 223,219,223, 32, 219,223,223, 32, 219,223,219, 32, 219,220,223,220,219]);
    set_color(0x0B);
    put_cp437(DV);
    tput(b'\n');

    // L
    set_color(0x0B);
    put_cp437(DV);
    tput(b' ');
    set_color(0x0F);
    tw_cp437_line(&[0,0,0,0,0, 219,220,220,223, 32,32, 219,220,220, 32,32, 219, 32,32,32,32, 219, 32,32, 219,220,220, 32, 219,223,220, 32, 219,176,223,176,219]);
    set_color(0x0B);
    put_cp437(DV);
    tput(b'\n');

    // I
    set_color(0x0B);
    put_cp437(DV);
    tput(b' ');
    set_color(0x0F);
    tw_cp437_line(&[0,0,0,0,0, 219,223,223, 0,0,0, 223,223,223, 0,0, 223, 0,0,0,0, 223, 0,0, 223,223,223, 0, 223, 0,0, 223, 0,0,0, 223, 0,0, 79,83,0,0,118,48,46,49,46,48]);
    set_color(0x0B);
    put_cp437(DV);
    tput(b'\n');

    // Empty row
    box_row_empty_double(bw);
    tput(b'\n');

    // Separator
    set_color(0x0B);
    put_cp437(DV);
    tput(b' ');
    set_color(0x08); // dark gray
    for _ in 0..bw - 2 { put_cp437(H); }
    set_color(0x0B);
    tw(b" \0");
    put_cp437(DV);
    tput(b'\n');

    // System info header
    set_color(0x0B);
    put_cp437(DV);
    tput(b' ');
    set_color(0x0E); // yellow
    tw(b"\0");
    // center "System Information"
    let hdr = b"System Information\0";
    let hdr_len = krust_strlen(hdr.as_ptr());
    let pad_left = (bw as usize - hdr_len) / 2;
    let pad_right = bw as usize - hdr_len - pad_left;
    set_color(0x0B);
    for _ in 1..pad_left { tput(b' '); }
    set_color(0x0E);
    tw(hdr);
    set_color(0x0B);
    for _ in 0..pad_right { tput(b' '); }
    tw(b" \0");
    put_cp437(DV);
    tput(b'\n');

    // Separator
    set_color(0x0B);
    put_cp437(DV);
    tput(b' ');
    set_color(0x08);
    for _ in 0..bw - 2 { put_cp437(H); }
    set_color(0x0B);
    tw(b" \0");
    put_cp437(DV);
    tput(b'\n');

    // ── OS row ──
    set_color(0x0B);
    put_cp437(DV);
    tput(b' ');
    set_color(0x0A); // green label
    tw(b"  OS\0");
    set_color(0x07);
    let mut ver_str = [0u8; 49];
    let mut vpos = 0;
    for &b in crate::KERNEL_NAME.as_bytes() { if vpos < ver_str.len() - 1 { ver_str[vpos] = b; vpos += 1; } }
    if vpos < ver_str.len() - 1 { ver_str[vpos] = b' '; vpos += 1; }
    if vpos < ver_str.len() - 1 { ver_str[vpos] = b'v'; vpos += 1; }
    for &b in crate::KERNEL_VERSION.as_bytes() { if vpos < ver_str.len() - 1 { ver_str[vpos] = b; vpos += 1; } }
    while vpos < 48 { ver_str[vpos] = b' '; vpos += 1; }
    ver_str[vpos] = 0;
    tw(&ver_str[..vpos]);
    set_color(0x0B);
    tw(b" \0");
    put_cp437(DV);
    tput(b'\n');

    // ── Arch row ──
    set_color(0x0B);
    put_cp437(DV);
    tput(b' ');
    set_color(0x0A);
    tw(b"  Arch\0");
    set_color(0x07);
    tw(b"    x86_64 long mode                             \0");
    set_color(0x0B);
    tw(b" \0");
    put_cp437(DV);
    tput(b'\n');

    // ── Memory row ──
    let mut total_kb: u32 = 0;
    let mut free_kb: u32 = 0;
    krust_mm_info(&mut total_kb as *mut _, &mut free_kb as *mut _);
    let used_kb = total_kb - free_kb;
    let total_mb = total_kb / 1024;
    let used_mb = used_kb / 1024;

    set_color(0x0B);
    put_cp437(DV);
    tput(b' ');
    set_color(0x0A);
    tw(b"  Mem \0");
    set_color(0x07);
    memory_bar(used_kb, total_kb, 16);
    tput(b' ');
    t_uint(used_mb);
    tw(b"/\0");
    t_uint(total_mb);
    tw(b" MB                \0");
    set_color(0x0B);
    tw(b" \0");
    put_cp437(DV);
    tput(b'\n');

    // ── CPU row ──
    set_color(0x0B);
    put_cp437(DV);
    tput(b' ');
    set_color(0x0A);
    tw(b"  CPU\0");
    set_color(0x07);
    tw(b"     QEMU QEMU CPU @ 3.0GHz                      \0");
    set_color(0x0B);
    tw(b" \0");
    put_cp437(DV);
    tput(b'\n');

    // ── Uptime row ──
    let ticks = krust_pittimer_get_ticks();
    set_color(0x0B);
    put_cp437(DV);
    tput(b' ');
    set_color(0x0A);
    tw(b"  Up\0");
    set_color(0x07);
    tw(b"      \0");
    format_uptime(ticks);
    tw(b"                                        \0");
    set_color(0x0B);
    tw(b" \0");
    put_cp437(DV);
    tput(b'\n');

    // ── Tasks row ──
    let next_id = krust_sched_get_next_id();
    let mut task_count: u32 = 0;
    for pid in 0..next_id {
        let task = krust_sched_get_task(pid);
        if !task.is_null() {
            task_count += 1;
        }
    }
    set_color(0x0B);
    put_cp437(DV);
    tput(b' ');
    set_color(0x0A);
    tw(b"  Task\0");
    set_color(0x07);
    tw(b"    \0");
    t_uint(task_count);
    tw(b" active processes                              \0");
    set_color(0x0B);
    tw(b" \0");
    put_cp437(DV);
    tput(b'\n');

    // ── Terminal row ──
    set_color(0x0B);
    put_cp437(DV);
    tput(b' ');
    set_color(0x0A);
    tw(b"  Term\0");
    set_color(0x07);
    tw(b"    80x25, 4 virtual consoles                   \0");
    set_color(0x0B);
    tw(b" \0");
    put_cp437(DV);
    tput(b'\n');

    // Empty row
    box_row_empty_double(bw);
    tput(b'\n');

    // Separator
    set_color(0x0B);
    put_cp437(DV);
    tput(b' ');
    set_color(0x08);
    for _ in 0..bw - 2 { put_cp437(H); }
    set_color(0x0B);
    tw(b" \0");
    put_cp437(DV);
    tput(b'\n');

    // Help hint
    set_color(0x0B);
    put_cp437(DV);
    tput(b' ');
    let hint = b"Type 'help' for available commands.\0";
    let hint_len = krust_strlen(hint.as_ptr());
    let hp = (bw as usize - hint_len) / 2;
    let hpr = bw as usize - hint_len - hp;
    for _ in 1..hp { tput(b' '); }
    set_color(0x0F);
    tw(hint);
    set_color(0x0B);
    for _ in 0..hpr { tput(b' '); }
    tw(b" \0");
    put_cp437(DV);
    tput(b'\n');

    // Bottom border
    set_color(0x0B);
    box_bottom_double(bw);
    set_color(0x07);
    tput(b'\n');
    tput(b'\n');
}

/// Print a styled prompt: user@elitra:~/path$
#[no_mangle]
pub unsafe fn cli_art_print_prompt(cwd: *const u8) {
    set_color(0x0A); // green
    tw(b"user@elitra\0");
    set_color(0x0F); // white
    tput(b':');
    set_color(0x0B); // cyan
    if !cwd.is_null() && ptr::read_volatile(cwd) != 0 {
        let cwd_len = krust_strlen(cwd);
        tw(core::slice::from_raw_parts(cwd, cwd_len));
    } else {
        tput(b'/');
    }
    set_color(0x0E); // yellow
    tput(b'$');
    tput(b' ');
    set_color(0x07);
}

/// Print the login screen and prompt for credentials.
/// Returns true if login succeeded.
#[no_mangle]
pub unsafe fn cli_art_login() -> bool {
    const BW: u32 = 56;

    // Clear screen
    crate::terminal::krust_terminal_clear();

    // Top spacing
    for _ in 0..4 { tput(b'\n'); }

    // Logo (compact)
    set_color(0x0B);
    for _ in 0..28 { tput(b' '); }
    box_line_double(BW - 30);
    tput(b'\n');

    for _ in 0..28 { tput(b' '); }
    box_row_empty_double(BW - 30);
    tput(b'\n');

    for _ in 0..28 { tput(b' '); }
    put_cp437(DV);
    tput(b' ');
    set_color(0x0B);
    tw(b"    \0");
    set_color(0x0F);
    tw_cp437_line(&[219,219,223,223,219, 32, 219,223,223, 32, 223,219,223, 32, 219,223,223, 32, 219,223,219, 32, 219,220,223,220,219]);
    set_color(0x0B);
    tw(b"\0");
    put_cp437(DV);
    tput(b'\n');

    for _ in 0..28 { tput(b' '); }
    put_cp437(DV);
    tput(b' ');
    set_color(0x0B);
    tw(b"    \0");
    set_color(0x0F);
    tw_cp437_line(&[219,220,220,223, 32,32, 219,220,220, 32,32, 219, 32,32,32,32, 219, 32,32, 219,220,220, 32, 219,223,220, 32, 219,176,223,176,219]);
    set_color(0x0B);
    tw(b"\0");
    put_cp437(DV);
    tput(b'\n');

    for _ in 0..28 { tput(b' '); }
    put_cp437(DV);
    tput(b' ');
    set_color(0x0B);
    tw(b"    \0");
    set_color(0x0F);
    tw_cp437_line(&[219,223,223, 0,0,0, 223,223,223, 0,0, 223, 0,0,0,0, 223, 0,0, 223,223,223, 0, 223, 0,0, 223, 0,0,0, 223, 0,0]);
    set_color(0x0B);
    tw(b"\0");
    put_cp437(DV);
    tput(b'\n');

    for _ in 0..28 { tput(b' '); }
    box_row_empty_double(BW - 30);
    tput(b'\n');

    for _ in 0..28 { tput(b' '); }
    box_bottom_double(BW - 30);
    tput(b'\n');

    // Blank line
    tput(b'\n');

    // Login box
    let login_w: u32 = 46;
    for _ in 0..19 { tput(b' '); }
    set_color(0x0B);
    box_line_double(login_w);
    tput(b'\n');

    for _ in 0..19 { tput(b' '); }
    box_row_empty_double(login_w);
    tput(b'\n');

    // Title
    for _ in 0..19 { tput(b' '); }
    set_color(0x0B);
    put_cp437(DV);
    tput(b' ');
    let title = b"  Elitra OS - Login  \0";
    let title_len = krust_strlen(title.as_ptr()) - 1; // minus null
    let pad = (login_w as usize - title_len) / 2;
    for _ in 1..pad { tput(b' '); }
    set_color(0x0E);
    tw(title);
    set_color(0x0B);
    for _ in 0..(login_w as usize - title_len - pad - 1) { tput(b' '); }
    tw(b" \0");
    put_cp437(DV);
    tput(b'\n');

    // Separator
    for _ in 0..19 { tput(b' '); }
    put_cp437(DTL);
    for _ in 0..login_w { put_cp437(DH); }
    put_cp437(DTR);
    tput(b'\n');

    // Username row
    for _ in 0..19 { tput(b' '); }
    set_color(0x0B);
    put_cp437(DV);
    set_color(0x0A);
    tw(b"  Username: \0");
    set_color(0x0F);
    tw(b"                     \0");
    set_color(0x0B);
    put_cp437(DV);
    tput(b'\n');

    // Password row
    for _ in 0..19 { tput(b' '); }
    set_color(0x0B);
    put_cp437(DV);
    set_color(0x0A);
    tw(b"  Password: \0");
    set_color(0x0F);
    tw(b"                     \0");
    set_color(0x0B);
    put_cp437(DV);
    tput(b'\n');

    // Bottom
    for _ in 0..19 { tput(b' '); }
    box_row_empty_double(login_w);
    tput(b'\n');

    for _ in 0..19 { tput(b' '); }
    box_bottom_double(login_w);
    tput(b'\n');

    tput(b'\n');

    // ── Actual login prompt (outside the box) ──
    for _ in 0..19 { tput(b' '); }
    set_color(0x08);
    tw(b"Any credentials accepted.\0");
    tput(b'\n');
    for _ in 0..19 { tput(b' '); }
    tw(b"\0");

    // Position cursor for username input
    for _ in 0..19 { tput(b' '); }
    set_color(0x0A);
    tw(b"Username: \0");
    set_color(0x07);
    let mut username_buf = [0u8; 64];
    crate::terminal::krust_terminal_readline(username_buf.as_mut_ptr(), 64);

    // Position cursor for password input
    for _ in 0..19 { tput(b' '); }
    set_color(0x0A);
    tw(b"Password: \0");
    set_color(0x07);
    let mut password_buf = [0u8; 64];
    crate::terminal::krust_terminal_readline(password_buf.as_mut_ptr(), 64);

    // Any non-empty credentials accepted
    let has_user = username_buf[0] != 0;

    // Clear screen again and show welcome
    crate::terminal::krust_terminal_clear();
    for _ in 0..2 { tput(b'\n'); }

    if has_user {
        for _ in 0..19 { tput(b' '); }
        set_color(0x0A);
        tw(b"Welcome back, \0");
        set_color(0x0F);
        let uname_len = krust_strlen(username_buf.as_ptr());
        tw(core::slice::from_raw_parts(username_buf.as_ptr(), uname_len));
        set_color(0x0A);
        tw(b"!\0");
        tput(b'\n');
    } else {
        for _ in 0..19 { tput(b' '); }
        set_color(0x0A);
        tw(b"Welcome to Elitra OS!\0");
    }

    tput(b'\n');
    tput(b'\n');

    true
}

/// Print styled help with categorized commands
#[no_mangle]
pub unsafe fn cli_art_print_help() {
    // System commands
    set_color(0x0E); // yellow
    tw_section_header(b"System", 53);
    help_entry(b"  ver\0", b"       Show kernel version\0");
    help_entry(b"  cpu\0", b"       Show CPU information\0");
    help_entry(b"  mem\0", b"       Show memory information\0");
    help_entry(b"  upt\0", b"       Show system uptime\0");
    help_entry(b"  date\0", b"      Show current date/time\0");
    help_entry(b"  rst\0", b"       Reboot the system\0");
    help_entry(b"  off\0", b"       Shutdown the system\0");
    tput(b'\n');

    // Process commands
    set_color(0x0E);
    tw_section_header(b"Process", 53);
    help_entry(b"  ps\0", b"        List processes\0");
    help_entry(b"  top\0", b"       Process monitor snapshot\0");
    help_entry(b"  kill <pid>\0", b" Kill a process\0");
    help_entry(b"  newt\0", b"      Create test tasks\0");
    help_entry(b"  jobs\0", b"       Show task info\0");
    tput(b'\n');

    // Filesystem commands
    set_color(0x0E);
    tw_section_header(b"Files", 53);
    help_entry(b"  list [path]\0", b"  List directory\0");
    help_entry(b"  tree [path]\0", b"  Visual directory tree\0");
    help_entry(b"  dump <file>\0", b" Print file contents\0");
    help_entry(b"  create <path>\0", b" Create empty file\0");
    help_entry(b"  del <path>\0", b"  Remove file\0");
    help_entry(b"  md <path>\0", b"   Create directory\0");
    help_entry(b"  put <p> <t>\0", b" Write text to file\0");
    help_entry(b"  cd <path>\0", b"  Change directory\0");
    help_entry(b"  exec <file>\0", b" Load and run ELF\0");
    tput(b'\n');

    // System info
    set_color(0x0E);
    tw_section_header(b"Info", 53);
    help_entry(b"  neo\0", b"       System info (neofetch)\0");
    help_entry(b"  fs\0", b"        Show VFS info\0");
    help_entry(b"  df\0", b"        Show disk usage\0");
    help_entry(b"  ata\0", b"       Show ATA drive info\0");
    help_entry(b"  mnt\0", b"       List mounted filesystems\0");
    help_entry(b"  unm <path>\0", b" Unmount filesystem\0");
    help_entry(b"  sync\0", b"      Flush disk writes\0");
    tput(b'\n');

    // Other
    set_color(0x0E);
    tw_section_header(b"Misc", 53);
    help_entry(b"  clr\0", b"       Clear the screen\0");
    help_entry(b"  say <text>\0", b" Print text\0");
    help_entry(b"  cowsay <t>\0", b" ASCII art cow\0");
    help_entry(b"  mall\0", b"      Test heap allocator\0");
    help_entry(b"  pt\0", b"        Test paging\0");
    set_color(0x07);
}

unsafe fn help_entry(cmd: &[u8], desc: &[u8]) {
    set_color(0x0F); // bright white for command
    tw(cmd);
    set_color(0x07); // gray for description
    tw(desc);
    tput(b'\n');
}

/// Print a styled version box
#[no_mangle]
pub unsafe fn cli_art_print_version_box() {
    let bw: u32 = 35;
    set_color(0x0B);
    box_line_double(bw);
    tput(b'\n');

    set_color(0x0B);
    put_cp437(DV);
    tput(b' ');
    set_color(0x0F);
    let mut title_buf = [0u8; 48];
    let mut tpos = 0;
    for &b in crate::KERNEL_NAME.as_bytes() { if tpos < title_buf.len() { title_buf[tpos] = b; tpos += 1; } }
    if tpos < title_buf.len() { title_buf[tpos] = b' '; tpos += 1; }
    if tpos < title_buf.len() { title_buf[tpos] = b'v'; tpos += 1; }
    for &b in crate::KERNEL_VERSION.as_bytes() { if tpos < title_buf.len() { title_buf[tpos] = b; tpos += 1; } }
    title_buf[tpos] = 0;
    let title = &title_buf[..tpos + 1];
    let tl = title.len();
    let tl = krust_strlen(title.as_ptr());
    let pl = (bw as usize - tl) / 2;
    let pr = bw as usize - tl - pl;
    for _ in 1..pl { tput(b' '); }
    tw(title);
    for _ in 0..pr { tput(b' '); }
    set_color(0x0B);
    tw(b" \0");
    put_cp437(DV);
    tput(b'\n');

    set_color(0x0B);
    put_cp437(DV);
    tput(b' ');
    set_color(0x07);
    let sub = b"x86_64 Hobby Kernel\0";
    let sl = krust_strlen(sub.as_ptr());
    let spl = (bw as usize - sl) / 2;
    let spr = bw as usize - sl - spl;
    for _ in 1..spl { tput(b' '); }
    tw(sub);
    for _ in 0..spr { tput(b' '); }
    set_color(0x0B);
    tw(b" \0");
    put_cp437(DV);
    tput(b'\n');

    set_color(0x0B);
    box_bottom_double(bw);
    set_color(0x07);
    tput(b'\n');
}

/// Print a colored memory info display
#[no_mangle]
pub unsafe fn cli_art_print_meminfo() {
    let mut total_kb: u32 = 0;
    let mut free_kb: u32 = 0;
    krust_mm_info(&mut total_kb as *mut _, &mut free_kb as *mut _);
    let used_kb = total_kb - free_kb;
    let total_mb = total_kb / 1024;
    let used_mb = used_kb / 1024;

    let bw: u32 = 48;
    set_color(0x0B);
    box_line_single(bw);
    tput(b'\n');

    // Title
    set_color(0x0B);
    put_cp437(V);
    tput(b' ');
    set_color(0x0E);
    let title = b"Memory Information\0";
    let tl = krust_strlen(title.as_ptr());
    let pl = (bw as usize - tl) / 2;
    let pr = bw as usize - tl - pl;
    for _ in 1..pl { tput(b' '); }
    tw(title);
    for _ in 0..pr { tput(b' '); }
    set_color(0x0B);
    tput(b' ');
    put_cp437(V);
    tput(b'\n');

    // Separator
    set_color(0x0B);
    put_cp437(LT);
    for _ in 0..bw - 2 { put_cp437(H); }
    put_cp437(RT);
    tput(b'\n');

    // Memory bar row
    set_color(0x0B);
    put_cp437(V);
    tput(b' ');
    set_color(0x07);
    memory_bar(used_kb, total_kb, 20);
    tput(b' ');
    let pct = if total_kb > 0 { used_kb * 100 / total_kb } else { 0 };
    t_uint(pct);
    tw(b"%  \0");
    set_color(0x07);
    tw(b"used of \0");
    t_uint(total_mb);
    tw(b" MB \0");
    set_color(0x0B);
    tput(b' ');
    put_cp437(V);
    tput(b'\n');

    // Separator
    set_color(0x0B);
    put_cp437(LT);
    for _ in 0..bw - 2 { put_cp437(H); }
    put_cp437(RT);
    tput(b'\n');

    // Details
    set_color(0x0B);
    put_cp437(V);
    tput(b' ');
    set_color(0x0A);
    tw(b"  Total:  \0");
    set_color(0x0F);
    t_uint(total_kb);
    tw(b" KB\0");
    set_color(0x0B);
    for _ in 0..(bw as usize - 24) { tput(b' '); }
    put_cp437(V);
    tput(b'\n');

    set_color(0x0B);
    put_cp437(V);
    tput(b' ');
    set_color(0x0A);
    tw(b"  Used:   \0");
    set_color(0x0C); // red for used
    t_uint(used_kb);
    tw(b" KB\0");
    set_color(0x0B);
    for _ in 0..(bw as usize - 25) { tput(b' '); }
    put_cp437(V);
    tput(b'\n');

    set_color(0x0B);
    put_cp437(V);
    tput(b' ');
    set_color(0x0A);
    tw(b"  Free:   \0");
    set_color(0x0A); // green for free
    t_uint(free_kb);
    tw(b" KB\0");
    set_color(0x0B);
    for _ in 0..(bw as usize - 25) { tput(b' '); }
    put_cp437(V);
    tput(b'\n');

    // Bottom
    set_color(0x0B);
    box_bottom_single(bw);
    set_color(0x07);
    tput(b'\n');
}

/// Print a styled process list with table borders
#[no_mangle]
pub unsafe fn cli_art_print_ps() {
    let next_id = krust_sched_get_next_id();

    let bw: u32 = 56;

    // Top border
    set_color(0x0B);
    put_cp437(TL);
    for _ in 0..5 { put_cp437(H); }
    put_cp437(TT);
    for _ in 0..6 { put_cp437(H); }
    put_cp437(TT);
    for _ in 0..9 { put_cp437(H); }
    put_cp437(TT);
    for _ in 0..9 { put_cp437(H); }
    put_cp437(TT);
    for _ in 0..23 { put_cp437(H); }
    put_cp437(TR);
    tput(b'\n');

    // Header
    set_color(0x0B);
    put_cp437(V);
    set_color(0x0F);
    tw(b" PID \0");
    set_color(0x0B);
    put_cp437(V);
    set_color(0x0F);
    tw(b" PPID \0");
    set_color(0x0B);
    put_cp437(V);
    set_color(0x0F);
    tw(b"  STATE  \0");
    set_color(0x0B);
    put_cp437(V);
    set_color(0x0F);
    tw(b"  PRIO  \0");
    set_color(0x0B);
    put_cp437(V);
    set_color(0x0F);
    tw(b"  COMMAND/PATH        \0");
    set_color(0x0B);
    put_cp437(V);
    tput(b'\n');

    // Separator
    set_color(0x0B);
    put_cp437(LT);
    for _ in 0..5 { put_cp437(H); }
    put_cp437(CR);
    for _ in 0..6 { put_cp437(H); }
    put_cp437(CR);
    for _ in 0..9 { put_cp437(H); }
    put_cp437(CR);
    for _ in 0..9 { put_cp437(H); }
    put_cp437(CR);
    for _ in 0..23 { put_cp437(H); }
    put_cp437(RT);
    tput(b'\n');

    for pid in 0..next_id {
        let task = krust_sched_get_task(pid);
        if task.is_null() { continue; }

        // PID
        set_color(0x0B);
        put_cp437(V);
        set_color(0x0F);
        let pid_str = (*task).id;
        let mut pid_buf = [0u8; 8];
        krust_uitoa(pid_str, pid_buf.as_mut_ptr());
        let pid_len = krust_strlen(pid_buf.as_ptr());
        let pad = if pid_len >= 5 { 0 } else { 5 - pid_len };
        for _ in 0..pad { tput(b' '); }
        tw(&pid_buf);

        // PPID
        set_color(0x0B);
        put_cp437(V);
        set_color(0x07);
        let mut ppid_buf = [0u8; 8];
        krust_uitoa((*task).ppid, ppid_buf.as_mut_ptr());
        let ppid_len = krust_strlen(ppid_buf.as_ptr());
        let pad = if ppid_len >= 6 { 0 } else { 6 - ppid_len };
        for _ in 0..pad { tput(b' '); }
        tw(&ppid_buf);

        // STATE with color
        set_color(0x0B);
        put_cp437(V);
        let (state_str, state_color) = match (*task).state {
            crate::scheduler::TaskState::RUNNING => (b"RUN   \0", 0x0Au8),
            crate::scheduler::TaskState::READY => (b"READY \0", 0x0Fu8),
            crate::scheduler::TaskState::BLOCKED => (b"BLOCK \0", 0x0Eu8),
            crate::scheduler::TaskState::WAITING => (b"WAIT  \0", 0x0Du8),
            crate::scheduler::TaskState::EXITED => (b"EXIT  \0", 0x0Cu8),
        };
        set_color(state_color);
        tw(b" \0");
        tw(state_str);
        tput(b' ');

        // Priority
        set_color(0x0B);
        put_cp437(V);
        set_color(0x07);
        let mut prio_buf = [0u8; 8];
        krust_uitoa((*task).priority, prio_buf.as_mut_ptr());
        let prio_len = krust_strlen(prio_buf.as_ptr());
        let pad = if prio_len >= 9 { 0 } else { 9 - prio_len };
        let half = pad / 2;
        let half2 = pad - half;
        for _ in 0..half + 1 { tput(b' '); }
        tw(&prio_buf);
        for _ in 0..half2 { tput(b' '); }

        // Command/cwd
        set_color(0x0B);
        put_cp437(V);
        set_color(0x07);
        if (*task).cwd[0] != 0 {
            tput(b' ');
            let cwd_len = krust_strlen((*task).cwd.as_ptr());
            tw(core::slice::from_raw_parts((*task).cwd.as_ptr(), cwd_len));
        } else {
            tw(b" shell              \0");
        }
        tput(b' ');
        set_color(0x0B);
        put_cp437(V);
        tput(b'\n');
    }

    // Bottom border
    set_color(0x0B);
    put_cp437(BL);
    for _ in 0..5 { put_cp437(H); }
    put_cp437(BT);
    for _ in 0..6 { put_cp437(H); }
    put_cp437(BT);
    for _ in 0..9 { put_cp437(H); }
    put_cp437(BT);
    for _ in 0..9 { put_cp437(H); }
    put_cp437(BT);
    for _ in 0..23 { put_cp437(H); }
    put_cp437(BR);
    tput(b'\n');

    set_color(0x07);
}

/// Print styled disk usage info
#[no_mangle]
pub unsafe fn cli_art_print_df() {
    let mut total_kb: u32 = 0;
    let mut free_kb: u32 = 0;
    krust_mm_info(&mut total_kb as *mut _, &mut free_kb as *mut _);

    let bw: u32 = 58;

    // Top border
    set_color(0x0B);
    put_cp437(TL);
    for _ in 0..16 { put_cp437(H); }
    put_cp437(TT);
    for _ in 0..10 { put_cp437(H); }
    put_cp437(TT);
    for _ in 0..10 { put_cp437(H); }
    put_cp437(TT);
    for _ in 0..10 { put_cp437(H); }
    put_cp437(TT);
    for _ in 0..6 { put_cp437(H); }
    put_cp437(TR);
    tput(b'\n');

    // Header
    set_color(0x0B);
    put_cp437(V);
    set_color(0x0F);
    tw(b" Filesystem     \0");
    set_color(0x0B);
    put_cp437(V);
    set_color(0x0F);
    tw(b"   Size   \0");
    set_color(0x0B);
    put_cp437(V);
    set_color(0x0F);
    tw(b"   Used   \0");
    set_color(0x0B);
    put_cp437(V);
    set_color(0x0F);
    tw(b"   Avail  \0");
    set_color(0x0B);
    put_cp437(V);
    set_color(0x0F);
    tw(b" Use% \0");
    set_color(0x0B);
    put_cp437(V);
    tput(b'\n');

    // Separator
    set_color(0x0B);
    put_cp437(LT);
    for _ in 0..16 { put_cp437(H); }
    put_cp437(CR);
    for _ in 0..10 { put_cp437(H); }
    put_cp437(CR);
    for _ in 0..10 { put_cp437(H); }
    put_cp437(CR);
    for _ in 0..10 { put_cp437(H); }
    put_cp437(CR);
    for _ in 0..6 { put_cp437(H); }
    put_cp437(RT);
    tput(b'\n');

    // Data row
    set_color(0x0B);
    put_cp437(V);
    set_color(0x0A);
    tw(b" /dev/ram0      \0");
    set_color(0x0B);
    put_cp437(V);
    set_color(0x07);
    tw(b" \0");
    t_uint(total_kb / 1024);
    tw(b" MB \0");
    set_color(0x0B);
    put_cp437(V);
    set_color(0x0C);
    tw(b" \0");
    t_uint((total_kb - free_kb) / 1024);
    tw(b" MB \0");
    set_color(0x0B);
    put_cp437(V);
    set_color(0x0A);
    tw(b" \0");
    t_uint(free_kb / 1024);
    tw(b" MB \0");
    set_color(0x0B);
    put_cp437(V);
    set_color(0x0F);
    tw(b" \0");
    if total_kb > 0 {
        t_uint((total_kb - free_kb) * 100 / total_kb);
    } else {
        tw(b"0");
    }
    tw(b"% \0");
    set_color(0x0B);
    put_cp437(V);
    tput(b'\n');

    // Bottom
    set_color(0x0B);
    put_cp437(BL);
    for _ in 0..16 { put_cp437(H); }
    put_cp437(BT);
    for _ in 0..10 { put_cp437(H); }
    put_cp437(BT);
    for _ in 0..10 { put_cp437(H); }
    put_cp437(BT);
    for _ in 0..10 { put_cp437(H); }
    put_cp437(BT);
    for _ in 0..6 { put_cp437(H); }
    put_cp437(BR);
    tput(b'\n');
    set_color(0x07);
}

/// Print styled uptime display
#[no_mangle]
pub unsafe fn cli_art_print_uptime() {
    let ticks = krust_pittimer_get_ticks();

    set_color(0x0A);
    tw(b"  Uptime: \0");
    set_color(0x0F);
    format_uptime(ticks);
    set_color(0x07);
    tput(b'\n');
}

/// Print styled date/time display
#[no_mangle]
pub unsafe fn cli_art_print_date() {
    let info = crate::cmos_rtc::krust_cmos_read_time();
    set_color(0x0A);
    tw(b"  Date:   \0");
    set_color(0x0F);
    t_uint(info.year as u32);
    tput(b'-');
    if info.month < 10 { tput(b'0'); }
    t_uint(info.month as u32);
    tput(b'-');
    if info.day < 10 { tput(b'0'); }
    t_uint(info.day as u32);
    tput(b'\n');

    set_color(0x0A);
    tw(b"  Time:   \0");
    set_color(0x0F);
    if info.hour < 10 { tput(b'0'); }
    t_uint(info.hour as u32);
    tput(b':');
    if info.minute < 10 { tput(b'0'); }
    t_uint(info.minute as u32);
    tput(b':');
    if info.second < 10 { tput(b'0'); }
    t_uint(info.second as u32);
    set_color(0x07);
    tput(b'\n');
}

/// Print a progress bar helper
unsafe fn progress_bar(value: u32, max: u32, width: u32, fg: u8, bg: u8) {
    let fill = if max > 0 { (value as u64 * width as u64 / max as u64) as u32 } else { 0 };
    set_color(fg);
    tput(b'[');
    for i in 0..width {
        if i < fill {
            put_cp437(BLOCK);
        } else {
            set_color(bg);
            tput(b'-');
            set_color(fg);
        }
    }
    tput(b']');
}

/// Neofetch-style system info display
#[no_mangle]
pub unsafe fn cli_art_print_neofetch() {
    use crate::cpuid;

    let mut total_kb: u32 = 0;
    let mut free_kb: u32 = 0;
    krust_mm_info(&mut total_kb as *mut _, &mut free_kb as *mut _);
    let used_kb = total_kb - free_kb;
    let total_mb = total_kb / 1024;
    let used_mb = used_kb / 1024;
    let ticks = krust_pittimer_get_ticks();
    let next_id = krust_sched_get_next_id();
    let mut task_count: u32 = 0;
    for pid in 0..next_id {
        if !krust_sched_get_task(pid).is_null() { task_count += 1; }
    }
    let block_count = crate::block::device_count() as u32;

    // ASCII art logo - left side
    let logo: [&[u8]; 10] = [
        b"        _____ _       _   _____  " as &[u8],
        b"       / ____| |     | | |  __ \\ " as &[u8],
        b"      | (___ | | ___ | |_| |__) |" as &[u8],
        b"       \\___ \\| |/ _ \\| __|  ___/ " as &[u8],
        b"       ____) | | (_) | |_| |     " as &[u8],
        b"      |_____/|_|\\___/ \\__|_|     " as &[u8],
        b"       __  ___ __  __ _ __       " as &[u8],
        b"      / _|| __|  \\/  | '_ \\     " as &[u8],
        b"     | (_ | _|| |\\/| | | | |    " as &[u8],
        b"      \\___|___|_|  |_|_| |_|    " as &[u8],
    ];

    let info_labels: [&[u8]; 9] = [
        b"OS" as &[u8],
        b"Kernel" as &[u8],
        b"Arch" as &[u8],
        b"Shell" as &[u8],
        b"CPU" as &[u8],
        b"Memory" as &[u8],
        b"Uptime" as &[u8],
        b"Tasks" as &[u8],
        b"Disks" as &[u8],
    ];

    let logo_lines = logo.len();
    let info_count = info_labels.len();
    let max_lines = if logo_lines > info_count { logo_lines } else { info_count };

    for i in 0..max_lines {
        // Logo side (left)
        set_color(0x0E); // yellow
        if i < logo_lines {
            tw(logo[i]);
        } else {
            tw(b"                                   \0");
        }

        tput(b' ');

        // Info side (right)
        if i == 0 {
            // Title with underline
            set_color(0x0F);
            tw(b"user\0");
            set_color(0x08);
            tw(b"@\0");
            set_color(0x0F);
            tw(b"elitra");
            set_color(0x08);
            // draw line
            for _ in 0..32 { tput(b'-'); }
        } else if i - 1 < info_count {
            set_color(0x0E); // yellow label
            tw(info_labels[i - 1]);
            set_color(0x08); // dark gray
            tw(b": \0");
            set_color(0x0F); // bright white value

            match i - 1 {
                0 => {
                    // OS
                    tw(crate::KERNEL_NAME.as_bytes());
                    tput(b' ');
                    tput(b'v');
                    tw(crate::KERNEL_VERSION.as_bytes());
                }
                1 => {
                    tw(crate::KERNEL_NAME.as_bytes());
                    tput(b' ');
                    tw(crate::KERNEL_VERSION.as_bytes());
                }
                2 => {
                    tw(b"x86_64");
                }
                3 => {
                    tw(b"elitra-sh");
                }
                4 => {
                    let vendor = cpuid::vendor();
                    tw(vendor);
                }
                5 => {
                    // Memory with bar
                    progress_bar(used_kb, total_kb, 12, 0x0A, 0x08);
                    tput(b' ');
                    t_uint(used_mb);
                    tput(b'/');
                    t_uint(total_mb);
                    tput(b'M');
                }
                6 => {
                    format_uptime(ticks);
                }
                7 => {
                    t_uint(task_count);
                    tput(b' ');
                    tw(b"processes\0");
                }
                8 => {
                    t_uint(block_count);
                    tput(b' ');
                    tw(b"block devices\0");
                }
                _ => {}
            }
        } else if i - 1 == info_count {
            // Color palette
            set_color(0x08);
            tw(b"                                   \0");
            tput(b' ');
            // 8 basic colors
            let colors: [u8; 8] = [0x00, 0x04, 0x02, 0x06, 0x01, 0x05, 0x03, 0x07];
            for &c in &colors {
                set_color(c);
                tput(BLOCK);
                tput(BLOCK);
            }
            set_color(0x0F);
            tput(b' ');
            // 8 bright colors
            let bright: [u8; 8] = [0x08, 0x0C, 0x0A, 0x0E, 0x09, 0x0D, 0x0B, 0x0F];
            for &c in &bright {
                set_color(c);
                tput(BLOCK);
                tput(BLOCK);
            }
        }
        set_color(0x07);
        tput(b'\n');
    }
}

/// Tree command - display directory tree with box-drawing characters
pub unsafe fn cli_art_print_tree(node: *mut crate::scheduler::VNode, prefix: &[u8], is_last: bool) {
    if node.is_null() { return; }

    // Print prefix
    tw(prefix);

    // Print connector
    if prefix.len() > 0 {
        if is_last {
            put_cp437(BL); // └
            put_cp437(H);  // ─
            put_cp437(H);  // ─
        } else {
            put_cp437(LT); // ├
            put_cp437(H);  // ─
            put_cp437(H);  // ─
        }
    }

    // Print node name
    if (*node).type_ == 1 {
        set_color(0x0B); // cyan for dirs
        tw(&(*node).name);
        tput(b'/');
    } else {
        set_color(0x07); // gray for files
        tw(&(*node).name);
    }
    set_color(0x07);
    tput(b'\n');

    // Recurse into children
    if (*node).type_ == 1 {
        let mut child = (*node).children;
        let mut count = 0u32;
        // Count children first
        let mut c = child;
        while !c.is_null() {
            count += 1;
            c = (*c).next;
        }
        let mut idx = 0u32;
        while !child.is_null() {
            let is_last_child = idx == count - 1;
            let mut new_prefix = [0u8; 128];
            let mut pi = 0;
            // Copy existing prefix
            for &b in prefix {
                if pi < new_prefix.len() { new_prefix[pi] = b; pi += 1; }
            }
            // Add indentation
            if prefix.len() > 0 || true {
                if is_last_child {
                    for _ in 0..3 { if pi < new_prefix.len() { new_prefix[pi] = b' '; pi += 1; } }
                } else {
                    for _ in 0..3 { if pi < new_prefix.len() { new_prefix[pi] = b' '; pi += 1; } }
                }
            }
            new_prefix[pi] = 0;
            cli_art_print_tree(child, &new_prefix[..pi], is_last_child);
            child = (*child).next;
            idx += 1;
        }
    }
}

/// Top-style process monitor (single snapshot)
#[no_mangle]
pub unsafe fn cli_art_print_top() {
    let ticks = krust_pittimer_get_ticks();
    let next_id = krust_sched_get_next_id();

    let mut total_kb: u32 = 0;
    let mut free_kb: u32 = 0;
    krust_mm_info(&mut total_kb as *mut _, &mut free_kb as *mut _);

    let bw: u32 = 60;

    // Title bar
    set_color(0x0B);
    box_line_double(bw);
    tput(b'\n');

    // Title
    set_color(0x0B);
    put_cp437(DV);
    tput(b' ');
    set_color(0x0E);
    tw(b"  Elitra OS Process Monitor\0");
    set_color(0x07);
    tw(b"                      \0");
    set_color(0x0B);
    tw(b" \0");
    put_cp437(DV);
    tput(b'\n');

    set_color(0x0B);
    box_bottom_double(bw);
    set_color(0x07);
    tput(b'\n');

    // System summary
    set_color(0x0A);
    tw(b"  Uptime: \0");
    set_color(0x0F);
    format_uptime(ticks);

    set_color(0x0A);
    tw(b"   Mem: \0");
    set_color(0x0F);
    t_uint(free_kb / 1024);
    tw(b"MB free / \0");
    t_uint(total_kb / 1024);
    tw(b"MB total");
    tput(b'\n');
    tput(b'\n');

    // Process table
    let table_bw: u32 = 58;

    // Top border
    set_color(0x0B);
    put_cp437(TL);
    for _ in 0..5 { put_cp437(H); }
    put_cp437(TT);
    for _ in 0..6 { put_cp437(H); }
    put_cp437(TT);
    for _ in 0..8 { put_cp437(H); }
    put_cp437(TT);
    for _ in 0..7 { put_cp437(H); }
    put_cp437(TT);
    for _ in 0..7 { put_cp437(H); }
    put_cp437(TT);
    for _ in 0..20 { put_cp437(H); }
    put_cp437(TR);
    tput(b'\n');

    // Header
    set_color(0x0B);
    put_cp437(V);
    set_color(0x0F);
    tw(b" PID \0");
    set_color(0x0B);
    put_cp437(V);
    set_color(0x0F);
    tw(b" PPID \0");
    set_color(0x0B);
    put_cp437(V);
    set_color(0x0F);
    tw(b" STATE  \0");
    set_color(0x0B);
    put_cp437(V);
    set_color(0x0F);
    tw(b" PRIO \0");
    set_color(0x0B);
    put_cp437(V);
    set_color(0x0F);
    tw(b" SIGS \0");
    set_color(0x0B);
    put_cp437(V);
    set_color(0x0F);
    tw(b" CWD              \0");
    set_color(0x0B);
    put_cp437(V);
    tput(b'\n');

    // Separator
    set_color(0x0B);
    put_cp437(LT);
    for _ in 0..5 { put_cp437(H); }
    put_cp437(CR);
    for _ in 0..6 { put_cp437(H); }
    put_cp437(CR);
    for _ in 0..8 { put_cp437(H); }
    put_cp437(CR);
    for _ in 0..7 { put_cp437(H); }
    put_cp437(CR);
    for _ in 0..7 { put_cp437(H); }
    put_cp437(CR);
    for _ in 0..20 { put_cp437(H); }
    put_cp437(RT);
    tput(b'\n');

    for pid in 0..next_id {
        let task = krust_sched_get_task(pid);
        if task.is_null() { continue; }

        set_color(0x0B);
        put_cp437(V);
        set_color(0x0F);
        let mut pid_buf = [0u8; 8];
        krust_uitoa((*task).id, pid_buf.as_mut_ptr());
        let pid_len = krust_strlen(pid_buf.as_ptr());
        let pad = if pid_len >= 5 { 0 } else { 5 - pid_len };
        for _ in 0..pad { tput(b' '); }
        tw(&pid_buf);

        set_color(0x0B);
        put_cp437(V);
        set_color(0x07);
        let mut ppid_buf = [0u8; 8];
        krust_uitoa((*task).ppid, ppid_buf.as_mut_ptr());
        let ppid_len = krust_strlen(ppid_buf.as_ptr());
        let pad = if ppid_len >= 6 { 0 } else { 6 - ppid_len };
        for _ in 0..pad { tput(b' '); }
        tw(&ppid_buf);

        set_color(0x0B);
        put_cp437(V);
        let (state_str, state_color) = match (*task).state {
            crate::scheduler::TaskState::RUNNING => (b"RUN    \0", 0x0Au8),
            crate::scheduler::TaskState::READY => (b"READY  \0", 0x0Fu8),
            crate::scheduler::TaskState::BLOCKED => (b"BLOCK  \0", 0x0Eu8),
            crate::scheduler::TaskState::WAITING => (b"WAIT   \0", 0x0Du8),
            crate::scheduler::TaskState::EXITED => (b"EXIT   \0", 0x0Cu8),
        };
        set_color(state_color);
        tput(b' ');
        tw(state_str);

        set_color(0x0B);
        put_cp437(V);
        set_color(0x07);
        let mut prio_buf = [0u8; 8];
        krust_uitoa((*task).priority, prio_buf.as_mut_ptr());
        let prio_len = krust_strlen(prio_buf.as_ptr());
        let pad = if prio_len >= 7 { 0 } else { 7 - prio_len };
        let half = pad / 2;
        let half2 = pad - half;
        for _ in 0..half + 1 { tput(b' '); }
        tw(&prio_buf);
        for _ in 0..half2 { tput(b' '); }

        // Signals
        set_color(0x0B);
        put_cp437(V);
        set_color(0x07);
        let sigs = (*task).sig_pending;
        let mut sig_buf = [0u8; 8];
        krust_uitoa(sigs, sig_buf.as_mut_ptr());
        let sig_len = krust_strlen(sig_buf.as_ptr());
        let pad = if sig_len >= 7 { 0 } else { 7 - sig_len };
        let half = pad / 2;
        let half2 = pad - half;
        for _ in 0..half + 1 { tput(b' '); }
        tw(&sig_buf);
        for _ in 0..half2 { tput(b' '); }

        // CWD
        set_color(0x0B);
        put_cp437(V);
        set_color(0x0B);
        if (*task).cwd[0] != 0 {
            tput(b' ');
            let cwd_len = krust_strlen((*task).cwd.as_ptr());
            // Truncate to fit
            let max_cwd = 18;
            if cwd_len <= max_cwd {
                tw(core::slice::from_raw_parts((*task).cwd.as_ptr(), cwd_len));
                for _ in cwd_len..max_cwd { tput(b' '); }
            } else {
                tput(b'.');
                tput(b'.');
                tw(core::slice::from_raw_parts((*task).cwd.as_ptr().add(cwd_len - max_cwd + 2), max_cwd - 2));
            }
        } else {
            tw(b" /                 \0");
        }
        tput(b' ');
        set_color(0x0B);
        put_cp437(V);
        tput(b'\n');
    }

    // Bottom border
    set_color(0x0B);
    put_cp437(BL);
    for _ in 0..5 { put_cp437(H); }
    put_cp437(BT);
    for _ in 0..6 { put_cp437(H); }
    put_cp437(BT);
    for _ in 0..8 { put_cp437(H); }
    put_cp437(BT);
    for _ in 0..7 { put_cp437(H); }
    put_cp437(BT);
    for _ in 0..7 { put_cp437(H); }
    put_cp437(BT);
    for _ in 0..20 { put_cp437(H); }
    put_cp437(BR);
    tput(b'\n');
    set_color(0x07);
}

/// Cowsay - ASCII art cow with a speech bubble
#[no_mangle]
pub unsafe fn cli_art_print_cowsay(message: *const u8) {
    if message.is_null() || ptr::read_volatile(message) == 0 {
        return;
    }
    let msg_len = krust_strlen(message);
    if msg_len == 0 { return; }

    let bubble_w = msg_len + 2;

    // Top of bubble
    set_color(0x0F);
    tput(b' ');
    put_cp437(TL);
    for _ in 0..bubble_w { put_cp437(H); }
    put_cp437(TR);
    tput(b'\n');

    // Message
    set_color(0x0F);
    put_cp437(V);
    tput(b' ');
    tw(core::slice::from_raw_parts(message, msg_len));
    tput(b' ');
    put_cp437(V);
    tput(b'\n');

    // Bottom of bubble
    set_color(0x0F);
    tput(b' ');
    put_cp437(BL);
    for _ in 0..bubble_w { put_cp437(H); }
    put_cp437(BR);
    tput(b'\n');

    // Cow
    set_color(0x0E); // yellow cow
    tw(b"        \\   ^__^\n\0");
    tw(b"         \\  (oo)\\_______\n\0");
    tw(b"            (__)\\       )\\/\\\n\0");
    tw(b"                ||----w |\n\0");
    tw(b"                ||     ||\n\0");
    set_color(0x07);
}
