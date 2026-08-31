//! The character on a notification: a few lines of text, animated by swapping frames.
//!
//! Every frame is the same width and height, so redrawing one never moves anything around it.
//! The body is one shape and only the eyes, mouth and feet change, which is what makes adding
//! an expression cheap.

use std::time::Duration;

/// Assembles a frame from the three parts that ever differ.
macro_rules! character {
    ($eyes:expr, $mouth:expr, $feet:expr) => {
        concat!(
            " .-----.\n | ",
            $eyes,
            " |\n |  ",
            $mouth,
            "  |\n '-----'\n  ",
            $feet
        )
    };
    ($eyes:expr, $mouth:expr) => {
        character!($eyes, $mouth, "/   \\")
    };
}

pub struct Frame {
    pub art: &'static str,
    pub millis: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    /// Nothing has happened yet: blinks and looks about.
    Idle,
    /// Already pushed back at least once: heavy lidded.
    Waiting,
    /// On screen a while with no answer: stares.
    Ignored,
}

const IDLE: &[Frame] = &[
    frame(character!("o o", "-"), 2600),
    frame(character!("- -", "-"), 130),
    frame(character!("o o", "-"), 300),
    frame(character!("- -", "-"), 130),
    frame(character!("o o", "-"), 2000),
    frame(character!("oo ", "-"), 900),
    frame(character!("o o", "-"), 700),
    frame(character!(" oo", "-"), 900),
    frame(character!("o o", "-"), 1600),
];

const WAITING: &[Frame] = &[
    frame(character!("- -", "."), 1700),
    frame(character!("o o", "."), 500),
    frame(character!("- -", "."), 2400),
    frame(character!("- -", "_"), 900),
];

const IGNORED: &[Frame] = &[
    frame(character!("O O", "_"), 3800),
    frame(character!("O O", "_", "\\   /"), 200),
    frame(character!("o o", "_"), 260),
    frame(character!("O O", "_"), 3000),
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

    /// The shape every frame has to keep, so a redraw never moves anything.
    const LINES: usize = 5;
    const COLUMNS: usize = 9;

    #[test]
    fn every_frame_is_the_same_shape() {
        for mood in [Mood::Idle, Mood::Waiting, Mood::Ignored] {
            for frame in frames(mood) {
                let lines: Vec<&str> = frame.art.lines().collect();
                assert_eq!(lines.len(), LINES, "{}", frame.art);
                for line in lines {
                    assert!(
                        line.chars().count() <= COLUMNS,
                        "{line:?} is wider than {COLUMNS} columns"
                    );
                }
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

    #[test]
    fn the_mood_follows_the_history() {
        assert!(mood(0, Duration::ZERO) == Mood::Idle);
        assert!(mood(2, Duration::ZERO) == Mood::Waiting);
        assert!(mood(0, Duration::from_secs(30)) == Mood::Ignored);
        assert!(mood(2, Duration::from_secs(30)) == Mood::Ignored);
    }
}
