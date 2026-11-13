use std::path::Path;

use super::*;
use crate::explorer::Purpose;
use crate::input::keys::parse;

const VIEWS: [&str; 6] = ["table", "song", "chain", "phrase", "instrument", "groove"];

/// Press keys as typed, with a named key in angle brackets, such as `<Esc>`.
fn press(picker: &mut Picker, keys: &str) -> Outcome {
    let mut outcome = Outcome::Stay;
    let mut rest = keys;
    while let Some(c) = rest.chars().next() {
        let (key, after) = match rest.find('>').filter(|_| c == '<') {
            Some(end) => (parse(&rest[1..end]).unwrap(), &rest[end + 1..]),
            None => (Key::char(c), &rest[c.len_utf8()..]),
        };
        outcome = picker.key(key);
        rest = after;
    }
    outcome
}

fn label(picker: &Picker) -> &str {
    picker.current().map_or("", |item| item.label.as_str())
}

/// `Off` on its own, Filter with three, Output with one, and Amp with two.
fn tree() -> Picker {
    let items = [
        ("Off", ""),
        ("Cut", "Filter"),
        ("Res", "Filter"),
        ("Drv", "Filter"),
        ("Pan", "Output"),
        ("Vol", "Amp"),
        ("Gain", "Amp"),
    ]
    .map(|(label, category)| Item::new(label, Pick::Track(0)).category(category))
    .to_vec();
    Picker::new("Dest", items)
}

/// A sample library: two folders of WAVs, a hidden one, and a file that isn't a sample.
fn library(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("vt-tree-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for folder in ["drums/kicks", "drums/hats", "loops", ".cache"] {
        std::fs::create_dir_all(root.join(folder)).unwrap();
    }
    for file in [
        "drums/kicks/deep.wav",
        "drums/kicks/short.wav",
        "drums/hats/closed.wav",
        "loops/break.wav",
        "notes.txt",
        ".cache/old.wav",
    ] {
        std::fs::write(root.join(file), "").unwrap();
    }
    root
}

fn explorer(root: &Path) -> Picker {
    let roots = vec![Root::Place {
        path: root.to_path_buf(),
        meaning: "samples".to_owned(),
    }];
    Picker::explorer(Purpose::Sample, roots, None)
}

/// The rows as they're shown: each indented by its depth, a folder with its arrow.
fn shown_rows(picker: &Picker) -> Vec<String> {
    picker
        .rows()
        .iter()
        .map(|row| {
            let Row::Item(item) = row else {
                return String::new();
            };
            let arrow = match picker.folder_open(*item) {
                Some(true) => "v ",
                Some(false) => "> ",
                None => "",
            };
            let indent = "  ".repeat(picker.depth(*item));
            let label = &picker.item(*item).label;
            if picker.depth(*item) == 0 {
                format!("{arrow}root")
            } else {
                format!("{indent}{arrow}{label}")
            }
        })
        .collect()
}

#[test]
fn the_explorer_opens_folders_with_l_and_closes_them_with_h() {
    let root = library("keys");
    let mut picker = explorer(&root);
    assert_eq!(shown_rows(&picker), ["> root"], "folded, nothing read");
    assert_eq!(picker.items().len(), 1);
    press(&mut picker, "l");
    assert_eq!(
        shown_rows(&picker),
        ["v root", "  > drums", "  > loops"],
        "folders first, and only samples, with hidden ones left out"
    );
    assert_eq!(label(&picker), "drums", "into the folder");
    press(&mut picker, "l");
    press(&mut picker, "l");
    assert_eq!(label(&picker), "closed.wav");
    assert_eq!(picker.depth(picker.current_index().unwrap()), 3);
    // h on a file closes the folder it's in, going up to it, and on an open folder closes it.
    press(&mut picker, "h");
    assert_eq!(label(&picker), "hats");
    assert_eq!(
        picker.folder_open(picker.current_index().unwrap()),
        Some(false)
    );
    press(&mut picker, "kh");
    assert_eq!(label(&picker), "drums");
    assert_eq!(shown_rows(&picker), ["v root", "  > drums", "  > loops"]);
    // l on a file chooses it, and Enter does too.
    press(&mut picker, "jl");
    assert_eq!(
        press(&mut picker, "l"),
        Outcome::Choose(Pick::Sample(root.join("loops/break.wav")))
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn h_lists_hidden_files_and_keeps_the_tree_as_it_was() {
    let root = library("hidden");
    let mut picker = explorer(&root);
    press(&mut picker, "ljl");
    assert_eq!(label(&picker), "break.wav");
    press(&mut picker, "H");
    assert!(picker.tree().unwrap().hidden);
    assert_eq!(
        shown_rows(&picker),
        [
            "v root",
            "  > .cache",
            "  > drums",
            "  v loops",
            "    break.wav"
        ],
        "the hidden folder, with what was open still open"
    );
    assert_eq!(label(&picker), "break.wav", "the cursor where it was");
    press(&mut picker, "H");
    assert_eq!(shown_rows(&picker).len(), 4);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn backspace_makes_the_root_its_parent_with_it_open_inside() {
    let root = library("up");
    let mut picker = explorer(&root.join("drums"));
    press(&mut picker, "l");
    assert_eq!(label(&picker), "hats");
    press(&mut picker, "<Backspace>");
    let top = picker.current_index().unwrap();
    assert_eq!(label(&picker), "hats", "the cursor stays");
    assert_eq!(picker.depth(top), 2, "a level deeper now");
    assert_eq!(
        shown_rows(&picker),
        [
            "v root",
            "  v drums",
            "    > hats",
            "    > kicks",
            "  > loops"
        ]
    );
    assert_eq!(picker.item(0).meaning, "samples", "it keeps why it's there");
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn the_explorers_search_is_exact_across_what_has_been_read() {
    let root = library("search");
    let mut picker = explorer(&root);
    press(&mut picker, "lllhh");
    assert_eq!(label(&picker), "drums");
    // `drums/hats` has been read, `drums/kicks` and `loops` haven't.
    press(&mut picker, "/cl");
    assert_eq!(
        shown_rows(&picker),
        ["v root", "  > drums", "    > hats", "      closed.wav"],
        "a match, under the folders it's in, open or not"
    );
    assert_eq!(
        label(&picker),
        "closed.wav",
        "on the match, not its folders"
    );
    assert_eq!(picker.matched(3), [0, 1]);
    press(&mut picker, "<Backspace><Backspace>csd");
    assert!(
        picker.rows().is_empty(),
        "not fuzzy: no c, s and d in a row"
    );
    press(&mut picker, "<Esc>eep");
    assert!(picker.rows().is_empty(), "kicks hasn't been read");
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn it_follows_a_file_opening_the_folders_down_to_it() {
    let root = library("follow");
    let picker = explorer(&root).following(Some(&root.join("drums/kicks/short.wav")));
    assert_eq!(label(&picker), "short.wav");
    assert_eq!(picker.marked(), picker.current_index(), "marked as chosen");
    assert_eq!(
        shown_rows(&picker),
        [
            "v root",
            "  v drums",
            "    > hats",
            "    v kicks",
            "      deep.wav",
            "      short.wav",
            "  > loops"
        ]
    );
    // A file that isn't listed goes as near as it can.
    let picker = explorer(&root).following(Some(&root.join("drums/gone.wav")));
    assert_eq!(label(&picker), "drums");
    assert_eq!(picker.marked(), None);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn p_and_a_switch_the_preview_and_playing_and_enter_chooses_a_folder_to_save_in() {
    let root = library("switches");
    let mut picker = explorer(&root);
    assert!(picker.has_previews(), "a sample has one");
    assert!(
        picker.tree().unwrap().audition,
        "and plays as it's moved onto"
    );
    press(&mut picker, "PA");
    assert!(!picker.has_previews());
    assert!(!picker.tree().unwrap().audition);
    assert!(picker.query.is_empty(), "neither searches");

    let roots = vec![Root::Place {
        path: root.clone(),
        meaning: String::new(),
    }];
    let mut picker = Picker::explorer(Purpose::Save, roots, None);
    assert!(!picker.has_previews(), "a folder has nothing to show");
    press(&mut picker, "l");
    assert_eq!(label(&picker), "drums", "l opens");
    press(&mut picker, "A");
    assert_eq!(picker.query, "A", "nothing plays, so A searches");
    press(&mut picker, "<Backspace><Esc>l");
    assert_eq!(
        press(&mut picker, "<Enter>"),
        Outcome::Choose(Pick::Folder(root.join("drums"))),
        "and Enter chooses"
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_group_of_items_given_up_front_starts_open() {
    let root = library("group");
    let items = vec![
        Item::new("BD", Pick::Track(0)),
        Item::new("SD", Pick::Track(1)),
    ];
    let roots = vec![
        Root::Group {
            label: "Kit 909".to_owned(),
            items,
        },
        Root::Place {
            path: root.clone(),
            meaning: String::new(),
        },
    ];
    let mut picker = Picker::explorer(Purpose::Sample, roots, None).revealing(Some(1));
    assert_eq!(label(&picker), "BD");
    assert_eq!(shown_rows(&picker), ["v root", "  BD", "  SD", "> root"]);
    assert_eq!(
        press(&mut picker, "j<Enter>"),
        Outcome::Choose(Pick::Track(1))
    );
    press(&mut picker, "h");
    assert_eq!(shown_rows(&picker), ["> root", "> root"]);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn categories_start_folded_and_one_of_one_item_is_the_item() {
    let picker = tree();
    assert_eq!(
        picker.rows(),
        [
            Row::Item(0),
            Row::Category(0),
            Row::Item(4),
            Row::Category(1)
        ]
    );
    assert_eq!(picker.category(0), ("Filter", 3, false));
    assert!(!picker.is_nested(4), "Pan stands alone");
}

#[test]
fn it_opens_on_the_current_value_inside_its_category() {
    let picker = tree().opened_on(Some(2));
    assert_eq!(label(&picker), "Res");
    assert_eq!(picker.marked(), Some(2));
    assert!(picker.category(0).2, "its category is open");
    assert!(!picker.category(1).2, "the others are folded");
    assert_eq!(picker.shown().count(), 5);
}

#[test]
fn l_goes_into_a_category_and_h_closes_it_from_inside() {
    let mut picker = tree();
    press(&mut picker, "j");
    assert_eq!(picker.current(), None, "on Filter's heading");
    press(&mut picker, "l");
    assert_eq!(label(&picker), "Cut");
    press(&mut picker, "j");
    assert_eq!(label(&picker), "Res");
    press(&mut picker, "h");
    assert_eq!(picker.rows()[picker.selected], Row::Category(0));
    assert!(!picker.category(0).2);
    // Enter opens one too, and h on an open heading folds it.
    press(&mut picker, "<Enter>k");
    assert!(picker.category(0).2);
    press(&mut picker, "h");
    assert!(!picker.category(0).2);
    // h at the top does nothing.
    press(&mut picker, "kh");
    assert_eq!(label(&picker), "Off");
}

#[test]
fn counts_gg_and_g_move_through_the_rows() {
    let mut picker = tree();
    press(&mut picker, "3j");
    assert_eq!(picker.selected, 3);
    press(&mut picker, "gg");
    assert_eq!(picker.selected, 0);
    press(&mut picker, "G");
    assert_eq!(picker.selected, 3);
    press(&mut picker, "2G");
    assert_eq!(picker.selected, 1);
    press(&mut picker, "<C-n>");
    assert_eq!(picker.selected, 2);
    press(&mut picker, "<C-p>");
    assert_eq!(picker.selected, 1);
    assert_eq!(picker.focus, Focus::List, "none of them type");
    assert!(picker.query.is_empty());
}

#[test]
fn a_search_flattens_the_tree_and_escape_clears_it_then_closes() {
    let mut picker = tree();
    press(&mut picker, "dr");
    assert_eq!(
        picker.focus,
        Focus::Search,
        "a letter that isn't a key searches"
    );
    assert_eq!(picker.query, "dr");
    assert_eq!(label(&picker), "Drv", "inside a folded category");
    assert!(picker.rows().iter().all(|row| matches!(row, Row::Item(_))));
    assert_eq!(picker.matched(0), [0, 1], "the letters it matched");
    // j and k type in the search, and a category's name counts.
    press(&mut picker, "<Esc>amp");
    assert_eq!(picker.matching(), 2);
    // `Esc` leaves the search for the list, keeping what it found, as `Esc` leaves Insert.
    press(&mut picker, "<Esc>");
    assert_eq!(picker.focus, Focus::List);
    assert_eq!(picker.query, "amp");
    assert_eq!(picker.matching(), 2);
    // `/` goes back to the search to change it. A name comes before a category's.
    press(&mut picker, "/<Backspace><Backspace>");
    assert_eq!(picker.query, "a");
    assert_eq!(label(&picker), "Pan", "before Amp's two");
    // Emptied, it shows the whole list.
    press(&mut picker, "<Backspace>g<Backspace><Esc>");
    assert!(picker.query.is_empty());
    assert_eq!(
        label(&picker),
        "Gain",
        "back on what it found, its category open"
    );
    assert!(picker.category(1).2);
    assert_eq!(press(&mut picker, "<Esc>"), Outcome::Close);
}

#[test]
fn enter_chooses_and_l_chooses_wider_where_it_can() {
    let items = vec![
        Item::new("909", Pick::Kit("909.ron".into())).wider(Pick::KitTracks("909.ron".into())),
        Item::new("808", Pick::Kit("808.ron".into())),
    ];
    let mut picker = Picker::new("Kits", items);
    assert_eq!(
        press(&mut picker, "L"),
        Outcome::Choose(Pick::KitTracks("909.ron".into()))
    );
    assert_eq!(
        press(&mut picker, "<Enter>"),
        Outcome::Choose(Pick::Kit("909.ron".into()))
    );
    assert_eq!(
        press(&mut picker, "jL"),
        Outcome::Choose(Pick::Kit("808.ron".into())),
        "nothing wider is the usual choice"
    );
}

#[test]
fn digits_then_enter_choose_by_number_and_zero_starts_a_search() {
    let items = (0..12)
        .map(|at| Item::new(format!("{at:02}"), Pick::Track(at)))
        .collect();
    let mut picker = Picker::new("Tracks", items);
    assert_eq!(
        press(&mut picker, "11<Enter>"),
        Outcome::Choose(Pick::Track(11))
    );
    let mut picker = Picker::new("Tracks", picker.items.clone());
    press(&mut picker, "04");
    assert_eq!(picker.query, "04");
    assert_eq!(label(&picker), "04");
}

#[test]
fn a_search_can_open_already_typed() {
    let picker = tree().searching("pan");
    assert_eq!(picker.focus, Focus::Search);
    assert_eq!(label(&picker), "Pan");
    let picker = tree().searching("");
    assert_eq!(picker.focus, Focus::Search);
    assert_eq!(picker.rows().len(), 4, "the tree, before typing");
}

#[test]
fn letters_match_in_order_only() {
    let found = rank(&VIEWS, "sg");
    assert_eq!(
        found,
        [1],
        "sg is in song, but groove has them the wrong way round"
    );
}

#[test]
fn an_empty_search_keeps_the_given_order() {
    assert_eq!(rank(&VIEWS, " "), [0, 1, 2, 3, 4, 5]);
}

#[test]
fn ties_go_to_the_earlier_row() {
    assert_eq!(rank(&["ab one", "ab two"], "ab"), [0, 1]);
    assert_eq!(rank(&["ab two", "ab one"], "ab"), [0, 1]);
}

/// A list with meanings, as a Steps slot's mode and destination lists have.
fn meant(items: &[(&str, &str)]) -> Picker {
    let items = items
        .iter()
        .map(|(label, meaning)| Item::new(*label, Pick::Track(0)).meaning(*meaning))
        .collect();
    Picker::new("Choices", items)
}

#[test]
fn each_word_of_a_search_matches_on_its_own() {
    let mut picker = meant(&[
        ("Pitch", "Pitch, with depth in semitones"),
        ("Pitch in scale", "Moves the note along the song's scale"),
        ("Cut", "Filter cutoff"),
    ]);
    for search in ["pitch in", "pi sc", "Pitch  in scale", "scale"] {
        picker.set_query(search.to_owned());
        assert_eq!(label(&picker), "Pitch in scale", "{search}");
    }
    // Its matched letters are in the name.
    picker.set_query("pi sc".to_owned());
    assert_eq!(picker.matched(0), [0, 1, 9, 10]);
}

#[test]
fn a_name_comes_before_a_meaning() {
    let mut picker = meant(&[
        ("Trig", "each note starts it at its start"),
        ("Free", "runs from the song's position"),
        ("Note", "one step on with each note"),
    ]);
    picker.set_query("note".to_owned());
    assert_eq!(label(&picker), "Note");
    assert_eq!(picker.rows().len(), 3, "Trig and Free still match, after");
    // The name itself, then one it starts, then one its words are in, then a meaning.
    let texts = [
        ("Pitches", "Pitches"),
        ("Spitch", "Spitch"),
        ("Cut", "Cut pitch"),
        ("Pitch", "Pitch"),
    ];
    assert_eq!(
        rank_with_positions(&texts, "pitch")
            .into_iter()
            .map(|(at, _)| at)
            .collect::<Vec<_>>(),
        [3, 0, 1, 2]
    );
}

#[test]
fn a_meaning_alone_still_finds_an_item() {
    let mut picker = meant(&[
        ("Random", "Any value between the two"),
        ("Walk", "Each value a step from the last"),
        ("1/f", "Pink, between random and a walk"),
    ]);
    picker.set_query("pink".to_owned());
    assert_eq!(label(&picker), "1/f");
    assert_eq!(picker.rows().len(), 1);
}

#[test]
fn the_highlight_stays_in_the_list() {
    let items = VIEWS.map(|label| Item::new(label, Pick::View(View::Phrase)));
    let mut picker = Picker::new("Views", items.to_vec());
    picker.move_highlight(-1);
    assert_eq!(picker.selected, 0);
    picker.move_highlight(100);
    assert_eq!(picker.selected, 5);
    picker.set_query("sg".to_owned());
    assert_eq!(picker.selected, 0);
    assert_eq!(picker.current().map(|i| i.label.as_str()), Some("song"));
    picker.set_query("zzz".to_owned());
    assert!(picker.current().is_none());
}

/// A list of forty items drawn sixteen rows tall, as the tallest list is.
fn long() -> Picker {
    let items = (0..40)
        .map(|at| Item::new(format!("{at:02} sound"), Pick::Track(at)))
        .collect();
    let mut picker = Picker::new("Sounds", items);
    picker.scroll_within(16);
    picker
}

/// The first row shown and the cursor's row.
fn view(picker: &Picker) -> (usize, usize) {
    (picker.first_shown(), picker.selected)
}

#[test]
fn the_list_keeps_three_rows_beyond_the_cursor_each_way_and_holds_as_it_turns() {
    let mut picker = long();
    press(&mut picker, "12j");
    assert_eq!(view(&picker), (0, 12), "three rows still show under it");
    press(&mut picker, "j");
    assert_eq!(view(&picker), (1, 13), "then it scrolls");
    press(&mut picker, "8j");
    assert_eq!(view(&picker), (9, 21));
    press(&mut picker, "k");
    assert_eq!(view(&picker), (9, 20), "turning round doesn't move it");
    press(&mut picker, "8k");
    assert_eq!(view(&picker), (9, 12), "three rows still show above it");
    press(&mut picker, "k");
    assert_eq!(view(&picker), (8, 11));
}

#[test]
fn the_margin_gives_way_at_the_lists_ends() {
    let mut picker = long();
    press(&mut picker, "G");
    assert_eq!(view(&picker), (24, 39), "no space after the last row");
    press(&mut picker, "3k");
    assert_eq!(view(&picker), (24, 36));
    press(&mut picker, "gg");
    assert_eq!(view(&picker), (0, 0));
    press(&mut picker, "<C-p>");
    assert_eq!(view(&picker), (0, 0));
    // A list shorter than its room never scrolls, and a short room keeps a smaller margin.
    let mut picker = tree();
    picker.scroll_within(16);
    press(&mut picker, "G");
    assert_eq!(view(&picker), (0, 3));
    let mut picker = long();
    picker.scroll_within(4);
    press(&mut picker, "4j");
    assert_eq!(view(&picker), (2, 4), "a margin of one in four rows");
}

#[test]
fn zz_zt_and_zb_move_the_view_round_the_cursor_without_moving_it() {
    let mut picker = long();
    press(&mut picker, "21G");
    assert_eq!(view(&picker), (8, 20));
    press(&mut picker, "zt");
    assert_eq!(view(&picker), (17, 20), "at the top but for the margin");
    press(&mut picker, "zb");
    assert_eq!(view(&picker), (8, 20), "at the bottom but for the margin");
    press(&mut picker, "zz");
    assert_eq!(view(&picker), (13, 20), "in the middle");
    press(&mut picker, "jk");
    assert_eq!(view(&picker), (13, 20), "and it stays there");
    assert_eq!(picker.focus, Focus::List, "none of them search");
    // Nothing shows before the first row.
    press(&mut picker, "gg");
    press(&mut picker, "zz");
    assert_eq!(view(&picker), (0, 0));
    press(&mut picker, "zb");
    assert_eq!(view(&picker), (0, 0));
    // But `zz` and `zt` show space after the last, as Vim does after a buffer's last line,
    // and moving keeps it until the margin needs the view back.
    press(&mut picker, "G");
    assert_eq!(view(&picker), (24, 39), "moving leaves no space after it");
    press(&mut picker, "zz");
    assert_eq!(view(&picker), (32, 39), "in the middle");
    press(&mut picker, "zt");
    assert_eq!(view(&picker), (36, 39), "at the top but for the margin");
    press(&mut picker, "k");
    assert_eq!(
        view(&picker),
        (35, 38),
        "the margin above pulls it back a row"
    );
    press(&mut picker, "zb");
    assert_eq!(view(&picker), (26, 38));
    press(&mut picker, "G");
    assert_eq!(
        view(&picker),
        (26, 39),
        "and moving doesn't push it past the end"
    );
    // A `z` waiting is let go by `Esc` rather than closing the list.
    assert_eq!(press(&mut picker, "z<Esc>"), Outcome::Stay);
}

#[test]
fn ctrl_d_and_ctrl_u_go_half_the_list() {
    let mut picker = long();
    press(&mut picker, "<C-d>");
    assert_eq!(view(&picker), (5, 8));
    press(&mut picker, "<C-d>");
    assert_eq!(
        view(&picker),
        (13, 16),
        "the view goes as far as the cursor"
    );
    press(&mut picker, "<C-u>");
    assert_eq!(view(&picker), (5, 8));
    press(&mut picker, "<C-u><C-u>");
    assert_eq!(view(&picker), (0, 0));
    press(&mut picker, "G<C-u>");
    assert_eq!(view(&picker), (19, 31), "the margin pulls the view up");
    press(&mut picker, "<C-d><C-d>");
    assert_eq!(view(&picker), (24, 39), "stopping at the end");
    // In the search too, as `C-n` and `C-p` are.
    press(&mut picker, "/sound<C-d>");
    assert_eq!(picker.query, "sound");
    assert_eq!(view(&picker), (5, 8));
}

#[test]
fn searching_and_folding_after_a_scroll_keep_the_view_on_the_list() {
    let mut picker = long();
    press(&mut picker, "G/07");
    assert_eq!(picker.rows().len(), 1);
    assert_eq!(view(&picker), (0, 0));
    press(&mut picker, "<Esc>");
    assert_eq!(picker.rows().len(), 1, "Esc keeps the search");
    assert_eq!(view(&picker), (0, 0));
    // Closing a long category from far down it.
    let mut items: Vec<Item> = (0..5)
        .map(|at| Item::new(format!("{at}"), Pick::Track(at)))
        .collect();
    items.extend((5..35).map(|at| Item::new(format!("{at}"), Pick::Track(at)).category("Many")));
    let mut picker = Picker::new("Sounds", items);
    picker.scroll_within(16);
    press(&mut picker, "Gl20j");
    assert_eq!(view(&picker), (14, 26));
    press(&mut picker, "h");
    assert_eq!(
        view(&picker),
        (0, 5),
        "on the category, with the list at its top"
    );
}
