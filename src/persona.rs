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
    (worried) => {
        "  /   \\  "
    };
    (drawn) => {
        "  \\   /  "
    };
    // The drawn brow with more weight behind it. Nothing above this.
    (furious) => {
        " \\\\   // "
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
    (narrowed) => {
        "  >   <  "
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
    (frown) => {
        "  /---\\  "
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

/// How put out the character is, worst last.
///
/// Two things drive it: how many times the reminder has been pushed back, and how long this
/// notification has sat there unanswered. They are read separately and the worse one wins, so
/// the order below is the whole rule. Being pushed back four times outranks being left on
/// screen, because the first is a habit and the second is a moment.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mood {
    /// Nothing has happened yet: blinks, glances about, and now and then turns to look.
    Idle,
    /// Already pushed back once or twice: heavy lidded.
    Waiting,
    /// On screen a while with no answer: stares.
    Ignored,
    /// Pushed back often enough that it has stopped being funny: stares, and is anxious with it.
    Worried,
    /// Pushed back well past that.
    Angry,
}

impl Mood {
    /// What to call it on screen. Only a debug build ever shows this, next to the title.
    pub fn name(self) -> &'static str {
        match self {
            Mood::Idle => "IDLE",
            Mood::Waiting => "WAITING",
            Mood::Ignored => "IGNORED",
            Mood::Worried => "WORRIED",
            Mood::Angry => "ANGRY",
        }
    }

    /// The next mood along, for the key that steps through them in a debug build. Exhaustive on
    /// purpose: a new mood will not compile until it has been given a place in the walk.
    #[cfg(debug_assertions)]
    pub fn next(self) -> Self {
        match self {
            Mood::Idle => Mood::Waiting,
            Mood::Waiting => Mood::Ignored,
            Mood::Ignored => Mood::Worried,
            Mood::Worried => Mood::Angry,
            Mood::Angry => Mood::Idle,
        }
    }
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

// Worried blinks and swallows where Ignored only stares, which is what keeps it anxious rather
// than accusing. Angry does neither.
const WORRIED: &[Frame] = &[
    frame(note!(worried, wide, small), 2600),
    frame(note!(worried, wide, open), 400),
    frame(note!(worried, wide, small), 2200),
    frame(note!(worried, shut, small), 150),
];

const ANGRY: &[Frame] = &[
    frame(note!(furious, narrowed, frown), 2200),
    frame(note!(furious, wide, frown), 200),
    frame(note!(furious, narrowed, frown), 1600),
    frame(note!(drawn, narrowed, frown), 300),
];

const fn frame(art: &'static str, millis: u64) -> Frame {
    Frame { art, millis }
}

pub fn frames(mood: Mood) -> &'static [Frame] {
    match mood {
        Mood::Idle => IDLE,
        Mood::Waiting => WAITING,
        Mood::Ignored => IGNORED,
        Mood::Worried => WORRIED,
        Mood::Angry => ANGRY,
    }
}

/// Reads the history and the wait separately, and takes whichever is worse.
pub fn mood(snoozes: u32, on_screen: Duration) -> Mood {
    let from_history = if snoozes > ANGRY_AFTER {
        Mood::Angry
    } else if snoozes > WORRIED_AFTER {
        Mood::Worried
    } else if snoozes > 0 {
        Mood::Waiting
    } else {
        Mood::Idle
    };

    let from_the_wait = if on_screen >= STARE_AFTER {
        Mood::Ignored
    } else {
        Mood::Idle
    };

    from_history.max(from_the_wait)
}

/// Pushed back more times than this before the character stops taking it well.
const WORRIED_AFTER: u32 = 3;
const ANGRY_AFTER: u32 = 5;

/// How long a notification may sit unanswered before it starts staring. Long enough to read the
/// thing and decide, rather than long enough to look away and glance back.
const STARE_AFTER: Duration = Duration::from_secs(40);

#[cfg(test)]
mod tests {
    use super::*;

    /// The rectangle every frame has to fill, so a redraw never moves anything. Every line is
    /// exactly this wide: the note is drawn as a block, and a short line would centre itself
    /// against the others and shift the drawing.
    const LINES: usize = 6;
    const COLUMNS: usize = 13;

    /// Named once so that adding a mood cannot quietly skip every check below.
    const EVERY_MOOD: [Mood; 5] = [
        Mood::Idle,
        Mood::Waiting,
        Mood::Ignored,
        Mood::Worried,
        Mood::Angry,
    ];

    #[test]
    fn every_frame_is_the_same_rectangle() {
        for mood in EVERY_MOOD {
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
        for mood in EVERY_MOOD {
            for frame in frames(mood) {
                assert!(frame.art.is_ascii(), "{}", frame.art);
            }
        }
    }

    #[test]
    fn every_mood_has_frames_that_advance() {
        for mood in EVERY_MOOD {
            let frames = frames(mood);
            assert!(frames.len() > 1);
            assert!(frames.iter().all(|frame| frame.millis > 0));
        }
    }

    /// A frame that repeats the one before it is a redraw that changes nothing.
    #[test]
    fn no_frame_repeats_the_one_before_it() {
        for mood in EVERY_MOOD {
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

    /// Keeps the debug walk honest against the list the rest of these tests use, so a new mood
    /// cannot be added to one and forgotten in the other.
    #[test]
    fn stepping_through_the_moods_visits_every_one_and_comes_back() {
        let mut mood = Mood::Idle;
        for expected in EVERY_MOOD.iter().skip(1) {
            mood = mood.next();
            assert!(mood == *expected, "the walk skipped {}", expected.name());
        }
        assert!(mood.next() == Mood::Idle, "the walk does not wrap");
    }

    #[test]
    fn the_mood_follows_how_often_it_was_pushed_back() {
        assert!(mood(0, Duration::ZERO) == Mood::Idle);
        assert!(mood(1, Duration::ZERO) == Mood::Waiting);
        assert!(mood(3, Duration::ZERO) == Mood::Waiting);
        assert!(mood(4, Duration::ZERO) == Mood::Worried);
        assert!(mood(5, Duration::ZERO) == Mood::Worried);
        assert!(mood(6, Duration::ZERO) == Mood::Angry);
    }

    #[test]
    fn sitting_unanswered_brings_on_the_stare() {
        assert!(mood(0, Duration::from_secs(39)) == Mood::Idle);
        assert!(mood(0, Duration::from_secs(40)) == Mood::Ignored);
    }

    /// The two readings never cancel out. A reminder pushed back all week does not soften by
    /// being looked at promptly, and a fresh one still hardens by being left alone.
    #[test]
    fn the_worse_of_the_two_readings_wins() {
        assert!(mood(1, Duration::from_secs(60)) == Mood::Ignored);
        assert!(mood(4, Duration::from_secs(60)) == Mood::Worried);
        assert!(mood(6, Duration::from_secs(60)) == Mood::Angry);
        assert!(mood(6, Duration::ZERO) == Mood::Angry);
    }
}
