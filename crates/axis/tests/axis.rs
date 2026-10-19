use adsim_axis::{Axis, AxisConfig};

fn config() -> AxisConfig {
    AxisConfig {
        min: 0.0,
        max: 300.0,
        max_velocity: 100.0,
        acceleration: 500.0,
    }
}

fn ready_axis() -> Axis {
    let mut axis = Axis::new(config());
    axis.set_enabled(true);
    axis.set_homed(true);
    axis
}

#[test]
fn a_disabled_axis_does_not_move() {
    let mut axis = Axis::new(config());
    axis.command(10.0);

    for _ in 0..100 {
        axis.step(0.01);
    }

    assert_eq!(axis.position(), 0.0);
}

#[test]
fn an_enabled_homed_axis_reaches_its_target() {
    let mut axis = ready_axis();
    axis.command(10.0);

    for _ in 0..1000 {
        axis.step(0.01);
    }

    assert!((axis.position() - 10.0).abs() < 1e-6);
    assert!(!axis.faulted());
}

#[test]
fn motion_respects_the_velocity_limit() {
    let mut axis = ready_axis();
    axis.command(100.0);
    let mut last = axis.position();

    for _ in 0..1000 {
        axis.step(0.001);
        let speed = (axis.position() - last).abs() / 0.001;
        assert!(speed <= 100.0 + 1e-6, "speed {speed}");
        last = axis.position();
    }
}

#[test]
fn a_target_outside_the_soft_limits_faults() {
    let mut axis = ready_axis();
    axis.command(400.0);

    for _ in 0..100 {
        axis.step(0.01);
    }

    assert!(axis.faulted());
    assert_eq!(axis.position(), 0.0);
}

#[test]
fn reset_clears_the_fault() {
    let mut axis = ready_axis();
    axis.command(-1.0);
    assert!(axis.faulted());

    axis.reset();
    assert!(!axis.faulted());

    axis.command(5.0);
    for _ in 0..1000 {
        axis.step(0.01);
    }
    assert!((axis.position() - 5.0).abs() < 1e-6);
}

#[test]
fn an_unhomed_axis_does_not_move() {
    let mut axis = Axis::new(config());
    axis.set_enabled(true);
    axis.command(10.0);

    for _ in 0..100 {
        axis.step(0.01);
    }

    assert_eq!(axis.position(), 0.0);
}
