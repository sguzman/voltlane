#[must_use]
pub fn ticks_to_seconds(ticks: u64, bpm: f64, ppq: u16) -> f64 {
    if bpm <= 0.0 || ppq == 0 {
        return 0.0;
    }

    let beats = ticks as f64 / f64::from(ppq);
    beats * (60.0 / bpm)
}

#[must_use]
pub fn seconds_to_ticks(seconds: f64, bpm: f64, ppq: u16) -> u64 {
    if seconds <= 0.0 || bpm <= 0.0 || ppq == 0 {
        return 0;
    }

    let beats = seconds * (bpm / 60.0);
    (beats * f64::from(ppq)).round() as u64
}

#[must_use]
pub fn ticks_to_samples(ticks: u64, bpm: f64, ppq: u16, sample_rate: u32) -> u64 {
    let seconds = ticks_to_seconds(ticks, bpm, ppq);
    (seconds * f64::from(sample_rate)).round() as u64
}

#[must_use]
pub fn samples_to_ticks(samples: u64, bpm: f64, ppq: u16, sample_rate: u32) -> u64 {
    if sample_rate == 0 {
        return 0;
    }

    let seconds = samples as f64 / f64::from(sample_rate);
    seconds_to_ticks(seconds, bpm, ppq)
}

#[must_use]
pub fn tracker_rows_to_ticks(rows: u32, lines_per_beat: u16, ppq: u16) -> u64 {
    if lines_per_beat == 0 {
        return 0;
    }

    let ticks_per_row = f64::from(ppq) / f64::from(lines_per_beat);
    (f64::from(rows) * ticks_per_row).round() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_round_trip_is_stable() {
        let bpm = 128.0;
        let ppq = 480;
        let ticks = 9_876;
        let seconds = ticks_to_seconds(ticks, bpm, ppq);
        let restored = seconds_to_ticks(seconds, bpm, ppq);
        assert_eq!(ticks, restored);
    }

    #[test]
    fn sample_tick_round_trip_is_stable() {
        let bpm = 140.0;
        let ppq = 480;
        let sample_rate = 48_000;
        let ticks = 19_200;
        let samples = ticks_to_samples(ticks, bpm, ppq, sample_rate);
        let restored = samples_to_ticks(samples, bpm, ppq, sample_rate);
        assert_eq!(ticks, restored);
    }

    #[test]
    fn tracker_rows_convert_to_ticks() {
        let ticks = tracker_rows_to_ticks(16, 4, 480);
        assert_eq!(ticks, 1_920);
    }
}
