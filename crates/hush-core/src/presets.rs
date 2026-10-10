//! Named tones and beat presets. Names are the traditional labels people search for, not health claims.

/// A named tone: the Solfeggio set plus 432 Hz.
#[derive(Clone, Copy, Debug)]
pub struct ToneChoice {
    pub hz: f32,
    pub name: &'static str,
}

pub const TONES: [ToneChoice; 10] = [
    ToneChoice {
        hz: 174.0,
        name: "Foundation",
    },
    ToneChoice {
        hz: 285.0,
        name: "Restore",
    },
    ToneChoice {
        hz: 396.0,
        name: "Release",
    },
    ToneChoice {
        hz: 417.0,
        name: "Change",
    },
    ToneChoice {
        hz: 432.0,
        name: "Verdi tuning",
    },
    ToneChoice {
        hz: 528.0,
        name: "Love",
    },
    ToneChoice {
        hz: 639.0,
        name: "Connect",
    },
    ToneChoice {
        hz: 741.0,
        name: "Clarity",
    },
    ToneChoice {
        hz: 852.0,
        name: "Third eye",
    },
    ToneChoice {
        hz: 963.0,
        name: "Crown",
    },
];

pub const DEFAULT_TONE_HZ: f32 = 852.0;

/// A carrier frequency and a beat rate.
#[derive(Clone, Copy, Debug)]
pub struct BeatPreset {
    pub name: &'static str,
    pub carrier: f32,
    pub beat: f32,
}

pub const BINAURAL: [BeatPreset; 15] = [
    BeatPreset {
        name: "Deep sleep",
        carrier: 136.0,
        beat: 1.0,
    },
    BeatPreset {
        name: "Sleep",
        carrier: 150.0,
        beat: 2.0,
    },
    BeatPreset {
        name: "Restful",
        carrier: 174.0,
        beat: 3.0,
    },
    BeatPreset {
        name: "Drowsy",
        carrier: 200.0,
        beat: 4.0,
    },
    BeatPreset {
        name: "Deep relax",
        carrier: 200.0,
        beat: 5.0,
    },
    BeatPreset {
        name: "Meditation",
        carrier: 210.0,
        beat: 6.0,
    },
    BeatPreset {
        name: "Daydream",
        carrier: 220.0,
        beat: 7.0,
    },
    BeatPreset {
        name: "Calm",
        carrier: 220.0,
        beat: 8.0,
    },
    BeatPreset {
        name: "Relaxed focus",
        carrier: 250.0,
        beat: 10.0,
    },
    BeatPreset {
        name: "Light calm",
        carrier: 250.0,
        beat: 12.0,
    },
    BeatPreset {
        name: "Alert",
        carrier: 300.0,
        beat: 14.0,
    },
    BeatPreset {
        name: "Focus",
        carrier: 300.0,
        beat: 18.0,
    },
    BeatPreset {
        name: "Energy",
        carrier: 300.0,
        beat: 25.0,
    },
    BeatPreset {
        name: "Gamma",
        carrier: 400.0,
        beat: 40.0,
    },
    BeatPreset {
        name: "852 Theta",
        carrier: 852.0,
        beat: 6.0,
    },
];

pub const ISOCHRONIC: [BeatPreset; 5] = [
    BeatPreset {
        name: "Sleep pulse",
        carrier: 200.0,
        beat: 2.0,
    },
    BeatPreset {
        name: "Meditate pulse",
        carrier: 300.0,
        beat: 6.0,
    },
    BeatPreset {
        name: "Calm pulse",
        carrier: 400.0,
        beat: 10.0,
    },
    BeatPreset {
        name: "Focus pulse",
        carrier: 400.0,
        beat: 15.0,
    },
    BeatPreset {
        name: "Gamma pulse",
        carrier: 400.0,
        beat: 40.0,
    },
];

/// Standard EEG band names, used only as labels for a pulse rate.
pub fn band_name(hz: f32) -> &'static str {
    match hz {
        h if h < 4.0 => "Delta",
        h if h < 8.0 => "Theta",
        h if h < 13.0 => "Alpha",
        h if h < 30.0 => "Beta",
        _ => "Gamma",
    }
}
