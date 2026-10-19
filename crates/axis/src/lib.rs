//! Single linear axis with soft limits and a trapezoid velocity profile.

pub struct AxisConfig {
    pub min: f64,
    pub max: f64,
    pub max_velocity: f64,
    pub acceleration: f64,
}

pub struct Axis {
    config: AxisConfig,
    position: f64,
    target: f64,
    velocity: f64,
    enabled: bool,
    homed: bool,
    faulted: bool,
}

impl Axis {
    pub fn new(config: AxisConfig) -> Self {
        Self {
            config,
            position: 0.0,
            target: 0.0,
            velocity: 0.0,
            enabled: false,
            homed: false,
            faulted: false,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn set_homed(&mut self, homed: bool) {
        self.homed = homed;
    }

    pub fn command(&mut self, target: f64) {
        if target < self.config.min || target > self.config.max {
            self.faulted = true;
            self.target = self.position;
            self.velocity = 0.0;
            return;
        }
        self.target = target;
    }

    pub fn step(&mut self, dt: f64) {
        if !self.enabled || !self.homed || self.faulted || dt <= 0.0 {
            self.velocity = 0.0;
            return;
        }

        let error = self.target - self.position;
        let distance = error.abs();
        if distance <= 1e-9 {
            self.position = self.target;
            self.velocity = 0.0;
            return;
        }

        let direction = error.signum();
        let stopping_distance = self.velocity * self.velocity / (2.0 * self.config.acceleration);
        let speed = if distance <= stopping_distance {
            (2.0 * self.config.acceleration * distance).sqrt()
        } else {
            self.config.max_velocity
        };
        let desired = direction * speed.min(self.config.max_velocity);

        let max_velocity_delta = self.config.acceleration * dt;
        self.velocity += (desired - self.velocity).clamp(-max_velocity_delta, max_velocity_delta);

        let step = self.velocity * dt;
        if step.abs() >= distance {
            self.position = self.target;
            self.velocity = 0.0;
        } else {
            self.position += step;
        }
    }

    pub fn position(&self) -> f64 {
        self.position
    }

    pub fn faulted(&self) -> bool {
        self.faulted
    }

    pub fn reset(&mut self) {
        self.faulted = false;
        self.velocity = 0.0;
        self.target = self.position;
    }
}
