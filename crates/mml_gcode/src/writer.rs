use std::fmt::Write;

/// Emits absolute millimetre G-code and drops words that are already
/// modal, which roughly halves raster programs compared to MaxmakeLAB.
#[derive(Debug)]
pub struct GcodeWriter {
    out: String,
    position: [Option<f64>; 3],
    motion: Option<u8>,
    feed: Option<f64>,
    power: Option<f64>,
    precision: usize,
}

impl Default for GcodeWriter {
    fn default() -> Self {
        Self::new()
    }
}

fn changed(current: Option<f64>, value: f64, epsilon: f64) -> bool {
    current.is_none_or(|current| (current - value).abs() > epsilon)
}

impl GcodeWriter {
    pub fn new() -> Self {
        Self {
            out: String::with_capacity(64 * 1024),
            position: [None; 3],
            motion: None,
            feed: None,
            power: None,
            precision: 3,
        }
    }

    pub fn finish(self) -> String {
        self.out
    }

    pub fn len(&self) -> usize {
        self.out.len()
    }

    pub fn is_empty(&self) -> bool {
        self.out.is_empty()
    }

    pub fn comment(&mut self, text: &str) {
        let _ = writeln!(self.out, "; {}", text.replace(['\n', '\r'], " "));
    }

    /// Raw lines (user start / end scripts); modal state is forgotten
    /// because the script may change anything.
    pub fn raw(&mut self, text: &str) {
        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            self.out.push_str(line);
            self.out.push('\n');
        }
        self.position = [None; 3];
        self.motion = None;
        self.feed = None;
        self.power = None;
    }

    pub fn preamble(&mut self) {
        self.raw("G90 G21 G17 G94");
    }

    fn number(&mut self, letter: char, value: f64) {
        let value = if value.abs() < 0.5 * 10f64.powi(-(self.precision as i32)) {
            0.0
        } else {
            value
        };
        let text = format!("{value:.precision$}", precision = self.precision);
        let text = text.trim_end_matches('0').trim_end_matches('.');
        let _ = write!(self.out, "{letter}{text}");
    }

    fn motion_word(&mut self, code: u8) {
        if self.motion != Some(code) {
            let _ = write!(self.out, "G{code}");
            self.motion = Some(code);
        }
    }

    fn axes(&mut self, target: [Option<f64>; 3]) -> bool {
        let epsilon = 0.5 * 10f64.powi(-(self.precision as i32));
        let mut written = false;
        for (index, letter) in ['X', 'Y', 'Z'].into_iter().enumerate() {
            if let Some(value) = target[index]
                && changed(self.position[index], value, epsilon)
            {
                self.number(letter, value);
                self.position[index] = Some(value);
                written = true;
            }
        }
        written
    }

    fn end_line(&mut self, mark: usize) {
        if self.out.len() > mark {
            self.out.push('\n');
        }
    }

    pub fn rapid(&mut self, x: f64, y: f64) {
        self.rapid_to([Some(x), Some(y), None]);
    }

    pub fn rapid_z(&mut self, z: f64) {
        self.rapid_to([None, None, Some(z)]);
    }

    fn rapid_to(&mut self, target: [Option<f64>; 3]) {
        let mark = self.out.len();
        let previous_motion = self.motion;
        self.motion_word(0);
        if !self.axes(target) {
            self.out.truncate(mark);
            self.motion = previous_motion;
            return;
        }
        self.end_line(mark);
    }

    /// Feed move; `power` is the S word, written only when it changes.
    pub fn linear(&mut self, x: f64, y: f64, feed: f64, power: Option<f64>) {
        self.linear_to([Some(x), Some(y), None], feed, power);
    }

    pub fn linear_z(&mut self, z: f64, feed: f64) {
        self.linear_to([None, None, Some(z)], feed, None);
    }

    pub fn linear_xyz(&mut self, x: f64, y: f64, z: f64, feed: f64) {
        self.linear_to([Some(x), Some(y), Some(z)], feed, None);
    }

    fn linear_to(&mut self, target: [Option<f64>; 3], feed: f64, power: Option<f64>) {
        let mark = self.out.len();
        let previous_motion = self.motion;
        self.motion_word(1);
        if !self.axes(target) {
            self.out.truncate(mark);
            self.motion = previous_motion;
            if let Some(power) = power {
                self.set_power(power);
            }
            return;
        }
        if changed(self.feed, feed, 1e-6) {
            self.number('F', feed);
            self.feed = Some(feed);
        }
        if let Some(power) = power
            && changed(self.power, power, 1e-6)
        {
            self.number('S', power);
            self.power = Some(power);
        }
        self.end_line(mark);
    }

    fn set_power(&mut self, power: f64) {
        if changed(self.power, power, 1e-6) {
            self.number('S', power);
            self.power = Some(power);
            self.out.push('\n');
        }
    }

    /// `dynamic` selects M4: GRBL then scales power with the real speed,
    /// which avoids dark corners when the head decelerates.
    pub fn laser_on(&mut self, dynamic: bool) {
        self.out
            .push_str(if dynamic { "M4 S0\n" } else { "M3 S0\n" });
        self.power = Some(0.0);
    }

    pub fn spindle_on(&mut self, speed: f64) {
        self.out.push_str("M3");
        self.number('S', speed);
        self.out.push('\n');
        self.power = Some(speed);
    }

    pub fn tool_off(&mut self) {
        self.out.push_str("M5\n");
        self.power = None;
    }

    pub fn air(&mut self, on: bool) {
        self.out.push_str(if on { "M8\n" } else { "M9\n" });
    }

    pub fn dwell(&mut self, seconds: f64) {
        self.out.push_str("G4");
        self.number('P', seconds);
        self.out.push('\n');
    }

    pub fn program_end(&mut self) {
        self.out.push_str("M2\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modal_words_are_not_repeated() {
        let mut writer = GcodeWriter::new();
        writer.rapid(0.0, 0.0);
        writer.linear(10.0, 0.0, 600.0, Some(500.0));
        writer.linear(10.0, 5.0, 600.0, Some(500.0));
        writer.linear(10.0, 5.0, 600.0, Some(0.0));
        writer.rapid(10.0, 5.0);
        assert_eq!(writer.finish(), "G0X0Y0\nG1X10F600S500\nY5\nS0\n");
    }
}
