use std::time::Duration;

pub const CLASS_COUNT: usize = 6;

/// Min particle count.
pub const MIN_PARTICLE_COUNT: usize = 0;
/// Maximal particle count per class.
pub const MAX_PARTICLE_COUNT: usize = 15000;
/// Default particle count per class.
pub const DEFAULT_PARTICLE_COUNT: usize = 10000;

pub const MAX_POWER: i8 = 100;
pub const MIN_POWER: i8 = -MAX_POWER;

// app

/// Display diameter of the particles in the simulation (in
/// pixels).
pub const PARTICLE_DIAMETER: f32 = 0.6;

pub const DEFAULT_ZOOM: f32 = 2.;
pub const MIN_ZOOM: f32 = 0.5;
pub const MAX_ZOOM: f32 = 30.;
pub const ZOOM_FACTOR: f32 = 1.05;

pub const MAX_HISTORY_LEN: usize = 10;

// simulation

pub const PROXIMITY_POWER: f32 = -160.;

// TODO make this interactive (changeable from ui)
pub const DAMPING_FACTOR: f32 = 100.;
pub const DT: f32 = 0.0004;

pub const SPAWN_DENSITY: f32 = 0.035;

pub const FIRST_THRESHOLD: f32 = 6.;
pub const SECOND_THRESHOLD: f32 = 20.;
/// Range in which particles interact. If two particles are
/// farther away than this distance, the will never interact.
pub const INTERACTION_RANGE: f32 = FIRST_THRESHOLD + 2. * SECOND_THRESHOLD;

// simulation manager

/// Min update interval in ms (when the simulation is running).
pub const UPDATE_INTERVAL: Duration = Duration::from_millis(30);
/// Min update rate when the simulation is paused.
pub const PAUSED_UPDATE_INTERVAL: Duration = Duration::from_millis(200);

// events

pub const LOG: bool = false;
