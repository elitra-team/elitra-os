#![no_std]
#![no_main]

include!("../src/rt.rs");

#[derive(Copy, Clone)]
struct Line {
    data: [u8; 256],
    len: usize,
}

struct LineVec {
    lines: [Line; 4096],
    count: usize,
}

impl LineVec {
    fn new() -> Self {
        LineVec {
            lines: [Line { data: [0u8; 256], len: 0 }; 4096],
            count: 0,
        }
    }
    fn push(&mut self, line: Line) {
        if self.count < 4096 {
            self.lines[self.count] = line;
            self.count += 1;
        }
    }
    fn len(&self) -> usize { self.count }
    fn iter(&self) -> core::slice::Iter<'_, Line> {
        self.lines[..self.count].iter()
    }
    fn swap(&mut self, a: usize, b: usize) {
        let tmp = self.lines[a];
        self.lines[a] = self.lines[b];
        self.lines[b] = tmp;
    }
}

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) {
    let mut lines = LineVec::new();
    let mut reverse = false;
    let mut file_arg_start: usize = 1;

    if argc > 1 {
        let first = unsafe { arg_at(argv, 1) };
        if first == "-r" || first == "--reverse" {
            reverse = true;
            file_arg_start = 2;
        }
    }

    let mut from_stdin = true;
    let mut i = file_arg_start;
    while (i as u32) < argc {
        from_stdin = false;
        let path = unsafe { arg_at(argv, i) };
        let fd = sys_open(path);
        if fd < 0 {
            sys_write(b"sort: cannot open ");
            sys_write(path.as_bytes());
            sys_write(b"\n");
            i += 1;
            continue;
        }
        read_lines(fd, &mut lines);
        sys_close(fd);
        i += 1;
    }

    if from_stdin {
        read_lines(0, &mut lines);
    }

    // Insertion sort
    let len = lines.len();
    if len > 1 {
        let mut i = 1;
        while i < len {
            let mut j = i;
            while j > 0 {
                let should_swap = if reverse {
                    cmp_lines(&lines.lines[j], &lines.lines[j - 1]) < 0
                } else {
                    cmp_lines(&lines.lines[j], &lines.lines[j - 1]) > 0
                };
                if should_swap {
                    lines.swap(j, j - 1);
                    j -= 1;
                } else {
                    break;
                }
            }
            i += 1;
        }
    }

    for l in lines.iter() {
        sys_write(&l.data[..l.len]);
        sys_write(b"\n");
    }
}

fn cmp_lines(a: &Line, b: &Line) -> i32 {
    let min_len = if a.len < b.len { a.len } else { b.len };
    let mut i = 0;
    while i < min_len {
        if a.data[i] < b.data[i] { return -1; }
        if a.data[i] > b.data[i] { return 1; }
        i += 1;
    }
    if a.len < b.len { return -1; }
    if a.len > b.len { return 1; }
    0
}

fn read_lines(fd: isize, lines: &mut LineVec) {
    let mut buf = [0u8; 512];
    let mut line = Line { data: [0u8; 256], len: 0 };

    loop {
        let n = sys_read(fd, &mut buf);
        if n <= 0 { break; }
        for i in 0..n as usize {
            if buf[i] == b'\n' {
                lines.push(line);
                line = Line { data: [0u8; 256], len: 0 };
            } else if line.len < 255 {
                line.data[line.len] = buf[i];
                line.len += 1;
            }
        }
    }
    if line.len > 0 {
        lines.push(line);
    }
}
