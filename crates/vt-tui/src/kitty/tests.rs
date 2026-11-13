use super::encoding::{adler32, crc32};
use super::*;

type Chunks = Vec<(String, Vec<u8>)>;

/// A PNG's chunks, having checked their CRCs, and its pixels, each as the number of its colour
/// in the palette.
fn decode_colors(png: &[u8]) -> (Chunks, Vec<Vec<u8>>) {
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let mut chunks = Vec::new();
    let mut at = 8;
    while at < png.len() {
        let length = u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
        let (name, data) = (&png[at + 4..at + 8], &png[at + 8..at + 8 + length]);
        let crc = u32::from_be_bytes(png[at + 8 + length..at + 12 + length].try_into().unwrap());
        assert_eq!(crc32(&[name, data]), crc);
        chunks.push((String::from_utf8(name.to_vec()).unwrap(), data.to_vec()));
        at += 12 + length;
    }
    let header = &chunks[0].1;
    let width = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
    let depth = usize::from(header[8]);
    let idat = &chunks.iter().find(|(name, _)| name == "IDAT").unwrap().1;
    // The decoder checks the zlib data's own checksum, and what's stored or compressed.
    let mut lines = Vec::new();
    flate2::read::ZlibDecoder::new(&idat[..])
        .read_to_end(&mut lines)
        .unwrap();
    let stride = (width * depth).div_ceil(8) + 1;
    assert_eq!(lines.len() % stride, 0);
    let pixels = lines
        .chunks(stride)
        .map(|line| {
            assert_eq!(line[0], 0);
            (0..width)
                .map(|x| {
                    let bit = x * depth;
                    (line[1 + bit / 8] >> (8 - depth - bit % 8)) & ((1 << depth) - 1) as u8
                })
                .collect()
        })
        .collect();
    (chunks, pixels)
}

/// A PNG's chunks and which of its pixels are drawn.
fn decode(png: &[u8]) -> (Chunks, Vec<Vec<bool>>) {
    let (chunks, colors) = decode_colors(png);
    let pixels = colors
        .iter()
        .map(|row| row.iter().map(|color| *color != 0).collect())
        .collect();
    (chunks, pixels)
}

fn sent(png: &[u8]) -> Vec<u8> {
    let image = Image {
        id: 7,
        png,
        columns: 28,
        rows: 4,
    };
    let mut out = Vec::new();
    transmit(&mut out, &image).unwrap();
    out
}

#[test]
fn placeholders_are_one_column_each_and_say_where_they_are() {
    let lines = placeholder_lines(28, 4);
    assert_eq!(lines.len(), 4);
    for (row, line) in lines.iter().enumerate() {
        let chars: Vec<char> = line.chars().collect();
        assert_eq!(chars.len(), 28 * 3);
        for (column, cell) in chars.chunks(3).enumerate() {
            assert_eq!(cell, [PLACEHOLDER, POSITIONS[row], POSITIONS[column]]);
        }
        assert_eq!(ratatui::text::Line::raw(line.as_str()).width(), 28);
    }
}

#[test]
fn the_terminal_takes_images_if_it_says_ok_before_its_attributes() {
    let attributes = "\x1b[?62;22c";
    let ok = format!("\x1b_Gi={QUERY_ID};OK\x1b\\{attributes}");
    assert_eq!(answer(ok.as_bytes()), Some(true));
    let refused = format!("\x1b_Gi={QUERY_ID};ENOTSUPPORTED:no\x1b\\{attributes}");
    assert_eq!(answer(refused.as_bytes()), Some(false));
    assert_eq!(answer(attributes.as_bytes()), Some(false));
    // Until the attributes have all come, it isn't over.
    assert_eq!(answer(&ok.as_bytes()[..ok.len() - 1]), None);
    assert_eq!(answer(b"\x1b_Gi=31;OK\x1b\\"), None);
    assert_eq!(answer(b""), None);
}

#[test]
fn the_scope_is_a_small_png_of_its_line() {
    let (across, down) = (
        SCOPE_WIDTH * CELL_DOTS_ACROSS,
        SCOPE_HEIGHT * CELL_DOTS_DOWN,
    );
    let spans: Vec<(usize, usize)> = (0..across)
        .map(|column| (column % down, down - 1))
        .collect();
    let png = scope_png(&spans, [255; 3]);
    assert!(png.len() < 4000, "{}", png.len());
    let (_, pixels) = decode(&png);
    assert_eq!(
        (pixels[0].len(), pixels.len()),
        (across * DOT_PIXELS, down * DOT_PIXELS)
    );
    for (column, (high, low)) in spans.iter().enumerate() {
        for row in 0..down {
            let pixel = pixels[row * DOT_PIXELS][column * DOT_PIXELS];
            assert_eq!(pixel, (*high..=*low).contains(&row), "dot {column}, {row}");
        }
    }
}

#[test]
fn a_color_holds_an_images_id() {
    assert_eq!(id_color(0x12_34_56), Color::Rgb(0x12, 0x34, 0x56));
}

#[test]
fn the_checksums_match_known_values() {
    assert_eq!(crc32(&[b"IEND"]), 0xAE42_6082);
    assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
}

#[test]
fn an_image_is_sent_in_pieces_the_protocol_takes() {
    let pixels = vec![9; 10_000];
    let out = String::from_utf8(sent(&pixels)).unwrap();
    let commands: Vec<&str> = out
        .split("\x1b\\")
        .filter(|command| !command.is_empty())
        .collect();
    assert!(commands.len() > 1);
    assert!(commands[0].starts_with("\x1b_Ga=T,U=1,f=100,i=7,c=28,r=4,q=2,m=1;"));
    let mut data = String::new();
    for (index, command) in commands.iter().enumerate() {
        let (controls, chunk) = command.split_once(';').unwrap();
        let last = index == commands.len() - 1;
        assert!(
            controls.ends_with(if last { "m=0" } else { "m=1" }),
            "{controls}"
        );
        assert!(controls.contains("q=2"), "{controls}");
        assert!(chunk.len() <= CHUNK);
        if !last {
            assert_eq!(chunk.len() % 4, 0);
        }
        data.push_str(chunk);
    }
    assert_eq!(STANDARD.decode(data).unwrap(), pixels);
}

#[test]
fn the_marks_are_all_of_kittys_and_ghosttys_in_order() {
    // From kitty's `rowcolumn-diacritics.txt`, whose list Ghostty's `graphics_unicode.zig` has
    // too: 297 marks in rising order.
    assert_eq!(POSITIONS.len(), 297);
    assert!(POSITIONS.windows(2).all(|pair| pair[0] < pair[1]));
    for (index, mark) in [
        (0, '\u{305}'),
        (27, '\u{36D}'),
        (28, '\u{36E}'),
        (30, '\u{483}'),
        (100, '\u{7EB}'),
        (159, '\u{1CD1}'),
        (200, '\u{20D4}'),
        (296, '\u{1D244}'),
    ] {
        assert_eq!(POSITIONS[index], mark, "mark {index}");
    }
}

#[test]
fn placeholders_past_28_cells_say_where_they_are() {
    let (width, height) = (160, 40);
    let lines = placeholder_lines(width, height);
    assert_eq!(lines.len(), height);
    for (row, line) in lines.iter().enumerate() {
        let chars: Vec<char> = line.chars().collect();
        for (column, cell) in chars.chunks(3).enumerate() {
            assert_eq!(cell, [PLACEHOLDER, POSITIONS[row], POSITIONS[column]]);
        }
        assert_eq!(ratatui::text::Line::raw(line.as_str()).width(), width);
    }
    let corner: Vec<char> = lines[30].chars().skip(159 * 3).collect();
    assert_eq!(corner, [PLACEHOLDER, '\u{483}', '\u{1CD1}']);
    let last = placeholder_lines(MOST_CELLS, MOST_CELLS);
    let corner: Vec<char> = last[MOST_CELLS - 1]
        .chars()
        .skip((MOST_CELLS - 1) * 3)
        .collect();
    assert_eq!(corner, [PLACEHOLDER, '\u{1D244}', '\u{1D244}']);
}

/// A picture with a stripe of each of its colours, under a wave.
fn striped(width: usize, height: usize, colors: usize) -> Picture {
    let palette = (0..colors).map(|color| [color as u8, 100, 200]).collect();
    let mut picture = Picture::new(width, height, palette);
    for x in 0..width {
        let level = (x as f32 * 0.02).sin() * 0.5 + 0.5;
        let top = (level * (height - 1) as f32) as usize;
        for y in top..height {
            picture.pixels[y * width + x] = (1 + x * colors / width) as u8;
        }
    }
    picture
}

#[test]
fn a_picture_takes_as_few_bits_as_its_colours_need() {
    for (colors, depth) in [(1, 1), (2, 2), (3, 2), (4, 4), (15, 4)] {
        let picture = striped(90, 20, colors);
        let (chunks, pixels) = decode_colors(&picture.png());
        assert_eq!(chunks[0].1[8], depth, "{colors} colours");
        assert_eq!(chunks[1].1.len(), 3 * (colors + 1));
        assert_eq!(chunks[2], ("tRNS".to_string(), vec![0]));
        assert_eq!(pixels.concat(), picture.pixels, "{colors} colours");
    }
}

#[test]
fn a_big_picture_is_compressed_and_has_the_same_pixels() {
    // The sample page's waveform: 160 by 14 characters of 15 by 39 pixels, in four colours.
    let picture = striped(2400, 546, 4);
    let png = picture.png();
    let stored = (2400 / 2 + 1) * 546;
    assert!(png.len() < stored / 10, "{} of {stored}", png.len());
    let (_, pixels) = decode_colors(&png);
    assert_eq!(pixels.concat(), picture.pixels);

    // Lines of 64 bytes, a filter byte and 63 of pixels: 1023 of them fit in one stored block,
    // which says it's the last and isn't compressed, and 1024 don't.
    for (height, is_stored) in [(1023, true), (1024, false)] {
        let picture = striped(504, height, 1);
        let (chunks, pixels) = decode_colors(&picture.png());
        let idat = &chunks.iter().find(|(name, _)| name == "IDAT").unwrap().1;
        assert_eq!(idat[..3] == [0x78, 0x01, 1], is_stored, "{height} lines");
        assert_eq!(idat.len() > 64 * height, is_stored, "{height} lines");
        assert_eq!(pixels.concat(), picture.pixels);
    }
}

#[test]
fn the_existing_drawings_are_unchanged() {
    // Each PNG's size and CRC, as they were before pictures could have more colours or be
    // compressed: the explorer's preview and the kit picker's. The envelopes are curves now,
    // drawn by `plot`, and the Sound page's waveform is a thumbnail drawn as the sample page's
    // waveform is.
    let frames: Vec<[f32; 2]> = (0..3000)
        .map(|frame| {
            let level = (frame as f32 * 0.05).sin() * (-(frame as f32) / 900.0).exp();
            [level, -level * 0.5]
        })
        .collect();
    let color = [200, 120, 40];
    let drawings = [
        spans_png(
            &waveform_spans(&frames, PREVIEW_SAMPLE_WIDTH, PREVIEW_SAMPLE_HEIGHT),
            PREVIEW_SAMPLE_HEIGHT,
            color,
        ),
        spans_png(
            &waveform_spans(&frames, PREVIEW_WAVE_WIDTH, PREVIEW_WAVE_HEIGHT),
            PREVIEW_WAVE_HEIGHT,
            color,
        ),
    ];
    let before = [(9315, 0xf14b_3760), (783, 0x143f_a841)];
    for (png, (length, crc)) in drawings.iter().zip(before) {
        assert_eq!((png.len(), crc32(&[png])), (length, crc));
    }
}

#[test]
fn a_mark_is_a_triangle_at_the_bottom_of_a_character_with_its_tip_on_its_line() {
    for (kind, line) in [(MarkKind::Start, 2), (MarkKind::Slice, 1)] {
        let (_, pixels) = decode(&mark_png(kind, DEFAULT_CELL));
        // A third of the author's 15-pixel character either side of the line, and a
        // character tall, so it's placed in the ruler's row as it is.
        assert_eq!((pixels[0].len(), pixels.len()), (line + 10, 39));
        let lit = |row: &Vec<bool>| -> Vec<usize> { (0..row.len()).filter(|x| row[*x]).collect() };
        // The tip, at the bottom, is the line's pixels, 5 in.
        assert_eq!(lit(&pixels[38]), (5..5 + line).collect::<Vec<_>>());
        // Each row up is a pixel wider either side, to the whole width 6 rows up.
        for up in 0..=5 {
            assert_eq!(
                lit(&pixels[38 - up]),
                (5 - up..5 + line + up).collect::<Vec<_>>()
            );
        }
        assert!(
            pixels[..33].iter().flatten().all(|lit| !lit),
            "nothing above it"
        );
    }
}

/// The commands in what was sent, each as its controls, its data left out.
fn commands(sent: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(sent)
        .split("\x1b_G")
        .skip(1)
        .map(|command| command.split(';').next().unwrap_or_default().to_owned())
        .collect()
}

#[test]
fn a_switch_look_is_stored_once_and_placed_again_only_when_it_changes() {
    use ratatui::layout::Rect;

    use crate::views::widgets::toggle;

    let screen = |switches: &[(u16, bool)]| {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 2));
        buffer.set_style(buffer.area, theme::screen());
        for (x, on) in switches {
            buffer.set_span(*x, 1, &toggle::span(*on), 3);
        }
        buffer
    };
    let mut graphics = Graphics::new();
    let frame = |graphics: &mut Graphics, switches: &[(u16, bool)]| {
        graphics.find_switches(&mut screen(switches));
        let mut sent = Vec::new();
        graphics.sync_switches(&mut sent).unwrap();
        commands(&sent)
    };
    let first = frame(&mut graphics, &[(2, true), (8, true), (14, false)]);
    let stores: Vec<_> = first.iter().filter(|c| c.starts_with("a=t,")).collect();
    let places: Vec<_> = first.iter().filter(|c| c.starts_with("a=p,")).collect();
    assert_eq!((stores.len(), places.len()), (2, 3), "{first:?}");
    let on = graphics.switch_images[&Look {
        on: true,
        color: rgb(theme::GREEN),
    }];
    // Both switches that are on are one picture, placed under each one's cell.
    assert!(places[0].starts_with(&format!("a=p,i={on},p={},", (1 << 16 | 2) + 1)));
    assert!(places[1].starts_with(&format!("a=p,i={on},p={},", (1 << 16 | 8) + 1)));
    assert!(places[0].contains(&format!(",h={},", DEFAULT_CELL.1)));

    // A screen that stays the same sends nothing.
    assert!(frame(&mut graphics, &[(2, true), (8, true), (14, false)]).is_empty());

    // One flipping off takes its placement off the green picture and places the faint one,
    // already stored, and one gone is taken off.
    let flipped = frame(&mut graphics, &[(2, true), (8, false)]);
    let off = graphics.switch_images[&Look {
        on: false,
        color: rgb(theme::OVERLAY0),
    }];
    assert_eq!(
        flipped,
        [
            format!("a=d,d=i,i={on},p={},q=2", (1 << 16 | 8) + 1),
            format!("a=d,d=i,i={off},p={},q=2", (1 << 16 | 14) + 1),
            format!(
                "a=p,i={off},p={},X=0,h={},C=1,z=1,q=2",
                (1 << 16 | 8) + 1,
                DEFAULT_CELL.1
            ),
        ]
    );

    // A new character size draws the pictures again under new ids, sent as they're needed.
    graphics.use_cell((10, 20));
    assert_eq!(graphics.retired.len(), 2);
    let again = frame(&mut graphics, &[(2, true), (8, false)]);
    assert_eq!(again.iter().filter(|c| c.starts_with("a=t,")).count(), 2);
    assert_eq!(again.iter().filter(|c| c.starts_with("a=p,")).count(), 2);
}
