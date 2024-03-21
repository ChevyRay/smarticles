use std::f32::consts::TAU;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use array2d::Array2D;
use egui::Vec2;
use rand::distributions::Open01;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use rayon::prelude::*;

use crate::{
    SharedState, SimResults, UiEvent, UpdateSharedState, DEFAULT_FORCE, DEFAULT_RADIUS,
    DEFAULT_WORLD_RADIUS, FORCE_FACTOR, MAX_CLASSES, MAX_PARTICLE_COUNT, MIN_RADIUS,
};

/// Radius of the spawn area.
const SPAWN_AREA_RADIUS: f32 = 40.;

/// Below this radius, particles repel each other (see [`get_dv`]).
const RAMP_START_RADIUS: f32 = MIN_RADIUS;
/// The force with which the particles repel each other when
/// below [`MIN_RADIUS`]. It is scaled depending on the distance
/// between particles (see [`get_dv`] second arm).
/// The radius where the force ramp ends (see [`get_dv`] first arm).
const RAMP_END_RADIUS: f32 = 10.;
/// "Close force", see graph below.
const CLOSE_FORCE: f32 = 20. * FORCE_FACTOR;

// I made a graph of the force with respect to distance in
// order to explain the above constants (it might not help at all):
//
//
//                   force ^
//                         |
//                         |
//  force of the particle  | . . . . . . . . . . . . . . . . . . . . ./-----------------------
//  (changing)             |                                        /-.
//                         |                                      /-  .
//                         |                                    /-    .
//                         |                                 /--      .
//                         |                               /-         .
//                         |                             /-           .
//                         |                           /-             .
//                         |                         /-               .
//                         |------------------------------------------------------------------>  radius (r)
//                         |                 ----/  ^                 ^
//                         |            ----/       |                 |
//                         |       ----/            |                 |
//                         |  ----/         RAMP_START_RADIUS     RAMP_START_RADIUS + RAMP_END_RADIUS
//            CLOSE_FORCE  |-/
//                         |
//                         |
//                         |
//                         |
//

const BORDER_FORCE: f32 = 10. * FORCE_FACTOR;

const DEFAULT_DAMPING_FACTOR: f32 = 0.4;
const POS_FACTOR: f32 = 40.;

#[derive(PartialEq)]
pub enum SimulationState {
    Stopped,
    Paused,
    Running,
}

pub struct Simulation {
    shared: SharedState,

    particle_positions: Array2D<Vec2>,
    particle_velocities: Array2D<Vec2>,

    prev_instant: Instant,

    sim_send: Sender<SimResults>,
    ui_rcv: Receiver<UiEvent>,
}

impl Simulation {
    const DT: Duration = Duration::from_millis(20);

    pub fn new(sim_send: Sender<SimResults>, ui_rcv: Receiver<UiEvent>) -> Self {
        Self {
            shared: SharedState::new(),

            particle_positions: Array2D::filled_with(Vec2::ZERO, MAX_CLASSES, MAX_PARTICLE_COUNT),
            particle_velocities: Array2D::filled_with(Vec2::ZERO, MAX_CLASSES, MAX_PARTICLE_COUNT),

            prev_instant: Instant::now(),

            sim_send,
            ui_rcv,
        }
    }

    pub fn update(&mut self) {
        let start_time = Instant::now();

        let events = self.ui_rcv.try_iter().collect::<Vec<_>>();
        for event in events {
            match event {
                UiEvent::Play => self.play(),
                UiEvent::Pause => self.pause(),
                UiEvent::Reset => {
                    self.reset();
                    self.shared.simulation_state = SimulationState::Stopped;
                }
                UiEvent::Spawn => self.spawn(),
                UiEvent::ParamsUpdate(params) => self.shared.param_matrix = params,
                UiEvent::ClassCountUpdate(class_count) => self.shared.class_count = class_count,
                UiEvent::ParticleCountsUpdate(particle_counts) => {
                    self.shared.particle_counts = particle_counts
                }
                UiEvent::WorldRadiusUpdate(world_radius) => self.shared.world_radius = world_radius,
            }
        }

        if self.shared.simulation_state == SimulationState::Running {
            let calc_start_time = Instant::now();
            self.move_particles(Self::DT.as_secs_f32());
            self.sim_send
                .send(SimResults(
                    calc_start_time.elapsed(),
                    self.particle_positions.to_owned(),
                ))
                .unwrap();
        }
        let elapsed = start_time.elapsed();
        if elapsed < Self::DT {
            thread::sleep(Self::DT - elapsed);
        }
        self.prev_instant = Instant::now();
    }

    fn move_particles(&mut self, dt: f32) {
        for c1 in 0..self.shared.class_count {
            for c2 in 0..self.shared.class_count {
                let param = &self.shared.param_matrix[(c1, c2)];
                let force = -param.force * FORCE_FACTOR;
                let radius = param.radius;

                (0..self.shared.particle_counts[c1])
                    .into_par_iter()
                    .filter_map(|p1| {
                        let mut dv = Vec2::ZERO;

                        let mut pos = self.particle_positions[(c1, p1)].to_owned();
                        let mut vel = self.particle_velocities[(c1, p1)].to_owned();
                        for p2 in 0..self.shared.particle_counts[c2] {
                            let other_pos = self.particle_positions[(c2, p2)];
                            dv += get_partial_vel(other_pos - pos, radius, force);
                        }

                        let r = pos.length();
                        if r >= self.shared.world_radius {
                            dv += -pos.normalized() * BORDER_FORCE * (r - self.shared.world_radius);
                        }

                        vel = (vel + dv) * DEFAULT_DAMPING_FACTOR;
                        pos += vel * POS_FACTOR * dt;

                        Some((pos, vel))
                    })
                    .collect::<Vec<(Vec2, Vec2)>>()
                    .iter()
                    .enumerate()
                    .for_each(|(p1, (pos, vel))| {
                        self.particle_positions[(c1, p1)] = pos.to_owned();
                        self.particle_velocities[(c1, p1)] = vel.to_owned();
                    });
            }
        }
    }

    fn reset_particles(&mut self) {
        for c in 0..self.shared.class_count {
            for p in 0..self.shared.particle_counts[c] {
                self.particle_positions[(c, p)] = Vec2::ZERO;
            }
        }
    }
}

impl UpdateSharedState for Simulation {
    fn play(&mut self) {
        self.prev_instant = Instant::now();
        self.shared.simulation_state = SimulationState::Running;
    }

    fn pause(&mut self) {
        self.shared.simulation_state = SimulationState::Paused;
    }

    fn reset(&mut self) {
        self.shared.simulation_state = SimulationState::Stopped;
        self.shared.world_radius = DEFAULT_WORLD_RADIUS;

        self.shared.particle_counts.iter_mut().for_each(|p| *p = 0);
        self.reset_particles();

        for i in 0..MAX_CLASSES {
            for j in 0..MAX_CLASSES {
                self.shared.param_matrix[(i, j)].force = DEFAULT_FORCE;
                self.shared.param_matrix[(i, j)].radius = DEFAULT_RADIUS;
            }
        }
    }

    fn spawn(&mut self) {
        self.reset_particles();

        let mut rand = SmallRng::from_entropy();

        for c in 0..self.shared.class_count {
            for p in 0..self.shared.particle_counts[c] {
                self.particle_positions[(c, p)] =
                    SPAWN_AREA_RADIUS * Vec2::angled(TAU * rand.sample::<f32, _>(Open01));
            }
        }
    }
}

pub fn get_partial_vel(distance: Vec2, action_radius: f32, force: f32) -> Vec2 {
    match distance.length() {
        r if r < action_radius && r > RAMP_START_RADIUS => {
            distance.normalized() * force * ramp_then_const(r, RAMP_START_RADIUS, RAMP_END_RADIUS)
        }
        r if r <= RAMP_START_RADIUS && r > 0. => {
            distance.normalized() * CLOSE_FORCE * simple_ramp(r, RAMP_START_RADIUS)
        }
        _ => Vec2::ZERO,
    }
}

#[inline]
fn simple_ramp(x: f32, y_intercept: f32) -> f32 {
    (x / y_intercept) - 1.
}
#[inline]
fn ramp_then_const(x: f32, zero: f32, const_start: f32) -> f32 {
    // value of const: 2. * const_start / (zero + const_start)
    (-(x - zero - const_start).abs() + x - zero + const_start) / (zero + const_start)
}
