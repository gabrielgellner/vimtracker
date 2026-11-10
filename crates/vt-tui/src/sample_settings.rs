//! The sample page's settings: whether points snap to zero crossings, and how the slice under the
//! cursor plays, its gain, pan, tune, direction and fades, or the entry's own with no slices. `K`
//! on the waveform chooses one to change, which puts the page's focus on it in the settings rows;
//! there `h` and `l` go along them, `=` `-` and `+` `_` change it by a small or a large step, `K`
//! lists its values, `x` puts it back, and `Esc` goes back to the waveform. `j` and `k` still go
//! by slice, so a setting can be set slice after slice. `>` and `<` move the slice's gain and `~`
//! reverses it from the waveform too. Each change is one undo step on the bank entry.

use vt_model::action::Action;
use vt_model::bank::{BankEntry, MOST_PAN};
use vt_model::kit::MAX_TUNE;

use crate::app::App;
use crate::input::command::MOST_SAMPLE_DB;
use crate::input::picker::{Item, Pick, Picker};
use crate::sample_page::Shown;
use crate::slicing::pan_text;
use crate::views::sample::time;

/// A setting on the sample page's settings rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleSetting {
    /// Whether points placed go to the nearest zero crossing, for the whole entry.
    Snap,
    /// The rest are the slice's own, or the entry's with no slices.
    Gain,
    Pan,
    Tune,
    Reverse,
    FadeIn,
    FadeOut,
}

impl SampleSetting {
    pub const ALL: [Self; 7] = [
        Self::Snap,
        Self::Gain,
        Self::Pan,
        Self::Tune,
        Self::Reverse,
        Self::FadeIn,
        Self::FadeOut,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Snap => "Snap",
            Self::Gain => "Gain",
            Self::Pan => "Pan",
            Self::Tune => "Tune",
            Self::Reverse => "Reverse",
            Self::FadeIn => "Fade in",
            Self::FadeOut => "Fade out",
        }
    }

    /// What it means, for its picker.
    fn meaning(self) -> &'static str {
        match self {
            Self::Snap => "Put points placed on the nearest zero crossing",
            Self::Gain => "How loud the slice plays",
            Self::Pan => "Where the slice sits, left to right",
            Self::Tune => "How far the slice is tuned, in semitones",
            Self::Reverse => "Whether the slice plays backwards",
            Self::FadeIn => "How long the slice fades in from its start",
            Self::FadeOut => "How long the slice fades out to its end",
        }
    }

    /// Whether the entry shown has it: pan is a slice's only, as the entry has the track's.
    pub fn exists_on(self, shown: &Shown) -> bool {
        self != Self::Pan || !shown.entry.edit.points.slices.is_empty()
    }

    /// The small and the large step `=` and `+` move it by, in its own units: hundredths of a dB,
    /// pan, semitones, on or off, and frames at the file's rate for a fade, a millisecond and ten.
    fn steps(self, rate: u32) -> (i32, i32) {
        match self {
            Self::Gain => (100, 600),
            Self::Pan => (1, 10),
            Self::Tune => (1, 12),
            Self::Snap | Self::Reverse => (1, 1),
            Self::FadeIn | Self::FadeOut => {
                let ms = (rate / 1000).max(1) as i32;
                (ms, 10 * ms)
            }
        }
    }

    /// The lowest and highest it goes, for a file this long.
    fn range(self, length: u64) -> (i32, i32) {
        match self {
            Self::Gain => {
                let most = (MOST_SAMPLE_DB * 100.0) as i32;
                (-most, most)
            }
            Self::Pan => (-i32::from(MOST_PAN), i32::from(MOST_PAN)),
            Self::Tune => (-i32::from(MAX_TUNE), i32::from(MAX_TUNE)),
            Self::Snap | Self::Reverse => (0, 1),
            Self::FadeIn | Self::FadeOut => (0, length.min(i32::MAX as u64) as i32),
        }
    }

    /// A value as the row says it.
    pub fn text(self, value: i32, rate: u32) -> String {
        match self {
            Self::Snap => if value != 0 { "on" } else { "off" }.to_owned(),
            Self::Gain => format!("{:+.1} dB", value as f32 / 100.0),
            Self::Pan => pan_text(value as i8),
            Self::Tune => format!("{value:+} st"),
            Self::Reverse => if value != 0 { "on" } else { "off" }.to_owned(),
            Self::FadeIn | Self::FadeOut => fade_text(value, rate),
        }
    }

    /// The values its picker lists, after the sample's own for a slice's setting.
    fn choices(self, rate: u32) -> Vec<i32> {
        match self {
            Self::Snap | Self::Reverse => vec![0, 1],
            Self::Gain => (-8..=4).map(|at| at * 300).collect(),
            Self::Pan => (-5..=5).map(|at| at * 10).collect(),
            Self::Tune => (-24..=24).collect(),
            Self::FadeIn | Self::FadeOut => [0, 1, 2, 5, 10, 20, 50, 100, 200, 500]
                .into_iter()
                .map(|ms| (u64::from(rate) * ms / 1000) as i32)
                .collect(),
        }
    }
}

/// A fade as the row says it: in milliseconds below a second, and as a time from there.
fn fade_text(frames: i32, rate: u32) -> String {
    let frames = frames.max(0) as u64;
    let ms = frames as f64 * 1000.0 / f64::from(rate.max(1));
    if ms < 1000.0 {
        format!("{ms:.0} ms")
    } else {
        time(frames, rate)
    }
}

impl Shown {
    /// The slice the cursor's settings are of: the one a frame is in, or none for an entry with no
    /// slices, whose settings are its own.
    pub fn settings_slice(&self, frame: u64) -> Option<usize> {
        (!self.entry.edit.points.slices.is_empty()).then(|| self.slice_at(frame).0)
    }

    /// A setting's value as it plays, for the slice a frame is in, and whether it's the slice's
    /// own rather than the entry's.
    pub fn setting_value(&self, setting: SampleSetting, frame: u64) -> (i32, bool) {
        let play = self.entry.edit.play;
        let entry = match setting {
            SampleSetting::Snap => i32::from(play.snap_zero),
            SampleSetting::Gain => (play.gain_db * 100.0).round() as i32,
            SampleSetting::Pan => 0,
            SampleSetting::Tune => i32::from(play.tune),
            SampleSetting::Reverse => i32::from(play.reverse),
            SampleSetting::FadeIn => play.fade_in.min(i32::MAX as u32) as i32,
            SampleSetting::FadeOut => play.fade_out.min(i32::MAX as u32) as i32,
        };
        let slice = self
            .settings_slice(frame)
            .filter(|_| setting != SampleSetting::Snap)
            .map(|number| self.entry.edit.points.slices[number].over);
        let own = slice.and_then(|over| match setting {
            SampleSetting::Snap => None,
            SampleSetting::Gain => over.gain_db.map(|db| (db * 100.0).round() as i32),
            SampleSetting::Pan => over.pan.map(i32::from),
            SampleSetting::Tune => over.tune.map(i32::from),
            SampleSetting::Reverse => over.reverse.map(i32::from),
            SampleSetting::FadeIn => over.fade_in.map(|frames| frames as i32),
            SampleSetting::FadeOut => over.fade_out.map(|frames| frames as i32),
        });
        match own {
            Some(value) => (value, true),
            // With no slices, every setting is the entry's own.
            None => (entry, slice.is_none()),
        }
    }

    /// The entry with a setting changed for the slice a frame is in, or the entry's own with no
    /// slices. None puts a slice's back as the entry's, and an entry's to its default.
    fn with_setting(&self, setting: SampleSetting, frame: u64, value: Option<i32>) -> BankEntry {
        let mut entry = self.entry.clone();
        let slice = self.settings_slice(frame);
        let play = &mut entry.edit.play;
        match (setting, slice) {
            (SampleSetting::Snap, _) => play.snap_zero = value.unwrap_or(0) != 0,
            (_, Some(number)) => {
                let over = &mut entry.edit.points.slices[number].over;
                match setting {
                    SampleSetting::Snap => {}
                    SampleSetting::Gain => over.gain_db = value.map(|at| at as f32 / 100.0),
                    SampleSetting::Pan => over.pan = value.map(|pan| pan as i8),
                    SampleSetting::Tune => over.tune = value.map(|tune| tune as i8),
                    SampleSetting::Reverse => over.reverse = value.map(|on| on != 0),
                    SampleSetting::FadeIn => over.fade_in = value.map(|frames| frames as u32),
                    SampleSetting::FadeOut => over.fade_out = value.map(|frames| frames as u32),
                }
            }
            (_, None) => {
                let value = value.unwrap_or(0);
                match setting {
                    SampleSetting::Snap | SampleSetting::Pan => {}
                    SampleSetting::Gain => play.gain_db = value as f32 / 100.0,
                    SampleSetting::Tune => play.tune = value as i8,
                    SampleSetting::Reverse => play.reverse = value != 0,
                    SampleSetting::FadeIn => play.fade_in = value as u32,
                    SampleSetting::FadeOut => play.fade_out = value as u32,
                }
            }
        }
        entry
    }

    /// The settings the rows have for the slice a frame is in.
    pub fn settings(&self) -> Vec<SampleSetting> {
        SampleSetting::ALL
            .into_iter()
            .filter(|setting| setting.exists_on(self))
            .collect()
    }
}

impl App {
    /// Put a setting's value in, for the slice under the cursor, as one undo step, and say what it
    /// is now.
    fn put_setting(&mut self, shown: &Shown, setting: SampleSetting, value: Option<i32>) {
        let cursor = self.sample_page.cursor;
        let entry = shown.with_setting(setting, cursor, value);
        if entry == shown.entry {
            let inherits = value.is_none() && shown.settings_slice(cursor).is_some();
            self.status = if inherits && setting != SampleSetting::Snap {
                format!("{} already plays as the sample's", setting.name())
            } else {
                format!("{} is already that", setting.name())
            };
            return;
        }
        let number = shown.number;
        self.apply(&Action::SetBankEntry {
            number,
            entry: Some(entry),
        });
        let Ok(now) = self.sample_shown() else {
            return;
        };
        let (value, own) = now.setting_value(setting, cursor);
        let text = setting.text(value, now.rate());
        let whose = match now.settings_slice(cursor) {
            _ if setting == SampleSetting::Snap => String::new(),
            Some(slice) => format!("Slice {slice:02X} "),
            None => format!("{} ", now.entry.title()),
        };
        let name = setting.name();
        let name = if whose.is_empty() {
            name.to_owned()
        } else {
            name.to_lowercase()
        };
        let inherited = if own { "" } else { ", as the sample" };
        self.status = format!("{whose}{name} {text}{inherited}: u to undo");
    }

    /// `=` `-` `+` `_` on the settings rows, `>` `<` for the gain: a setting up or down a small or
    /// a large step, `times` over, from what it plays as.
    pub(crate) fn change_setting(
        &mut self,
        shown: &Shown,
        setting: SampleSetting,
        up: bool,
        large: bool,
        times: u64,
    ) {
        if !setting.exists_on(shown) {
            self.status = format!("{} is a slice's: slice the sample first", setting.name());
            return;
        }
        let (value, _) = shown.setting_value(setting, self.sample_page.cursor);
        let (small, big) = setting.steps(shown.rate());
        let step = if large { big } else { small };
        let by = i64::from(step) * times.min(1000) as i64;
        let (low, high) = setting.range(shown.length());
        let to = if up {
            i64::from(value) + by
        } else {
            i64::from(value) - by
        };
        let to = to.clamp(i64::from(low), i64::from(high)) as i32;
        self.put_setting(shown, setting, Some(to));
    }

    /// `x` on the settings rows: a slice's setting back to the entry's, or the entry's to its
    /// default.
    pub(crate) fn reset_setting(&mut self, shown: &Shown, setting: SampleSetting) {
        self.put_setting(shown, setting, None);
    }

    /// `~`: the slice under the cursor, or the entry with no slices, played the other way.
    pub(crate) fn reverse_here(&mut self, shown: &Shown) {
        let (value, _) = shown.setting_value(SampleSetting::Reverse, self.sample_page.cursor);
        self.put_setting(shown, SampleSetting::Reverse, Some(1 - value.clamp(0, 1)));
    }

    /// `K` on the waveform: the settings, each with its value, to choose one to change on the
    /// rows.
    pub(crate) fn open_settings_picker(&mut self, shown: &Shown) {
        let cursor = self.sample_page.cursor;
        let items = shown
            .settings()
            .into_iter()
            .map(|setting| {
                let (value, own) = shown.setting_value(setting, cursor);
                let inherited = if own { "" } else { ", as the sample" };
                Item::new(
                    format!(
                        "{:<9}{}{inherited}",
                        setting.name(),
                        setting.text(value, shown.rate())
                    ),
                    Pick::SampleSetting(setting),
                )
                .meaning(setting.meaning())
            })
            .collect();
        let at = self
            .sample_page
            .setting
            .and_then(|setting| shown.settings().iter().position(|has| *has == setting));
        self.show_picker(Picker::new("Sample settings", items).opened_on(at.or(Some(0))));
    }

    /// `K` on the settings rows: the setting's values, and the sample's own first for a slice's.
    pub(crate) fn open_setting_values_picker(&mut self, shown: &Shown, setting: SampleSetting) {
        let cursor = self.sample_page.cursor;
        let rate = shown.rate();
        let (current, own) = shown.setting_value(setting, cursor);
        let slice = shown
            .settings_slice(cursor)
            .filter(|_| setting != SampleSetting::Snap);
        let mut items = Vec::new();
        if slice.is_some() {
            let (entry, _) = Shown {
                entry: shown.with_setting(setting, cursor, None),
                ..shown.clone()
            }
            .setting_value(setting, cursor);
            items.push(
                Item::new(
                    format!("As the sample, {}", setting.text(entry, rate)),
                    Pick::SampleValue {
                        setting,
                        value: None,
                    },
                )
                .meaning("Play as the sample does"),
            );
        }
        let first = items.len();
        let choices = setting.choices(rate);
        items.extend(choices.iter().map(|value| {
            Item::new(
                setting.text(*value, rate),
                Pick::SampleValue {
                    setting,
                    value: Some(*value),
                },
            )
        }));
        let at = if own || slice.is_none() {
            choices
                .iter()
                .position(|value| *value == current)
                .map(|at| first + at)
        } else {
            Some(0)
        };
        self.show_picker(Picker::new("Sample setting", items).opened_on(at));
    }

    /// A setting chosen from `K` on the waveform: the focus goes to it on the rows.
    pub(crate) fn pick_sample_setting(&mut self, setting: SampleSetting) {
        self.sample_page.setting = Some(setting);
        self.status = format!(
            "{}: = - + _ change it, K lists, x puts it back, h l the next, Esc the waveform",
            setting.name()
        );
    }

    /// A value chosen from `K` on the rows.
    pub(crate) fn pick_sample_value(&mut self, setting: SampleSetting, value: Option<i32>) {
        if let Ok(shown) = self.sample_page_here() {
            self.put_setting(&shown, setting, value);
        }
    }

    /// `h` and `l` on the rows: the setting before or after, stopping at either end.
    pub(crate) fn move_setting(&mut self, shown: &Shown, later: bool, times: usize) {
        let settings = shown.settings();
        let Some(here) = self
            .sample_page
            .setting
            .and_then(|setting| settings.iter().position(|has| *has == setting))
        else {
            self.sample_page.setting = settings.first().copied();
            return;
        };
        let to = if later {
            here.saturating_add(times).min(settings.len() - 1)
        } else {
            here.saturating_sub(times)
        };
        self.sample_page.setting = Some(settings[to]);
    }
}
