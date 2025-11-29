use const_format::str_repeat;

/// Emplace Websocket Text frame with message into buffer.
pub fn set_text(buf: &mut [u8], msg: &str) -> usize {
    let mut i: usize = 0;

    buf[i] = 0x81; // FIN + Opcode + MASK
    i += 1;

    let len = msg.len();
    if len <= 125 {
        buf[i] = len as u8;
        i += 1;
    } else if len < 65536 {
        buf[i] = 126;
        i += 1;

        for byte in (len as u16).to_be_bytes() {
            buf[i] = byte;
            i += 1;
        }
    } else {
        buf[i] = 127;
        i += 1;

        for byte in (len as u64).to_be_bytes() {
            buf[i] = byte;
            i += 1;
        }
    }

    buf[i..i + len].copy_from_slice(msg.as_bytes());
    i += len;

    i
}

/// Extract message from Websocket Text frame buffer.
pub fn get_text(buf: &[u8]) -> Result<String, std::str::Utf8Error> {
    let masked = ((buf[1] & 0x80) >> 7) == 1;
    let size_encoding = (buf[1] & 0x7f) as usize;

    let mut i: usize = 2;
    #[allow(unused_assignments)]
    let mut size: usize = 0;

    if size_encoding <= 125 {
        size = size_encoding;
    } else if size_encoding == 126 {
        size = (((buf[i] & 0x7f) as usize) << 8) | buf[i + 1] as usize;
        i += 2;
    } else {
        size = (((buf[i] & 0x7f) as usize) << 56)
            | ((buf[i + 1] as usize) << 48)
            | ((buf[i + 2] as usize) << 40)
            | ((buf[i + 3] as usize) << 32)
            | ((buf[i + 4] as usize) << 24)
            | ((buf[i + 5] as usize) << 16)
            | ((buf[i + 6] as usize) << 8)
            | (buf[i + 7] as usize);
        i += 8;
    };

    // TODO: Figure out a better memory allocation strategy here. The goal sould
    // be we have to pre-allocated memory per connection.
    let mut data = vec![0u8; size];

    if masked {
        let mask = &buf[i..i + 4];
        i += 4;

        for x in 0..size {
            data[x] = buf[i + x] ^ mask[x % 4];
        }
    } else {
        data.copy_from_slice(&buf[i..i + size]);
    }

    str::from_utf8(&data).map(|s| s.to_string())
}

#[test]
fn test_ws_get_text() {
    let msg = "aaa";
    let buf = [129, 131, 194, 47, 97, 242, 163, 78, 0];

    let decoded = get_text(&buf).expect("failed to decode frame");
    assert_eq!(decoded, msg);
}

#[test]
fn test_ws_text_frame_small() {
    let msg = str_repeat!("a", 3);
    let mut buf = [0; 1024];

    let _ = set_text(&mut buf, msg);
    let decoded = get_text(&buf).expect("failed to decode frame");
    assert_eq!(decoded, msg);
}

#[test]
fn test_ws_text_frame_medium() {
    let msg = str_repeat!("a", 150);
    let mut buf = [0; 1024];

    let _ = set_text(&mut buf, msg);
    let decoded = get_text(&buf).expect("failed to decode frame");
    assert_eq!(decoded, msg);
}

#[test]
fn test_ws_text_frame_huge() {
    let msg = str_repeat!("a", 65536);
    let mut buf = [0; 1024 * 1024];

    let _ = set_text(&mut buf, msg);
    let decoded = get_text(&buf).expect("failed to decode frame");
    assert_eq!(decoded, msg);
}
