use litematic_render::terminal::{
    TerminalOptions, TerminalProtocol, detect_terminal_protocol, detect_terminal_size,
    enable_windows_virtual_terminal, encode_halfblocks, encode_iterm2, encode_kitty,
    encode_kitty_png, encode_sixel, encode_terminal, render_to_terminal,
};
use litematic_render::{RenderOutput, RenderStatistics};

fn test_image(width: u32, height: u32) -> RenderOutput {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let r = ((x * 255) / width.max(1)) as u8;
            let g = ((y * 255) / height.max(1)) as u8;
            let b = (((x + y) * 255) / (width + height).max(1)) as u8;
            let a = if (x + y) % 7 == 0 { 0 } else { 255 };
            rgba.extend_from_slice(&[r, g, b, a]);
        }
    }
    RenderOutput {
        width,
        height,
        rgba,
        statistics: RenderStatistics::default(),
    }
}

#[test]
fn kitty_renders_valid_apc_sequences() {
    let img = test_image(64, 64);
    let options = TerminalOptions {
        protocol: TerminalProtocol::Kitty,
        width: Some(40),
        height: Some(20),
        ..Default::default()
    };
    let bytes = encode_kitty(&img, &options).expect("encode kitty");
    let text = std::str::from_utf8(&bytes).expect("valid utf-8");

    assert!(text.starts_with("\x1b_G"));
    assert!(text.contains("a=T,f=100"));
    assert!(text.contains("c=40,r=20"));
    assert!(text.ends_with("\x1b\\\n"));
}

#[test]
fn kitty_png_chunking_handles_large_payload() {
    let large_png = vec![0x89; 10000];
    let options = TerminalOptions::new(TerminalProtocol::Kitty);
    let bytes = encode_kitty_png(&large_png, &options);
    let text = std::str::from_utf8(&bytes).expect("valid utf-8");

    // Must contain intermediate chunk marker m=1 and final chunk marker m=0
    assert!(text.contains("m=1;"));
    assert!(text.contains("m=0;"));
}

#[test]
fn iterm2_renders_valid_osc_sequences() {
    let img = test_image(32, 32);
    let options = TerminalOptions {
        protocol: TerminalProtocol::Iterm2,
        width: Some(60),
        height: Some(30),
        preserve_aspect_ratio: true,
        ..Default::default()
    };
    let bytes = encode_iterm2(&img, &options).expect("encode iterm2");
    let text = std::str::from_utf8(&bytes).expect("valid utf-8");

    assert!(text.starts_with("\x1b]1337;File="));
    assert!(text.contains("inline=1"));
    assert!(text.contains("width=60;height=30"));
    assert!(text.contains("preserveAspectRatio=1"));
    assert!(text.ends_with("\x07\n"));
}

#[test]
fn sixel_renders_valid_dcs_sequences() {
    let img = test_image(48, 48);
    let options = TerminalOptions {
        protocol: TerminalProtocol::Sixel,
        width: Some(30),
        ..Default::default()
    };
    let bytes = encode_sixel(&img, &options).expect("encode sixel");
    let text = std::str::from_utf8(&bytes).expect("valid utf-8");

    assert!(text.starts_with("\x1bPq"));
    // Sixel palette declarations
    assert!(text.contains("#0;2;"));
    // Sixel line feeds
    assert!(text.contains('-'));
    // DCS terminator
    assert!(text.ends_with("\x1b\\\n"));
}

#[test]
fn halfblocks_renders_truecolor_ansi() {
    let img = test_image(16, 16);
    let options = TerminalOptions {
        protocol: TerminalProtocol::Halfblocks,
        width: Some(16),
        height: Some(8),
        ..Default::default()
    };
    let text = encode_halfblocks(&img, &options);

    assert!(text.contains('▀') || text.contains('▄'));
    assert!(text.contains("\x1b[38;2;"));
    assert!(text.ends_with("\x1b[0m\n"));
}

#[test]
fn encode_terminal_produces_non_empty_output_for_all_protocols() {
    let img = test_image(16, 16);
    for proto in TerminalProtocol::ALL {
        let options = TerminalOptions::new(*proto);
        let bytes = encode_terminal(&img, &options).expect("encode terminal");
        assert!(!bytes.is_empty());
    }
}

#[test]
fn render_to_terminal_flushes_all_protocols() {
    let img = test_image(12, 12);
    for protocol in TerminalProtocol::ALL {
        let options = TerminalOptions::new(*protocol);
        let mut buffer = Vec::new();
        let res = render_to_terminal(&img, &options, &mut buffer);
        assert!(res.is_ok());
        assert!(!buffer.is_empty());
    }
}

#[test]
fn protocol_parsing_handles_all_cases() {
    assert_eq!(
        TerminalProtocol::parse("KiTtY"),
        Some(TerminalProtocol::Kitty)
    );
    assert_eq!(
        TerminalProtocol::parse("iterm2"),
        Some(TerminalProtocol::Iterm2)
    );
    assert_eq!(
        TerminalProtocol::parse("SIXel"),
        Some(TerminalProtocol::Sixel)
    );
    assert_eq!(
        TerminalProtocol::parse("halfblocks"),
        Some(TerminalProtocol::Halfblocks)
    );
    assert_eq!(
        TerminalProtocol::parse("unicode"),
        Some(TerminalProtocol::Halfblocks)
    );
    assert_eq!(
        TerminalProtocol::parse("ansi"),
        Some(TerminalProtocol::Halfblocks)
    );
    assert_eq!(TerminalProtocol::parse("none"), None);
}

#[test]
fn windows_vt_helper_can_be_called_safely() {
    enable_windows_virtual_terminal();
}

#[test]
fn detect_terminal_size_does_not_panic() {
    let _ = detect_terminal_size();
}

#[test]
fn detect_protocol_returns_supported_variant() {
    let proto = detect_terminal_protocol();
    assert!(TerminalProtocol::ALL.contains(&proto));
}
