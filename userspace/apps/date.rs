#![no_std]
#![no_main]

include!("../src/rt.rs");

fn print_u32(val: u32) {
    if val == 0 { sys_write(b"0"); return; }
    let mut tmp = [0u8; 12];
    let mut n = 0;
    let mut v = val;
    while v > 0 { tmp[n] = b'0' + (v % 10) as u8; v /= 10; n += 1; }
    let mut i = n;
    while i > 0 { i -= 1; sys_write(&[tmp[i]]); }
}

#[no_mangle]
pub extern "C" fn rust_main(_argc: u32, _argv: *const *const u8) {
    let mut info = RTCInfo { second: 0, minute: 0, hour: 0, day: 0, month: 0, year: 0 };
    if sys_gettime(&mut info) < 0 {
        println!("date: failed to get time");
        sys_exit();
    }

    let months = [b"Jan\0", b"Feb\0", b"Mar\0", b"Apr\0", b"May\0", b"Jun\0",
                  b"Jul\0", b"Aug\0", b"Sep\0", b"Oct\0", b"Nov\0", b"Dec\0"];
    let mi = if info.month > 0 && (info.month as usize) <= 12 { (info.month - 1) as usize } else { 0 };

    // Format: Mon Jan  1 12:00:00 2024
    let day_names = [b"Sun\0", b"Mon\0", b"Tue\0", b"Wed\0", b"Thu\0", b"Fri\0", b"Sat\0"];

    // Use Zeller's formula to get day of week
    let y = info.year as i32;
    let m = info.month as i32;
    let d = info.day as i32;
    let q = d;
    let mut month = m;
    let mut year = y;
    if month < 3 { month += 12; year -= 1; }
    let k = year % 100;
    let j = year / 100;
    let h = (q + (13 * (month + 1)) / 5 + k + k / 4 + j / 4 - 2 * j) % 7;
    let dow = ((h + 5) % 7 + 7) % 7; // 0=Sun

    sys_write(day_names[dow as usize]);
    sys_write(b" ");
    sys_write(months[mi]);
    sys_write(b" ");
    if info.day < 10 { sys_write(b" "); }
    print_u32(info.day as u32);
    sys_write(b" ");
    if info.hour < 10 { sys_write(b"0"); }
    print_u32(info.hour as u32);
    sys_write(b":");
    if info.minute < 10 { sys_write(b"0"); }
    print_u32(info.minute as u32);
    sys_write(b":");
    if info.second < 10 { sys_write(b"0"); }
    print_u32(info.second as u32);
    sys_write(b" UTC ");
    print_u32(info.year as u32);
    sys_write(b"\n");
}
