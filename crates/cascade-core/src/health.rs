use crate::types::Impact;

/// Debounces a per-window health signal so one bad (or good) window doesn't flap.
/// Escalates after `raise_after` consecutive worse windows, to the mildest level
/// seen in the streak; clears after `clear_after` consecutive better windows, to
/// the worst level seen in the streak.
#[derive(Debug, Clone)]
pub struct HealthTracker {
    raise_after: u32,
    clear_after: u32,
    current: Impact,
    up: (u32, Impact),
    down: (u32, Impact),
}

impl HealthTracker {
    pub fn new(raise_after: u32, clear_after: u32) -> Self {
        Self {
            raise_after: raise_after.max(1),
            clear_after: clear_after.max(1),
            current: Impact::None,
            up: (0, Impact::Down),
            down: (0, Impact::None),
        }
    }

    pub fn current(&self) -> Impact {
        self.current
    }

    pub fn observe(&mut self, level: Impact) -> Impact {
        if level > self.current {
            self.down = (0, Impact::None);
            self.up = (self.up.0 + 1, self.up.1.min(level));
            if self.up.0 >= self.raise_after {
                self.current = self.up.1;
                self.up = (0, Impact::Down);
            }
        } else if level < self.current {
            self.up = (0, Impact::Down);
            self.down = (self.down.0 + 1, self.down.1.max(level));
            if self.down.0 >= self.clear_after {
                self.current = self.down.1;
                self.down = (0, Impact::None);
            }
        } else {
            self.up = (0, Impact::Down);
            self.down = (0, Impact::None);
        }
        self.current
    }
}
