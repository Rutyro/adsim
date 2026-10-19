use adsim_plc::GantryCycle;
use adsim_symbols::SymbolTable;

fn ready(table: &mut SymbolTable) {
    for axis in ["X", "Y", "Z"] {
        table
            .write_bool(&format!("MAIN.stAxis{axis}.bEnabled"), true)
            .expect("enable");
        table
            .write_bool(&format!("MAIN.stAxis{axis}.bHomed"), true)
            .expect("home");
    }
}

#[test]
fn disabled_axes_do_not_move() {
    let mut table = SymbolTable::gantry();
    let mut cycle = GantryCycle::new();
    table
        .write_f64("MAIN.stAxisX.fSetPos", 10.0)
        .expect("setpoint");

    for _ in 0..100 {
        cycle.step(&mut table, 0.01);
    }

    assert_eq!(table.read_f64("MAIN.stAxisX.fActPos"), Some(0.0));
}

#[test]
fn an_enabled_axis_follows_the_setpoint() {
    let mut table = SymbolTable::gantry();
    let mut cycle = GantryCycle::new();
    ready(&mut table);
    table
        .write_f64("MAIN.stAxisX.fSetPos", 10.0)
        .expect("setpoint");

    for _ in 0..1000 {
        cycle.step(&mut table, 0.01);
    }

    let position = table.read_f64("MAIN.stAxisX.fActPos").expect("position");
    assert!((position - 10.0).abs() < 1e-6);
    assert_eq!(table.read_i32("MAIN.stAxisX.nError"), Some(0));
}

#[test]
fn an_out_of_range_setpoint_sets_the_error_and_stops() {
    let mut table = SymbolTable::gantry();
    let mut cycle = GantryCycle::new();
    ready(&mut table);
    table
        .write_f64("MAIN.stAxisX.fSetPos", 400.0)
        .expect("setpoint");

    for _ in 0..100 {
        cycle.step(&mut table, 0.01);
    }

    assert_eq!(table.read_i32("MAIN.stAxisX.nError"), Some(1));
    assert_eq!(table.read_f64("MAIN.stAxisX.fActPos"), Some(0.0));
}

#[test]
fn reset_clears_the_error_after_the_setpoint_is_fixed() {
    let mut table = SymbolTable::gantry();
    let mut cycle = GantryCycle::new();
    ready(&mut table);
    table
        .write_f64("MAIN.stAxisX.fSetPos", 400.0)
        .expect("setpoint");
    cycle.step(&mut table, 0.01);
    assert_eq!(table.read_i32("MAIN.stAxisX.nError"), Some(1));

    table
        .write_f64("MAIN.stAxisX.fSetPos", 20.0)
        .expect("setpoint");
    table
        .write_bool("MAIN.bReset", true)
        .expect("reset");
    cycle.step(&mut table, 0.01);
    table
        .write_bool("MAIN.bReset", false)
        .expect("reset off");
    assert_eq!(table.read_i32("MAIN.stAxisX.nError"), Some(0));

    for _ in 0..2000 {
        cycle.step(&mut table, 0.01);
    }
    let position = table.read_f64("MAIN.stAxisX.fActPos").expect("position");
    assert!((position - 20.0).abs() < 1e-6);
}
