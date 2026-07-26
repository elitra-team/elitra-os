#![no_std]
#![no_main]

include!("../src/rt.rs");

#[no_mangle]
pub extern "C" fn rust_main(argc: u32, argv: *const *const u8) {
    if argc < 3 {
        sys_write(b"cut: cut -d<delim> -f<fields>\n");
        sys_write(b"  Reads stdin, outputs selected fields\n");
        sys_write(b"  Example: echo 'a:b:c' | cut -d: -f1,3\n");
        sys_exit_code(1);
    }

    let mut delim = b':';
    let mut fields_start = 1;
    let mut fields: [u16; 32] = [0; 32];
    let mut field_count = 0;

    let mut i = 1;
    while (i as u32) < argc {
        let arg = unsafe { arg_at(argv, i as usize) };
        if arg.starts_with("-d") || arg.starts_with("--delimiter=") {
            let d = if arg.starts_with("--delimiter=") {
                &arg[12..]
            } else if arg.len() > 2 {
                &arg[2..]
            } else if (i as u32) + 1 < argc {
                i += 1;
                unsafe { arg_at(argv, i as usize) }
            } else {
                ""
            };
            if !d.is_empty() {
                delim = d.as_bytes()[0];
            }
        } else if arg.starts_with("-f") || arg.starts_with("--fields=") {
            let f = if arg.starts_with("--fields=") {
                &arg[9..]
            } else if arg.len() > 2 {
                &arg[2..]
            } else if (i as u32) + 1 < argc {
                i += 1;
                unsafe { arg_at(argv, i as usize) }
            } else {
                ""
            };
            // Parse comma-separated field numbers
            let mut num = 0u16;
            for &b in f.as_bytes() {
                if b == b',' {
                    if field_count < 32 && num > 0 {
                        fields[field_count] = num;
                        field_count += 1;
                    }
                    num = 0;
                } else if b >= b'0' && b <= b'9' {
                    num = num * 10 + (b - b'0') as u16;
                }
            }
            if field_count < 32 && num > 0 {
                fields[field_count] = num;
                field_count += 1;
            }
        }
        i += 1;
    }

    if field_count == 0 {
        sys_write(b"cut: no fields specified\n");
        sys_exit_code(1);
    }

    let mut buf = [0u8; 512];
    let mut line_buf = [0u8; 2048];
    let mut line_len = 0;

    loop {
        let n = sys_read(0, &mut buf);
        if n <= 0 { break; }

        for idx in 0..n as usize {
            if buf[idx] == b'\n' {
                line_buf[line_len] = 0;
                // Process the line
                let mut field_num = 1u16;
                let mut field_idx = 0;
                let mut pos = 0;
                let mut start = 0;
                let mut in_range = false;

                // Check if field_num is in our list
                while field_idx < field_count && fields[field_idx] < field_num {
                    field_idx += 1;
                }

                while pos <= line_len {
                    if pos == line_len || line_buf[pos] == delim {
                        if field_idx < field_count && fields[field_idx] == field_num {
                            if in_range { sys_write(&[delim]); }
                            let end = pos;
                            if end > start {
                                sys_write(&line_buf[start..end]);
                            }
                            in_range = true;
                        }
                        field_num += 1;
                        field_idx += 1;
                        start = pos + 1;
                    }
                    pos += 1;
                }
                sys_write(b"\n");
                line_len = 0;
            } else if line_len < 2047 {
                line_buf[line_len] = buf[idx];
                line_len += 1;
            }
        }
    }

    // Handle last line without newline
    if line_len > 0 {
        line_buf[line_len] = 0;
        let mut field_num = 1u16;
        let mut field_idx = 0;
        let mut pos = 0;
        let mut start = 0;
        let mut in_range = false;

        while field_idx < field_count && fields[field_idx] < field_num {
            field_idx += 1;
        }

        while pos <= line_len {
            if pos == line_len || line_buf[pos] == delim {
                if field_idx < field_count && fields[field_idx] == field_num {
                    if in_range { sys_write(&[delim]); }
                    let end = pos;
                    if end > start {
                        sys_write(&line_buf[start..end]);
                    }
                    in_range = true;
                }
                field_num += 1;
                field_idx += 1;
                start = pos + 1;
            }
            pos += 1;
        }
        sys_write(b"\n");
    }
}
