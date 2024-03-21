use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;

use array2d::Array2D;
use eframe::epaint::Color32;
use eframe::NativeOptions;
use egui::Vec2;
use simulation::SimulationState;
use ui::Smarticles;

use crate::simulation::Simulation;

mod simulation;
mod ui;

// IDEA Add recordings ? By exporting positions of all the
// particles each frame ? That would make around 8000 postions
// every 1/60 second that is to say 60*8000=480,000 positions
// per second, let's assume a position is 8 bytes (from Vec2),
// then one second of simulation is 8*480,000=3,840,000 bytes
// this is around 4MB. 1min of simulation is 60*4=240MB.
// This seems possible, although not for long recordings.
// Saving the exact starting position might also work although
// if the simulation runs for too long there might be differences
// between computers.

/// Min number of particle classes in the simulation.
const MIN_CLASSES: usize = 3;
/// Max number of particle classes in the simulation.
const MAX_CLASSES: usize = 8;

/// Default world width the simulation.
const DEFAULT_WORLD_RADIUS: f32 = 900.;
/// Min world width the simulation.
const MIN_WORLD_RADIUS: f32 = 200.;
/// Max world width the simulation.
const MAX_WORLD_RADIUS: f32 = 1200.;

/// Min particle count.
const MIN_PARTICLE_COUNT: usize = 0;
/// Maximal particle count per class.
const MAX_PARTICLE_COUNT: usize = 1200;
/// When randomizing particle counts, this is the lowest
/// possible value, this prevent random particle counts from
/// being under this value.
const RANDOM_MIN_PARTICLE_COUNT: usize = 200;
/// When randomizing particle counts, this is the highest
/// possible value, this prevent random particle counts from
/// being above this value.
const RANDOM_MAX_PARTICLE_COUNT: usize = 1000;

const DEFAULT_FORCE: f32 = 0.;
const MAX_FORCE: f32 = 100.;
const MIN_FORCE: f32 = -MAX_FORCE;
/// Scales force.
const FORCE_FACTOR: f32 = 1. / 500.;

const DEFAULT_RADIUS: f32 = 80.;
const MIN_RADIUS: f32 = 30.;
const MAX_RADIUS: f32 = 100.;

fn main() {
    let options = NativeOptions {
        // initial_window_size: Some(Vec2::new(1600., 900.)),
        fullscreen: true,
        ..Default::default()
    };

    // ("α", Color32::from_rgb(251, 70, 76)),
    // ("β", Color32::from_rgb(233, 151, 63)),
    // ("γ", Color32::from_rgb(224, 222, 113)),
    // ("δ", Color32::from_rgb(68, 207, 110)),
    // ("ε", Color32::from_rgb(83, 223, 221)),
    // ("ζ", Color32::from_rgb(2, 122, 255)),
    // ("η", Color32::from_rgb(168, 130, 255)),
    // ("θ", Color32::from_rgb(250, 153, 205)),

    // ("α", Color32::from_rgb(251, 123, 119)),
    // ("β", Color32::from_rgb(253, 193, 112)),
    // ("γ", Color32::from_rgb(243, 248, 127)),
    // ("δ", Color32::from_rgb(152, 247, 134)),
    // ("ε", Color32::from_rgb(105, 235, 252)),
    // ("ζ", Color32::from_rgb(109, 158, 252)),
    // ("η", Color32::from_rgb(147, 125, 248)),
    // ("θ", Color32::from_rgb(247, 142, 240)),

    let (ui_send, ui_rcv) = channel::<UiEvent>();
    let (sim_send, sim_rcv) = channel::<SimResults>();

    let smarticles = Smarticles::new(
        [
            ("α", Color32::from_rgb(247, 0, 243)),
            ("β", Color32::from_rgb(166, 0, 255)),
            ("γ", Color32::from_rgb(60, 80, 255)),
            ("δ", Color32::from_rgb(0, 247, 255)),
            ("ε", Color32::from_rgb(68, 255, 0)),
            ("ζ", Color32::from_rgb(225, 255, 0)),
            ("η", Color32::from_rgb(255, 140, 0)),
            ("θ", Color32::from_rgb(255, 0, 0)),
        ],
        ui_send,
        sim_rcv,
    );

    eframe::run_native(
        "Smarticles",
        options,
        Box::new(|cc| {
            let frame = cc.egui_ctx.clone();

            thread::spawn(move || {
                let mut simulation = Simulation::new(sim_send, ui_rcv);
                thread::sleep(Duration::from_millis(500));

                loop {
                    simulation.update();
                    frame.request_repaint();
                }
            });

            Box::new(smarticles)
        }),
    );
}

#[derive(Debug)]
enum UiEvent {
    Play,
    Pause,
    Reset,
    Spawn,
    ParamsUpdate(Array2D<Param>),
    ClassCountUpdate(usize),
    ParticleCountsUpdate([usize; MAX_CLASSES]),
    WorldRadiusUpdate(f32),
}

#[derive(Debug)]
struct SimResults(Duration, Array2D<Vec2>);

#[derive(Debug, Clone)]
struct Param {
    force: f32,
    radius: f32,
}
impl Param {
    pub fn new(force: f32, radius: f32) -> Self {
        Self { force, radius }
    }
}

struct SharedState {
    simulation_state: SimulationState,
    world_radius: f32,
    class_count: usize,
    particle_counts: [usize; MAX_CLASSES],
    /// Matrix containing force and radius for each particle class
    /// with respect to each other.
    param_matrix: Array2D<Param>,
}

impl SharedState {
    fn new() -> Self {
        Self {
            simulation_state: SimulationState::Stopped,
            world_radius: DEFAULT_WORLD_RADIUS,
            class_count: MAX_CLASSES,
            particle_counts: [0; MAX_CLASSES],
            param_matrix: Array2D::filled_with(
                Param::new(DEFAULT_FORCE, DEFAULT_RADIUS),
                MAX_CLASSES,
                MAX_CLASSES,
            ),
        }
    }
}

trait UpdateSharedState {
    fn play(&mut self);
    fn pause(&mut self);
    fn reset(&mut self);
    fn spawn(&mut self);
}
