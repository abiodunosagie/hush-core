//! Sleep journeys: the pulse rate walks down in stages, gliding from one to the next.

/// One stage: glide into `beat` (over up to [`GLIDE_SECONDS`]) and hold it. `minutes` includes the glide.
/// The last stage holds until stopped.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JourneyStep {
    pub beat: f32,
    pub minutes: f32,
}

/// Seconds spent gliding from one stage's rate to the next, so the change is never abrupt.
pub const GLIDE_SECONDS: f32 = 120.0;

pub const WIND_DOWN: [JourneyStep; 4] = [
    JourneyStep {
        beat: 10.0,
        minutes: 10.0,
    },
    JourneyStep {
        beat: 6.0,
        minutes: 15.0,
    },
    JourneyStep {
        beat: 3.0,
        minutes: 20.0,
    },
    JourneyStep {
        beat: 1.5,
        minutes: 30.0,
    },
];

/// Pulse rate at `elapsed` seconds into the journey, or `None` for an empty journey.
pub fn beat_at(elapsed: f32, steps: &[JourneyStep]) -> Option<f32> {
    let first = steps.first()?;
    let mut start = 0.0;
    let mut previous = first.beat;
    for (i, step) in steps.iter().enumerate() {
        let length = step.minutes.max(0.0) * 60.0;
        let is_last = i == steps.len() - 1;
        if elapsed < start + length || is_last {
            if i == 0 {
                return Some(step.beat);
            }
            let into = (elapsed - start).max(0.0);
            let glide = GLIDE_SECONDS.min(length.max(1.0));
            let t = (into / glide).min(1.0);
            return Some(previous + (step.beat - previous) * t);
        }
        start += length;
        previous = step.beat;
    }
    // The last stage always returns inside the loop.
    Some(first.beat)
}
