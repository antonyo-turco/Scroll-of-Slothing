//! Blades in the Dark style clocks: a circle cut into segments, filled by
//! hand or, optionally, one segment every N seconds of real time.
//! Spec: docs/features/clocks.md.
//!
//! Pure data and rules; times are Unix milliseconds passed in by the caller.

use serde::{Deserialize, Serialize};

pub const MIN_SEGMENTS: u32 = 2;
pub const MAX_SEGMENTS: u32 = 24;
/// Shortest auto-advance period (10 s).
pub const MIN_EVERY_SECS: u32 = 10;
/// Longest auto-advance period (24 h).
pub const MAX_EVERY_SECS: u32 = 24 * 3600;

/// Advancing on a timer: one segment every `every_secs`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoAdvance {
    pub every_secs: u32,
    /// When the timer is running, the moment the current segment started
    /// counting (minus any time carried over from before a pause).
    #[serde(default)]
    pub since_ms: Option<i64>,
    /// Time already counted toward the next segment while paused.
    #[serde(default)]
    pub carry_ms: u64,
}

impl AutoAdvance {
    pub fn new(every_secs: u32) -> Self {
        AutoAdvance { every_secs, since_ms: None, carry_ms: 0 }
    }

    pub fn running(&self) -> bool {
        self.since_ms.is_some()
    }

    fn period_ms(&self) -> i64 {
        i64::from(self.every_secs) * 1000
    }

    /// Time counted toward the next segment.
    pub fn elapsed_ms(&self, now_ms: i64) -> u64 {
        match self.since_ms {
            Some(t) => (now_ms - t).max(0) as u64,
            None => self.carry_ms,
        }
    }
}

/// Something that happens when the clock reaches a segment: at `at`, and
/// with `repeat`, again every `repeat` segments after it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClockEvent {
    pub id: String,
    /// The segment (1-based) whose filling triggers it.
    pub at: u32,
    #[serde(default)]
    pub repeat: Option<u32>,
    pub text: String,
}

impl ClockEvent {
    /// Whether filling segment `segment` (1-based) triggers this event.
    pub fn fires_at(&self, segment: u32) -> bool {
        match self.repeat {
            _ if segment == self.at => true,
            Some(r) if r > 0 && segment > self.at => (segment - self.at) % r == 0,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Clock {
    pub id: String,
    pub name: String,
    pub segments: u32,
    #[serde(default)]
    pub filled: u32,
    /// `#rrggbb`; `None` uses the accent colour.
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub auto: Option<AutoAdvance>,
    /// What happens at given segments, in the order they were written.
    #[serde(default)]
    pub events: Vec<ClockEvent>,
    pub created: String,
    pub updated: String,
}

/// What a tick did to one clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Advanced {
    pub added: u32,
    /// The clock filled with this tick (its timer stopped).
    pub full: bool,
}

pub fn valid_segments(n: u32) -> bool {
    (MIN_SEGMENTS..=MAX_SEGMENTS).contains(&n)
}

pub fn valid_every(secs: u32) -> bool {
    (MIN_EVERY_SECS..=MAX_EVERY_SECS).contains(&secs)
}

impl Clock {
    pub fn new(id: String, name: String, segments: u32, now: String) -> Self {
        Clock {
            id,
            name,
            segments: segments.clamp(MIN_SEGMENTS, MAX_SEGMENTS),
            filled: 0,
            color: None,
            notes: String::new(),
            auto: None,
            events: vec![],
            created: now.clone(),
            updated: now,
        }
    }

    /// Events triggered by filling from `from` to `to` segments, in segment
    /// order (an event repeating several times in the range is listed each time).
    pub fn events_reached(&self, from: u32, to: u32) -> Vec<(u32, &ClockEvent)> {
        let mut out = vec![];
        for seg in from.saturating_add(1)..=to.min(self.segments) {
            out.extend(self.events.iter().filter(|e| e.fires_at(seg)).map(|e| (seg, e)));
        }
        out
    }

    pub fn is_full(&self) -> bool {
        self.filled >= self.segments
    }

    pub fn running(&self) -> bool {
        self.auto.as_ref().is_some_and(AutoAdvance::running)
    }

    /// Sets the fill (clamped to the segments). A full clock's timer stops
    /// and forgets its partial segment.
    pub fn set_filled(&mut self, filled: u32) {
        self.filled = filled.min(self.segments);
        if self.is_full() {
            self.stop_full();
        }
    }

    /// Adds (or with a negative `by`, removes) segments.
    pub fn advance_by(&mut self, by: i64) {
        let f = (i64::from(self.filled) + by).clamp(0, i64::from(self.segments));
        self.set_filled(f as u32);
    }

    /// Changes the segment count; the fill is kept, clamped to the new count.
    pub fn set_segments(&mut self, segments: u32) {
        self.segments = segments.clamp(MIN_SEGMENTS, MAX_SEGMENTS);
        self.set_filled(self.filled);
    }

    /// Turns the timer on (`Some(secs)`) or off (`None`). Changing the period
    /// keeps the running state and the time counted so far.
    pub fn set_every(&mut self, every_secs: Option<u32>) {
        match (every_secs, self.auto.as_mut()) {
            (None, _) => self.auto = None,
            (Some(s), Some(a)) => a.every_secs = s,
            (Some(s), None) => self.auto = Some(AutoAdvance::new(s)),
        }
    }

    /// Starts or pauses the timer. Starting a full clock or a clock without a
    /// timer does nothing; returns whether the timer is now running.
    pub fn set_running(&mut self, run: bool, now_ms: i64) -> bool {
        let full = self.is_full();
        let Some(a) = self.auto.as_mut() else { return false };
        match (run, a.since_ms) {
            (true, None) if !full => a.since_ms = Some(now_ms - a.carry_ms as i64),
            (false, Some(_)) => {
                a.carry_ms = a.elapsed_ms(now_ms);
                a.since_ms = None;
            }
            _ => {}
        }
        a.running()
    }

    /// Empties the clock and its timer's partial segment, and pauses it.
    pub fn reset(&mut self) {
        self.filled = 0;
        if let Some(a) = self.auto.as_mut() {
            a.since_ms = None;
            a.carry_ms = 0;
        }
    }

    fn stop_full(&mut self) {
        if let Some(a) = self.auto.as_mut() {
            a.since_ms = None;
            a.carry_ms = 0;
        }
    }

    /// When the timer adds its next segment (`None` when it is not running).
    pub fn next_due_ms(&self) -> Option<i64> {
        let a = self.auto.as_ref()?;
        Some(a.since_ms? + a.period_ms())
    }

    /// Adds every segment the timer owes at `now_ms` (several after a long
    /// gap), up to full. `None` when nothing was due.
    pub fn tick(&mut self, now_ms: i64) -> Option<Advanced> {
        let left = self.segments.saturating_sub(self.filled);
        let a = self.auto.as_mut()?;
        let since = a.since_ms?;
        let period = a.period_ms();
        let due = (now_ms - since).div_euclid(period);
        if due <= 0 {
            return None;
        }
        let added = (due.min(i64::from(left))) as u32;
        a.since_ms = Some(since + i64::from(added) * period);
        self.filled += added;
        let full = self.is_full();
        if full {
            self.stop_full();
        }
        Some(Advanced { added, full })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 60_000;

    fn clock(segments: u32) -> Clock {
        Clock::new("c".into(), "Alarm".into(), segments, "t".into())
    }

    fn timed(segments: u32, every_secs: u32) -> Clock {
        let mut c = clock(segments);
        c.set_every(Some(every_secs));
        c
    }

    #[test]
    fn segments_are_clamped() {
        assert_eq!(clock(1).segments, 2);
        assert_eq!(clock(99).segments, 24);
        assert!(valid_segments(4) && !valid_segments(1) && !valid_segments(25));
    }

    #[test]
    fn hand_fill_is_clamped() {
        let mut c = clock(4);
        c.advance_by(3);
        assert_eq!(c.filled, 3);
        c.advance_by(5);
        assert_eq!(c.filled, 4);
        assert!(c.is_full());
        c.advance_by(-9);
        assert_eq!(c.filled, 0);
        c.set_filled(7);
        assert_eq!(c.filled, 4);
    }

    #[test]
    fn fewer_segments_clamp_the_fill() {
        let mut c = clock(8);
        c.set_filled(6);
        c.set_segments(4);
        assert_eq!((c.segments, c.filled), (4, 4));
    }

    #[test]
    fn timer_adds_one_segment_per_period() {
        let mut c = timed(6, 600);
        assert!(c.set_running(true, 0));
        assert_eq!(c.next_due_ms(), Some(10 * MIN));
        assert_eq!(c.tick(10 * MIN - 1), None);
        assert_eq!(c.tick(10 * MIN), Some(Advanced { added: 1, full: false }));
        assert_eq!(c.filled, 1);
        // the next segment counts from the due time, not the tick time
        assert_eq!(c.next_due_ms(), Some(20 * MIN));
        assert_eq!(c.tick(10 * MIN + 5), None);
    }

    #[test]
    fn a_late_tick_catches_up_and_keeps_the_phase() {
        let mut c = timed(8, 600);
        c.set_running(true, 0);
        assert_eq!(c.tick(35 * MIN), Some(Advanced { added: 3, full: false }));
        assert_eq!(c.next_due_ms(), Some(40 * MIN));
    }

    #[test]
    fn filling_stops_the_timer() {
        let mut c = timed(4, 600);
        c.set_filled(2);
        c.set_running(true, 0);
        assert_eq!(c.tick(60 * MIN), Some(Advanced { added: 2, full: true }));
        assert_eq!(c.filled, 4);
        assert!(!c.running());
        assert_eq!(c.tick(120 * MIN), None);
        // a full clock does not start
        assert!(!c.set_running(true, 0));
    }

    #[test]
    fn filling_by_hand_stops_the_timer() {
        let mut c = timed(4, 600);
        c.set_running(true, 0);
        c.advance_by(4);
        assert!(!c.running());
        assert_eq!(c.auto.as_ref().unwrap().carry_ms, 0);
    }

    #[test]
    fn pause_keeps_the_partial_segment() {
        let mut c = timed(6, 600);
        c.set_running(true, 0);
        assert!(!c.set_running(false, 4 * MIN));
        assert_eq!(c.auto.as_ref().unwrap().elapsed_ms(99 * MIN), (4 * MIN) as u64);
        assert_eq!(c.tick(99 * MIN), None);
        c.set_running(true, 100 * MIN);
        // 6 more minutes complete the segment
        assert_eq!(c.next_due_ms(), Some(106 * MIN));
        assert_eq!(c.tick(106 * MIN).map(|a| a.added), Some(1));
    }

    #[test]
    fn changing_the_period_keeps_the_count() {
        let mut c = timed(6, 600);
        c.set_running(true, 0);
        c.set_every(Some(300));
        assert!(c.running());
        assert_eq!(c.next_due_ms(), Some(5 * MIN));
        c.set_every(None);
        assert!(!c.running());
        assert_eq!(c.next_due_ms(), None);
    }

    #[test]
    fn reset_empties_and_pauses() {
        let mut c = timed(6, 600);
        c.set_running(true, 0);
        c.tick(25 * MIN);
        c.reset();
        assert_eq!(c.filled, 0);
        assert!(!c.running());
        assert_eq!(c.auto.as_ref().unwrap().carry_ms, 0);
    }

    fn event(at: u32, repeat: Option<u32>, text: &str) -> ClockEvent {
        ClockEvent { id: text.into(), at, repeat, text: text.into() }
    }

    #[test]
    fn events_fire_at_their_segment_and_repeat() {
        let once = event(3, None, "guards");
        assert!(once.fires_at(3) && !once.fires_at(2) && !once.fires_at(6));
        let every = event(2, Some(2), "tremor");
        assert_eq!((1..=8).filter(|&s| every.fires_at(s)).collect::<Vec<_>>(), vec![2, 4, 6, 8]);
        // a zero repeat is treated as no repeat
        assert!(!event(2, Some(0), "x").fires_at(4));
    }

    #[test]
    fn events_reached_lists_each_crossed_segment() {
        let mut c = clock(8);
        c.events = vec![event(2, Some(3), "tremor"), event(4, None, "guards")];
        let texts = |v: Vec<(u32, &ClockEvent)>| {
            v.into_iter().map(|(s, e)| format!("{s}:{}", e.text)).collect::<Vec<_>>()
        };
        assert_eq!(texts(c.events_reached(0, 5)), ["2:tremor", "4:guards", "5:tremor"]);
        assert_eq!(texts(c.events_reached(5, 5)), Vec::<String>::new());
        // emptying never triggers, and nothing fires past the last segment
        assert!(c.events_reached(5, 3).is_empty());
        assert_eq!(texts(c.events_reached(7, 99)), ["8:tremor"]);
    }

    #[test]
    fn hand_clocks_never_tick() {
        let mut c = clock(4);
        assert!(!c.set_running(true, 0));
        assert_eq!(c.tick(i64::MAX / 2), None);
    }
}
