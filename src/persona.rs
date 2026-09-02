//! The character on a notification: a note pinned to the screen, animated by swapping frames.
//!
//! Every frame is the same rectangle of characters, so redrawing one never moves anything around
//! it. The note itself never changes. Only the brows, the eyes and the mouth differ, which is
//! what makes a new expression cost three names rather than a new drawing.
//!
//! The brows are blank at rest, so a settled note carries only eyes and a mouth. That is
//! deliberate: a face that has been level all along and then draws its brows together says
//! something a permanently drawn brow cannot.

use std::time::Duration;

// The three parts that ever differ. Each is nine characters wide and sits in the same nine
// columns of the note, so the eyes line up under the brows and over the mouth by construction.
//
// These are macros rather than constants because `concat!` takes literals, and assembling the
// frames at compile time is what keeps a frame a `&'static str`.

macro_rules! brows {
    (level) => {
        "         "
    };
    (raised) => {
        "  ^   ^  "
    };
    (cocked) => {
        "  ^   _  "
    };
    (drawn) => {
        "  \\   /  "
    };
    (tired) => {
        "  ~   ~  "
    };
}

macro_rules! eyes {
    (open) => {
        "  o   o  "
    };
    (shut) => {
        "  -   -  "
    };
    (wide) => {
        "  O   O  "
    };
    (huge) => {
        "  @   @  "
    };
    // One column off centre reads as a glance, two as the head turning.
    (left) => {
        " o   o   "
    };
    (right) => {
        "   o   o "
    };
    (turned_left) => {
        "o   o    "
    };
    (turned_right) => {
        "    o   o"
    };
}

macro_rules! mouth {
    (flat) => {
        "    -    "
    };
    (small) => {
        "    .    "
    };
    (open) => {
        "   ___   "
    };
    (grim) => {
        "   ---   "
    };
}

/// Assembles a frame: a pinned note with the three parts dropped into it.
macro_rules! note {
    ($brows:ident, $eyes:ident, $mouth:ident) => {
        concat!(
            "     (o)     \n,-----------,\n| ",
            brows!($brows),
            " |\n| ",
            eyes!($eyes),
            " |\n| ",
            mouth!($mouth),
            " |\n'~v~v~v~v~v~'"
        )
    };
}

pub struct Frame {
    pub art: &'static str,
    pub millis: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    /// Nothing has happened yet: blinks, glances about, and now and then turns to look.
    Idle,
    /// Already pushed back at least once: heavy lidded.
    Waiting,
    /// On screen a while with no answer: stares.
    Ignored,
}

const IDLE: &[Frame] = &[
    frame(note!(level, open, flat), 2600),
    frame(note!(level, shut, flat), 130),
    frame(note!(level, open, flat), 300),
    frame(note!(level, shut, flat), 130),
    frame(note!(level, open, flat), 2000),
    frame(note!(level, left, flat), 900),
    frame(note!(level, turned_left, flat), 1300),
    frame(note!(raised, turned_left, small), 700),
    frame(note!(level, open, flat), 1500),
    frame(note!(level, shut, flat), 130),
    frame(note!(level, open, flat), 1800),
    frame(note!(level, right, flat), 900),
    frame(note!(level, turned_right, flat), 1300),
    frame(note!(cocked, turned_right, small), 800),
];

const WAITING: &[Frame] = &[
    frame(note!(tired, shut, small), 1700),
    frame(note!(tired, open, small), 500),
    frame(note!(tired, shut, small), 2400),
    frame(note!(tired, shut, open), 900),
];

const IGNORED: &[Frame] = &[
    frame(note!(drawn, wide, grim), 3800),
    frame(note!(drawn, huge, open), 220),
    frame(note!(drawn, wide, open), 300),
];

const fn frame(art: &'static str, millis: u64) -> Frame {
    Frame { art, millis }
}

pub fn frames(mood: Mood) -> &'static [Frame] {
    match mood {
        Mood::Idle => IDLE,
        Mood::Waiting => WAITING,
        Mood::Ignored => IGNORED,
    }
}

/// How it looks: put out by having been pushed back, and increasingly pointed about being
/// left on screen unanswered.
pub fn mood(snoozes: u32, on_screen: Duration) -> Mood {
    if on_screen >= STARE_AFTER {
        Mood::Ignored
    } else if snoozes > 0 {
        Mood::Waiting
    } else {
        Mood::Idle
    }
}

const STARE_AFTER: Duration = Duration::from_secs(25);

#[cfg(test)]
mod tests {
    use super::*;

    /// The rectangle every frame has to fill, so a redraw never moves anything. Every line is
    /// exactly this wide: the note is drawn as a block, and a short line would centre itself
    /// against the others and shift the drawing.
    const LINES: usize = 6;
    const COLUMNS: usize = 13;

    #[test]
    fn every_frame_is_the_same_rectangle() {
        for mood in [Mood::Idle, Mood::Waiting, Mood::Ignored] {
            for frame in frames(mood) {
                let lines: Vec<&str> = frame.art.lines().collect();
                assert_eq!(lines.len(), LINES, "{}", frame.art);
                for line in lines {
                    assert_eq!(
                        line.chars().count(),
                        COLUMNS,
                        "{line:?} is not {COLUMNS} columns"
                    );
                }
            }
        }
    }

    /// Nothing outside Consolas: a glyph it lacks is fetched from another font and then measured
    /// at a width it is not drawn at, which is what pushed the tick off the Done button.
    #[test]
    fn every_frame_is_plain_ascii() {
        for mood in [Mood::Idle, Mood::Waiting, Mood::Ignored] {
            for frame in frames(mood) {
                assert!(frame.art.is_ascii(), "{}", frame.art);
            }
        }
    }

    #[test]
    fn every_mood_has_frames_that_advance() {
        for mood in [Mood::Idle, Mood::Waiting, Mood::Ignored] {
            let frames = frames(mood);
            assert!(frames.len() > 1);
            assert!(frames.iter().all(|frame| frame.millis > 0));
        }
    }

    /// A frame that repeats the one before it is a redraw that changes nothing.
    #[test]
    fn no_frame_repeats_the_one_before_it() {
        for mood in [Mood::Idle, Mood::Waiting, Mood::Ignored] {
            let frames = frames(mood);
            for pair in frames.windows(2) {
                assert_ne!(pair[0].art, pair[1].art);
            }
            assert_ne!(
                frames[frames.len() - 1].art,
                frames[0].art,
                "the loop repeats where it wraps"
            );
        }
    }

    #[test]
    fn the_mood_follows_the_history() {
        assert!(mood(0, Duration::ZERO) == Mood::Idle);
        assert!(mood(2, Duration::ZERO) == Mood::Waiting);
        assert!(mood(0, Duration::from_secs(30)) == Mood::Ignored);
        assert!(mood(2, Duration::from_secs(30)) == Mood::Ignored);
    }
}
