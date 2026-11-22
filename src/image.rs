//! Image logos through the Kitty graphics protocol.

use std::fs::File;
use std::io::{IsTerminal, Read};
use std::path::Path;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use color_eyre::eyre::{bail, Result, WrapErr};
use sha2::{Digest, Sha256};
use unicode_width::UnicodeWidthChar;

use crate::logo::Logo;
use crate::render::{to_ansi, Line, Span};

/// Cell used to reserve image space in a styled line. It is one column wide.
pub const MARKER: char = '\u{10FFFD}';
/// Placeholder for the top-left image cell during ANSI rendering.
const SENTINEL: char = '\u{10FFFC}';

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const MAX_CHUNK_CHARS: usize = 4096;
const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_IMAGE_DIMENSION: u32 = 16_384;

/// Graphics protocol understood by the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Kitty,
}

/// Detect a graphics protocol from terminal environment values.
pub fn detect_protocol(
    term_program: Option<&str>,
    term: Option<&str>,
    kitty_window: bool,
    wezterm_pane: bool,
) -> Option<Protocol> {
    if term.is_some_and(|value| value.eq_ignore_ascii_case("dumb")) {
        return None;
    }
    if matches!(term_program, Some("koral" | "ghostty" | "WezTerm")) {
        return Some(Protocol::Kitty);
    }
    if kitty_window || wezterm_pane || term == Some("xterm-kitty") {
        return Some(Protocol::Kitty);
    }
    None
}

/// Whether the attached terminal can display an image logo.
pub fn terminal_supports_image() -> bool {
    if !stdout_is_terminal() {
        return false;
    }
    let term_program = std::env::var("TERM_PROGRAM").ok();
    let term = std::env::var("TERM").ok();
    let kitty_window = env_flag("KITTY_WINDOW_ID");
    let wezterm_pane = env_flag("WEZTERM_PANE");
    detect_protocol(
        term_program.as_deref(),
        term.as_deref(),
        kitty_window,
        wezterm_pane,
    )
    .is_some()
}

/// Whether stdout is a terminal. Used by the explicit `kitty` protocol setting
/// to allow a terminal that does not advertise itself through the environment.
pub fn stdout_is_terminal() -> bool {
    std::io::stdout().is_terminal()
}

fn env_flag(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| !value.is_empty())
}

/// A logo made of marker cells, later replaced by the image.
pub fn marker_logo(columns: usize, rows: usize) -> Logo {
    let columns = columns.max(1);
    let rows = rows.max(1);
    Logo {
        lines: vec![MARKER.to_string().repeat(columns); rows],
        width: columns,
    }
}

/// Stable 31-bit image id derived from the path.
pub fn image_id(path: &Path) -> u32 {
    let digest = Sha256::digest(path.as_os_str().as_encoded_bytes());
    let id = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]) & 0x7fff_ffff;
    if id == 0 {
        1
    } else {
        id
    }
}

/// Encode `png` as Kitty graphics APC chunks sized for the given cell grid.
pub fn kitty_sequence(png: &[u8], id: u32, columns: usize, rows: usize) -> Result<String> {
    validate_png(png)?;

    let encoded = BASE64.encode(png);
    let chunks: Vec<&str> = encoded
        .as_bytes()
        .chunks(MAX_CHUNK_CHARS)
        .map(|chunk| std::str::from_utf8(chunk).expect("base64 output is ASCII"))
        .collect();
    let last = chunks.len().saturating_sub(1);

    let mut out = String::new();
    for (index, data) in chunks.iter().enumerate() {
        let more = index < last;
        let control = if index == 0 {
            let mut control = format!("a=T,f=100,c={columns},r={rows},z=-1,C=1,q=2,i={id}");
            control.push_str(if more { ",m=1" } else { ",m=0" });
            control
        } else if more {
            "m=1".to_string()
        } else {
            "m=0".to_string()
        };
        out.push_str("\x1b_G");
        out.push_str(&control);
        out.push(';');
        out.push_str(data);
        out.push_str("\x1b\\");
    }
    Ok(out)
}

/// Render `lines`, replacing a marker rectangle with the image at `source.path`.
/// A missing or malformed rectangle is an error so the caller can use the original
/// character logo instead of blanking it.
pub fn render(mut lines: Vec<Line>, source: &ImageSource) -> Result<String> {
    let png = read_png(&source.path)?;
    let sequence = kitty_sequence(&png, image_id(&source.path), source.columns, source.rows)?;

    let rect = find_rect(&lines, source.columns, source.rows)
        .ok_or_else(|| color_eyre::eyre::eyre!("image logo area not found"))?;
    clear_markers(&mut lines, &rect);

    let ansi = to_ansi(&lines);
    Ok(ansi.replacen(&SENTINEL.to_string(), &format!("{sequence} "), 1))
}

/// An image logo: a PNG path and the cell grid it occupies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageSource {
    pub path: std::path::PathBuf,
    pub columns: usize,
    pub rows: usize,
}

fn read_png(path: &Path) -> Result<Vec<u8>> {
    let file = File::open(path).wrap_err_with(|| format!("open image {}", path.display()))?;
    let len = file
        .metadata()
        .wrap_err_with(|| format!("stat image {}", path.display()))?
        .len();
    if len > MAX_IMAGE_BYTES {
        bail!("image {} is larger than 16 MiB", path.display());
    }

    let mut png = Vec::with_capacity(len as usize + 1);
    file.take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut png)
        .wrap_err_with(|| format!("read image {}", path.display()))?;
    if png.len() as u64 > MAX_IMAGE_BYTES {
        bail!("image {} is larger than 16 MiB", path.display());
    }
    Ok(png)
}

/// Validate a PNG file before sending it to the terminal.
pub fn validate_file(path: &Path) -> Result<()> {
    let png = read_png(path)?;
    validate_png(&png)
}

/// Checks the signature, every chunk boundary, the CRC, the IHDR fields and the
/// presence of image data and an end marker. This is not a full decoder, but it
/// rejects truncated, corrupted and structurally invalid files.
fn validate_png(png: &[u8]) -> Result<()> {
    if !png.starts_with(PNG_SIGNATURE) {
        bail!("not a PNG image: expected signature 89 50 4e 47 0d 0a 1a 0a");
    }

    let mut position = PNG_SIGNATURE.len();
    let mut first = true;
    let mut has_data = false;
    let mut has_end = false;
    while position < png.len() {
        if png.len() - position < 12 {
            bail!("truncated PNG chunk");
        }
        let length = u32::from_be_bytes([
            png[position],
            png[position + 1],
            png[position + 2],
            png[position + 3],
        ]) as usize;
        let data_start = position + 8;
        let data_end = data_start
            .checked_add(length)
            .ok_or_else(|| color_eyre::eyre::eyre!("PNG chunk length overflow"))?;
        let crc_end = data_end
            .checked_add(4)
            .ok_or_else(|| color_eyre::eyre::eyre!("PNG chunk length overflow"))?;
        if crc_end > png.len() {
            bail!("truncated PNG chunk");
        }
        let kind = &png[position + 4..position + 8];
        let expected = u32::from_be_bytes([
            png[data_end],
            png[data_end + 1],
            png[data_end + 2],
            png[data_end + 3],
        ]);
        if crc32(&png[position + 4..data_end]) != expected {
            bail!("PNG chunk has an invalid CRC");
        }

        if first {
            if kind != b"IHDR" || length != 13 {
                bail!("PNG does not start with a 13-byte IHDR chunk");
            }
            validate_ihdr(&png[data_start..data_end])?;
            first = false;
        }
        match kind {
            b"IDAT" => has_data = true,
            b"IEND" => {
                if length != 0 {
                    bail!("PNG IEND chunk is not empty");
                }
                has_end = true;
                break;
            }
            _ => {}
        }
        position = crc_end;
    }

    if first {
        bail!("PNG has no IHDR chunk");
    }
    if !has_data {
        bail!("PNG has no IDAT chunk");
    }
    if !has_end {
        bail!("PNG has no IEND chunk");
    }
    Ok(())
}

fn validate_ihdr(data: &[u8]) -> Result<()> {
    let width = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    let height = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    if width == 0 || height == 0 {
        bail!("PNG has a zero width or height");
    }
    if width > MAX_IMAGE_DIMENSION || height > MAX_IMAGE_DIMENSION {
        bail!("PNG is larger than {MAX_IMAGE_DIMENSION} pixels on a side");
    }
    let bit_depth = data[8];
    let color_type = data[9];
    let allowed = match color_type {
        0 => matches!(bit_depth, 1 | 2 | 4 | 8 | 16),
        2 => matches!(bit_depth, 8 | 16),
        3 => matches!(bit_depth, 1 | 2 | 4 | 8),
        4 | 6 => matches!(bit_depth, 8 | 16),
        _ => false,
    };
    if !allowed {
        bail!("PNG has an invalid bit depth or color type");
    }
    if data[10] != 0 || data[11] != 0 || data[12] > 1 {
        bail!("PNG uses an unsupported compression, filter or interlace method");
    }
    Ok(())
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

#[derive(Debug, Clone, Copy)]
struct MarkerRect {
    first_row: usize,
    start_col: usize,
}

fn marker_columns(line: &Line) -> Vec<usize> {
    let mut column = 0;
    let mut markers = Vec::new();
    for span in &line.spans {
        for ch in span.text.chars() {
            if ch == MARKER {
                markers.push(column);
            }
            column += UnicodeWidthChar::width(ch).unwrap_or(0);
        }
    }
    markers
}

/// Find an exact `columns` by `rows` block of markers.
fn find_rect(lines: &[Line], columns: usize, rows: usize) -> Option<MarkerRect> {
    let per_line: Vec<Vec<usize>> = lines.iter().map(marker_columns).collect();
    let first_row = per_line.iter().position(|line| !line.is_empty())?;
    if per_line.iter().filter(|line| !line.is_empty()).count() != rows {
        return None;
    }
    if first_row + rows > lines.len() {
        return None;
    }

    // Every marker row must be a single contiguous run of `columns` cells.
    // Repeated runs in one line would make the placement ambiguous.
    let first = &per_line[first_row];
    let start_col = first[0];
    if first.len() != columns
        || first
            .iter()
            .enumerate()
            .any(|(index, &col)| col != start_col + index)
    {
        return None;
    }

    let expected: Vec<usize> = (start_col..start_col + columns).collect();
    if per_line[first_row..first_row + rows]
        .iter()
        .all(|line| *line == expected)
    {
        Some(MarkerRect {
            first_row,
            start_col,
        })
    } else {
        None
    }
}

/// Replace every marker with an unstyled space and mark the image origin.
fn clear_markers(lines: &mut [Line], rect: &MarkerRect) {
    for (row, line) in lines.iter_mut().enumerate() {
        let mut rebuilt = Line::new();
        let mut column = 0;
        for span in &line.spans {
            for ch in span.text.chars() {
                if ch == MARKER {
                    if row == rect.first_row && column == rect.start_col {
                        rebuilt.push(Span::plain(SENTINEL.to_string()));
                    } else {
                        rebuilt.push(Span::plain(" ".to_string()));
                    }
                } else {
                    rebuilt.push(Span::new(ch.to_string(), span.style));
                }
                column += UnicodeWidthChar::width(ch).unwrap_or(0);
            }
        }
        *line = rebuilt;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::Style;
    use crate::theme::Color;
    use std::path::PathBuf;

    const PNG_1X1: &[u8] = &[
        0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, b'I', b'H', b'D',
        b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0b, b'I', b'D', b'A', b'T', 0x78, 0x9c, 0x63, 0xf8,
        0x0f, 0x04, 0x00, 0x09, 0xfb, 0x03, 0xfd, 0xfb, 0x5e, 0x6b, 0x2b, 0x00, 0x00, 0x00, 0x00,
        b'I', b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82,
    ];

    struct TempPng(PathBuf);

    impl TempPng {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("halofetch-image-{}-{name}.png", std::process::id()));
            std::fs::write(&path, PNG_1X1).expect("write test PNG");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempPng {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn chunks(sequence: &str) -> Vec<(&str, &str)> {
        sequence
            .split("\x1b_G")
            .filter(|part| !part.is_empty())
            .map(|part| {
                let body = part.strip_suffix("\x1b\\").expect("chunk terminator");
                body.split_once(';').expect("control separator")
            })
            .collect()
    }

    /// Width of the output with CSI and Kitty escapes removed.
    fn visible_width(text: &str) -> usize {
        let mut width = 0;
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\n' {
                break;
            }
            if bytes[i] != 0x1b {
                let ch = text[i..].chars().next().expect("char at byte index");
                width += UnicodeWidthChar::width(ch).unwrap_or(0);
                i += ch.len_utf8();
                continue;
            }
            i += 1;
            if i >= bytes.len() {
                break;
            }
            match bytes[i] {
                b'[' => {
                    i += 1;
                    while i < bytes.len() {
                        let b = bytes[i];
                        i += 1;
                        if (b'@'..=b'~').contains(&b) {
                            break;
                        }
                    }
                }
                b'_' => {
                    // APC: skip until the string terminator ESC backslash.
                    i += 1;
                    while i + 1 < bytes.len() {
                        if bytes[i] == 0x1b && bytes[i + 1] == b'\\' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => {}
            }
        }
        width
    }

    #[test]
    fn detect_protocol_recognizes_kitty_environments() {
        assert_eq!(
            detect_protocol(Some("koral"), None, false, false),
            Some(Protocol::Kitty)
        );
        assert_eq!(
            detect_protocol(Some("ghostty"), None, false, false),
            Some(Protocol::Kitty)
        );
        assert_eq!(
            detect_protocol(Some("WezTerm"), None, false, false),
            Some(Protocol::Kitty)
        );
        assert_eq!(
            detect_protocol(None, Some("xterm-kitty"), false, false),
            Some(Protocol::Kitty)
        );
        assert_eq!(
            detect_protocol(None, None, true, false),
            Some(Protocol::Kitty)
        );
        assert_eq!(
            detect_protocol(None, None, false, true),
            Some(Protocol::Kitty)
        );
    }

    #[test]
    fn detect_protocol_rejects_dumb_and_unknown_terminals() {
        assert_eq!(detect_protocol(None, Some("dumb"), true, true), None);
        assert_eq!(
            detect_protocol(Some("koral"), Some("DUMB"), false, false),
            None
        );
        assert_eq!(
            detect_protocol(None, Some("xterm-256color"), false, false),
            None
        );
        assert_eq!(detect_protocol(None, None, false, false), None);
    }

    #[test]
    fn marker_is_single_width_and_logo_clamps_dimensions() {
        assert_eq!(UnicodeWidthChar::width(MARKER), Some(1));
        let logo = marker_logo(3, 2);
        assert_eq!(logo.width, 3);
        assert_eq!(logo.lines, vec![MARKER.to_string().repeat(3); 2]);
        let clamped = marker_logo(0, 0);
        assert_eq!(clamped.width, 1);
        assert_eq!(clamped.lines, vec![MARKER.to_string()]);
    }

    #[test]
    fn image_id_is_stable_positive_and_31_bits() {
        let id = image_id(Path::new("/tmp/logo.png"));
        assert_ne!(id, 0);
        assert_eq!(id & 0x8000_0000, 0);
        assert_eq!(id, image_id(Path::new("/tmp/logo.png")));
        assert_ne!(id, image_id(Path::new("/tmp/other.png")));
    }

    #[test]
    fn kitty_sequence_describes_single_chunk_image() {
        let sequence = kitty_sequence(PNG_1X1, 42, 3, 2).expect("PNG sequence");
        assert_eq!(
            sequence,
            format!(
                "\x1b_Ga=T,f=100,c=3,r=2,z=-1,C=1,q=2,i=42,m=0;{}\x1b\\",
                BASE64.encode(PNG_1X1)
            )
        );
        let parsed = chunks(&sequence);
        assert_eq!(parsed.len(), 1);
        assert_eq!(BASE64.decode(parsed[0].1).expect("base64"), PNG_1X1);
    }

    #[test]
    fn kitty_sequence_splits_payload_into_limited_chunks() {
        let mut png = PNG_1X1.to_vec();
        png.resize(9008, 0x5a);
        let sequence = kitty_sequence(&png, 7, 8, 4).expect("PNG sequence");
        let parsed = chunks(&sequence);
        assert!(parsed.len() >= 3, "expected multiple chunks");

        for (index, (control, data)) in parsed.iter().enumerate() {
            assert!(data.len() <= MAX_CHUNK_CHARS);
            if index == 0 {
                assert!(control.starts_with("a=T,f=100,c=8,r=4,z=-1,C=1,q=2,i=7"));
                assert!(control.ends_with(",m=1"));
            } else if index + 1 == parsed.len() {
                assert_eq!(*control, "m=0");
            } else {
                assert_eq!(*control, "m=1");
            }
        }

        let joined: String = parsed.iter().map(|(_, data)| *data).collect();
        assert_eq!(BASE64.decode(joined).expect("base64"), png);
    }

    #[test]
    fn kitty_sequence_rejects_non_png_bytes() {
        let error = kitty_sequence(b"not a PNG", 1, 1, 1).expect_err("invalid PNG");
        assert!(format!("{error:?}").to_lowercase().contains("png"));
    }

    #[test]
    fn png_validation_rejects_bad_crc_and_truncation() {
        let mut corrupt = PNG_1X1.to_vec();
        corrupt[20] ^= 0xff;
        assert!(validate_png(&corrupt).is_err());
        assert!(validate_png(&PNG_1X1[..PNG_1X1.len() - 5]).is_err());
        assert!(validate_png(&PNG_1X1[..20]).is_err());
        assert!(validate_png(PNG_1X1).is_ok());
    }

    #[test]
    fn render_replaces_marker_grid_and_preserves_line_width() {
        let png = TempPng::new("grid");
        let source = ImageSource {
            path: png.path().to_path_buf(),
            columns: 3,
            rows: 2,
        };
        let markers = MARKER.to_string().repeat(3);
        let colored = Style::new().fg(Color::new(1, 2, 3));
        let lines = vec![
            Line::from_spans(vec![
                Span::plain("ab"),
                Span::new(markers.clone(), colored),
                Span::plain("X"),
            ]),
            Line::from_spans(vec![
                Span::plain("12"),
                Span::plain(markers),
                Span::plain("Y"),
            ]),
        ];

        let rendered = render(lines, &source).expect("render");
        assert!(rendered.contains("\x1b_G"));
        assert!(!rendered.contains(MARKER));
        assert!(!rendered.contains(SENTINEL));
        assert!(!rendered.contains("\x1b[38;2;1;2;3m"));
        let widths = rendered.lines().map(visible_width).collect::<Vec<_>>();
        assert_eq!(widths, vec![6, 6]);
    }

    #[test]
    fn render_without_marker_is_an_error() {
        let png = TempPng::new("no-marker");
        let source = ImageSource {
            path: png.path().to_path_buf(),
            columns: 3,
            rows: 2,
        };
        let lines = vec![Line::from_spans(vec![Span::plain("hello")])];
        assert!(render(lines, &source).is_err());
    }

    #[test]
    fn render_with_wrong_or_truncated_rectangle_is_an_error() {
        let png = TempPng::new("wrong-size");
        let source = ImageSource {
            path: png.path().to_path_buf(),
            columns: 3,
            rows: 2,
        };
        let narrow = MARKER.to_string().repeat(2);
        let lines = vec![
            Line::from_spans(vec![Span::plain(narrow.clone())]),
            Line::from_spans(vec![Span::plain(narrow)]),
        ];
        assert!(render(lines, &source).is_err());

        let truncated = vec![
            Line::from_spans(vec![Span::plain(format!("{MARKER}{MARKER}\u{2026}"))]),
            Line::from_spans(vec![Span::plain(MARKER.to_string().repeat(3))]),
        ];
        assert!(render(truncated, &source).is_err());
    }
}
