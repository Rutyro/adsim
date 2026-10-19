//! PLC cycle that maps gantry symbols to the simulated axes.

use adsim_axis::{Axis, AxisConfig};
use adsim_symbols::SymbolTable;

const AXES: [&str; 3] = ["X", "Y", "Z"];

pub struct GantryCycle {
    axes: [Axis; 3],
}

impl GantryCycle {
    pub fn new() -> Self {
        let config = || AxisConfig {
            min: 0.0,
            max: 300.0,
            max_velocity: 100.0,
            acceleration: 500.0,
        };

        Self {
            axes: [
                Axis::new(config()),
                Axis::new(config()),
                Axis::new(config()),
            ],
        }
    }

    pub fn step(&mut self, symbols: &mut SymbolTable, dt: f64) {
        let reset = symbols.read_bool("MAIN.bReset").unwrap_or(false);

        for (axis, name) in self.axes.iter_mut().zip(AXES) {
            let prefix = format!("MAIN.stAxis{name}");
            axis.set_enabled(
                symbols
                    .read_bool(&format!("{prefix}.bEnabled"))
                    .unwrap_or(false),
            );
            axis.set_homed(
                symbols
                    .read_bool(&format!("{prefix}.bHomed"))
                    .unwrap_or(false),
            );
            if reset {
                axis.reset();
            }

            let setpoint = symbols
                .read_f64(&format!("{prefix}.fSetPos"))
                .unwrap_or(0.0);
            axis.command(setpoint);
            axis.step(dt);

            let _ = symbols.write_f64(&format!("{prefix}.fActPos"), axis.position());
            let _ = symbols.write_i32(&format!("{prefix}.nError"), i32::from(axis.faulted()));
        }

        let faulted = self.axes.iter().position(|axis| axis.faulted());
        let _ = symbols.write_bool("MAIN.bAlarm", faulted.is_some());
        let _ = symbols.write_i32(
            "MAIN.nAlarmCode",
            faulted.map_or(0, |axis| 1001 + axis as i32),
        );
    }
}

impl Default for GantryCycle {
    fn default() -> Self {
        Self::new()
    }
}
