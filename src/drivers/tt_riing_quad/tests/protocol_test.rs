use super::super::protocol::{Command, Response};

#[test]
fn set_speed_bytes_and_parse_status() {
    let cmd = Command::SetSpeed {
        port: 2,
        speed: 123,
    };
    let bytes = cmd.to_bytes();
    std::assert_eq!(bytes, vec![0x00, 0x32, 0x51, 2, 0x01, 123]);

    let mut buf = [0u8; 193];
    buf[2] = 0xFC;
    let resp = Response::parse(cmd, &buf).unwrap();
    std::assert_eq!(resp, Response::Status(0xFC));
}

#[test]
fn get_data_parse() {
    let cmd = Command::GetData { port: 1 };
    let mut buf = [0u8; 193];
    buf[2] = 55;
    buf[3] = 0x10;
    buf[4] = 0x20; // rpm = 0x2010 = 8208
    let resp = Response::parse(cmd, &buf).unwrap();
    match resp {
        Response::Data { speed, rpm } => {
            std::assert_eq!(speed, 55);
            std::assert_eq!(rpm, 0x2010);
        }
        _ => panic!("expected Data"),
    }
}

// #[test]
// fn set_rgb_bytes() {
//     let colors = vec![(1, 2, 3); 52];
//     let cmd = Command::SetRgb {
//         port: 3,
//         mode: 0x24,
//         colors: colors.clone(),
//     };
//     let bytes = cmd.to_bytes();
//     std::assert_eq!(bytes[0..5], [0x00, 0x32, 0x52, 3, 0x24]);
//     // payload (g, r, b order in protocol)
//     for chunk in bytes[5..].chunks(3) {
//         std::assert_eq!(chunk, &[2, 1, 3]); // g=2, r=1, b=3
//     }
//     std::assert_eq!(bytes.len(), 5 + 52 * 3);
// }
