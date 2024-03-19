use std::collections::hash_map::DefaultHasher;
use std::collections::VecDeque;
use std::f32::consts::TAU;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use array2d::Array2D;
use byteorder::{ReadBytesExt, WriteBytesExt, LE};
use eframe::epaint::Color32;
use eframe::{App, Frame, NativeOptions};
use egui::plot::{Line, Plot, PlotPoints};
use egui::{
    Align2, CentralPanel, ComboBox, Context, FontId, Pos2, ScrollArea, Sense, SidePanel, Slider,
    Stroke, Vec2,
};
use rand::distributions::Open01;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use rayon::prelude::*;

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

/// Max update rate of the simulation (limited by the
/// eframe default update rate which is 60 fps). Set this to
/// reduce the amount of calculations per second. Note: this
/// reduces the simulation accuracy...
const MAX_UPDATE_RATE: Option<f32> = None; // Some(1. / 50.);

/// Min number of particle classes in the simulation.
const MIN_CLASSES: usize = 3;
/// Max number of particle classes in the simulation.
const MAX_CLASSES: usize = 8;

/// Size of the particles in the simulation.
const PARTICLE_SIZE: f32 = 2.;

/// Default world width the simulation.
const DEFAULT_WORLD_RADIUS: f32 = 900.;
/// Min world width the simulation.
const MIN_WORLD_RADIUS: f32 = 200.;
/// Max world width the simulation.
const MAX_WORLD_RADIUS: f32 = 1200.;

/// Radius of the spawn area.
const SPAWN_AREA_RADIUS: f32 = 40.;

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

const DEFAULT_POWER: f32 = 0.;
const MAX_POWER: f32 = 100.;
const MIN_POWER: f32 = -MAX_POWER;
/// Scales power.
const POWER_FACTOR: f32 = 1. / 500.;

const DEFAULT_RADIUS: f32 = 80.;
const MIN_RADIUS: f32 = RAMP_START_RADIUS;
const MAX_RADIUS: f32 = 100.;

/// Below this radius, particles repel each other (see [`get_dv`]).
const RAMP_START_RADIUS: f32 = 30.;
/// The power with which the particles repel each other when
/// below [`MIN_RADIUS`]. It is scaled depending on the distance
/// between particles (see [`get_dv`] second arm).
/// The radius where the power ramp ends (see [`get_dv`] first arm).
const RAMP_END_RADIUS: f32 = 10.;
/// "Close power", see graph below.
const CLOSE_POWER: f32 = 20. * POWER_FACTOR;

// I made a graph of the power with respect to the radius
// in order to explain the above constants (it might not help at all):
//
//
//                   power ^
//                         |
//                         |
//  power of the particle  | . . . . . . . . . . . . . . . . . . . . ./-----------------------
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
//            CLOSE_POWER  |-/
//                         |
//                         |
//                         |
//                         |
//

const BORDER_POWER: f32 = 10. * POWER_FACTOR;

const DEFAULT_DAMPING_FACTOR: f32 = 0.4;
const POS_FACTOR: f32 = 40.;

const DEFAULT_ZOOM: f32 = 1.;
const MIN_ZOOM: f32 = 0.5;
const MAX_ZOOM: f32 = 10.;
const ZOOM_FACTOR: f32 = 0.02;

const MAX_HISTORY_LEN: usize = 10;

fn main() {
    let options = NativeOptions {
        // initial_window_size: Some(Vec2::new(1600., 900.)),
        fullscreen: true,
        ..Default::default()
    };

    let smarticles = Smarticles::new([
        ("α", Color32::from_rgb(247, 0, 243)),
        ("β", Color32::from_rgb(166, 0, 255)),
        ("γ", Color32::from_rgb(60, 80, 255)),
        ("δ", Color32::from_rgb(0, 247, 255)),
        ("ε", Color32::from_rgb(68, 255, 0)),
        ("ζ", Color32::from_rgb(225, 255, 0)),
        ("η", Color32::from_rgb(255, 140, 0)),
        ("θ", Color32::from_rgb(255, 0, 0)),
    ]);

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

    eframe::run_native("Smarticles", options, Box::new(|_| Box::new(smarticles)));
}

struct Smarticles {
    world_radius: f32,

    state: SimulationState,
    class_count: usize,
    seed: String,

    classes: [ClassProps; MAX_CLASSES],
    /// The particle matrix: the first index is the class index
    /// `c` and the second is the particle index `p`. The `p`th
    /// particle of the `c`th class has index `(c, p)` in the matrix.
    particles: Array2D<Particle>,
    /// Matrix containing power and radius for each particle class
    /// with respect to each other.
    param_matrix: Array2D<Param>,

    prev_time: Instant,
    view: View,

    selected_param: (usize, usize),
    selected_particle: (usize, usize),

    history: VecDeque<String>,
    selected_history_entry: usize,

    words: Vec<String>,
}

impl Smarticles {
    fn new<S>(classes: [(S, Color32); MAX_CLASSES]) -> Self
    where
        S: ToString,
    {
        let words = include_str!("words.txt");
        let words: Vec<String> = words
            .par_lines()
            .filter_map(|w| {
                if w.len() > 8 {
                    return None;
                }
                for chr in w.chars() {
                    if !chr.is_ascii_alphabetic() || chr.is_ascii_uppercase() {
                        return None;
                    }
                }
                Some(w.to_string())
            })
            .collect();

        Self {
            world_radius: DEFAULT_WORLD_RADIUS,

            state: SimulationState::Stopped,
            class_count: MAX_CLASSES,
            seed: "".to_string(),

            classes: classes.map(|(name, color)| ClassProps {
                name: name.to_string(),
                heading: "class ".to_string() + &name.to_string(),
                color,
                particle_count: 0,
            }),
            particles: Array2D::filled_with(Particle::default(), MAX_CLASSES, MAX_PARTICLE_COUNT),
            param_matrix: Array2D::filled_with(
                Param::new(DEFAULT_POWER, DEFAULT_RADIUS),
                MAX_CLASSES,
                MAX_CLASSES,
            ),

            prev_time: Instant::now(),
            view: View::DEFAULT,

            selected_param: (0, 0),
            selected_particle: (0, 0),

            history: VecDeque::new(),
            selected_history_entry: 0,

            words,
        }
    }

    fn play(&mut self) {
        self.prev_time = Instant::now();
        self.state = SimulationState::Running;
    }

    fn pause(&mut self) {
        self.state = SimulationState::Paused;
    }

    fn reset(&mut self) {
        self.state = SimulationState::Stopped;
        self.world_radius = DEFAULT_WORLD_RADIUS;
        self.view = View::DEFAULT;

        self.classes.iter_mut().for_each(|p| p.particle_count = 0);
        self.reset_particles();

        for i in 0..MAX_CLASSES {
            for j in 0..MAX_CLASSES {
                self.param_matrix[(i, j)].power = DEFAULT_POWER;
                self.param_matrix[(i, j)].radius = DEFAULT_RADIUS;
            }
        }
    }

    fn spawn(&mut self) {
        self.reset_particles();

        let mut rand = SmallRng::from_entropy();

        for c in 0..self.class_count {
            for p in 0..self.classes[c].particle_count {
                self.particles[(c, p)] = Particle {
                    pos: Vec2::angled(TAU * rand.sample::<f32, _>(Open01))
                        * SPAWN_AREA_RADIUS
                        * rand.sample::<f32, _>(Open01),
                    vel: Vec2::ZERO,
                };
            }
        }
    }

    fn reset_particles(&mut self) {
        for c in 0..self.class_count {
            for p in 0..self.classes[c].particle_count {
                self.particles[(c, p)] = Particle::default();
            }
        }
    }

    fn simulate(&mut self, dt: f32) {
        for c1 in 0..self.class_count {
            for c2 in 0..self.class_count {
                let param = &self.param_matrix[(c1, c2)];
                let power = -param.power * POWER_FACTOR;
                let radius = param.radius;

                (0..self.classes[c1].particle_count)
                    .into_par_iter()
                    .map(|p1| {
                        let mut v = Vec2::ZERO;

                        let mut a = self.particles[(c1, p1)];
                        for p2 in 0..self.classes[c2].particle_count {
                            let b = &self.particles[(c2, p2)];
                            v += get_dv(b.pos - a.pos, radius, power);
                        }

                        let d = a.pos;
                        let r = d.length();
                        if r >= self.world_radius {
                            v += -d.normalized() * BORDER_POWER * (r - self.world_radius);
                        }

                        a.vel = (a.vel + v) * DEFAULT_DAMPING_FACTOR;
                        a.pos += a.vel * POS_FACTOR * dt;

                        a
                    })
                    .collect::<Vec<Particle>>()
                    .iter()
                    .enumerate()
                    .for_each(|(p1, particle)| {
                        self.particles[(c1, p1)] = *particle;
                    });
            }
        }
    }

    fn apply_seed(&mut self) {
        self.reset_particles();

        let mut rand = if self.seed.is_empty() {
            SmallRng::from_entropy()
        } else {
            if self.seed.starts_with('@') {
                if let Ok(bytes) = base64::decode(&self.seed[1..]) {
                    self.import(&bytes);
                    return;
                }
            }
            let mut hasher = DefaultHasher::new();
            self.seed.hash(&mut hasher);
            SmallRng::seed_from_u64(hasher.finish())
        };
        let mut rand = |min: f32, max: f32| min + (max - min) * rand.sample::<f32, _>(Open01);

        const POW_F: f32 = 1.25;
        const RAD_F: f32 = 1.1;

        for i in 0..self.class_count {
            self.classes[i].particle_count = rand(
                RANDOM_MIN_PARTICLE_COUNT as f32,
                RANDOM_MAX_PARTICLE_COUNT as f32,
            ) as usize;
            for j in 0..self.class_count {
                let pow = rand(MIN_POWER, MAX_POWER);
                self.param_matrix[(i, j)].power = pow.signum() * pow.abs().powf(1. / POW_F);
                self.param_matrix[(i, j)].radius = rand(MIN_RADIUS, MAX_RADIUS).powf(1. / RAD_F);
            }
        }
    }

    fn export(&self) -> String {
        let mut bytes: Vec<u8> = Vec::new();
        bytes.write_u16::<LE>(self.world_radius as u16).unwrap();
        bytes.write_u8(self.class_count as u8).unwrap();
        for prop in &self.classes {
            // bytes.write_u8((prop.color.r() * 255.) as u8).unwrap();
            // bytes.write_u8((prop.color.g() * 255.) as u8).unwrap();
            // bytes.write_u8((prop.color.b() * 255.) as u8).unwrap();
            bytes.write_u16::<LE>(prop.particle_count as u16).unwrap();
        }
        self.param_matrix.elements_row_major_iter().for_each(|p| {
            bytes.write_i8(p.power as i8).unwrap();
            bytes.write_i8(p.radius as i8).unwrap();
        });

        format!("@{}", base64::encode(bytes))
    }

    fn import(&mut self, mut bytes: &[u8]) {
        self.world_radius = bytes
            .read_u16::<LE>()
            .unwrap_or(DEFAULT_WORLD_RADIUS as u16) as f32;
        self.class_count = bytes.read_u8().unwrap_or(MAX_CLASSES as u8) as usize;
        for p in &mut self.classes {
            // let r = (bytes.read_u8().unwrap_or((p.color.r() * 255.) as u8) as f32) / 255.;
            // let g = (bytes.read_u8().unwrap_or((p.color.g() * 255.) as u8) as f32) / 255.;
            // let b = (bytes.read_u8().unwrap_or((p.color.b() * 255.) as u8) as f32) / 255.;
            // p.color = Rgba::from_rgb(r, g, b);
            p.particle_count = bytes.read_u16::<LE>().unwrap_or(0) as usize;
        }

        for i in 0..MAX_CLASSES {
            for j in 0..MAX_CLASSES {
                self.param_matrix[(i, j)].power = bytes.read_i8().unwrap_or(0) as f32;
                self.param_matrix[(i, j)].radius = bytes.read_i8().unwrap_or(0) as f32;
            }
        }
    }

    fn update_history(&mut self) {
        self.history.push_back(self.seed.to_owned());
        if self.history.len() > MAX_HISTORY_LEN {
            self.history.pop_front();
        }
        self.selected_history_entry = self.history.len() - 1;
    }
}

impl App for Smarticles {
    fn update(&mut self, ctx: &Context, frame: &mut Frame) {
        let mut calc_duration = 0;

        if self.state == SimulationState::Running {
            let time = Instant::now();
            let dt = time - self.prev_time;
            if let Some(tps) = MAX_UPDATE_RATE {
                if dt > Duration::from_secs_f32(tps) {
                    self.simulate(dt.as_secs_f32());
                    self.prev_time = time;
                }
            } else {
                self.simulate(dt.as_secs_f32());
                self.prev_time = time;
            }
            calc_duration = time.elapsed().as_millis();
        }

        SidePanel::left("settings").show(ctx, |ui| {
            ui.heading("settings");
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .button("respawn")
                    .on_hover_text("spawn particles again")
                    .clicked()
                {
                    self.spawn();
                }

                if self.state == SimulationState::Running {
                    if ui
                        .button("pause")
                        .on_hover_text("pause the simulation")
                        .clicked()
                    {
                        self.pause();
                    }
                } else if ui
                    .button("play")
                    .on_hover_text("start the simulation")
                    .clicked()
                {
                    self.play();
                }

                if ui
                    .button("randomize")
                    .on_hover_text("randomly pick a new seed")
                    .clicked()
                {
                    let w1 = rand::random::<usize>() % self.words.len();
                    let w2 = rand::random::<usize>() % self.words.len();
                    let w3 = rand::random::<usize>() % self.words.len();
                    self.seed = format!("{}_{}_{}", self.words[w1], self.words[w2], self.words[w3]);

                    self.update_history();

                    self.apply_seed();
                    self.spawn();
                }

                if ui
                    .button("reset View")
                    .on_hover_text("reset zoom and position")
                    .clicked()
                {
                    self.view = View::DEFAULT;
                }

                if ui
                    .button("reset")
                    .on_hover_text("reset everything")
                    .clicked()
                {
                    self.reset();
                }

                if ui.button("quit").on_hover_text("exit smarticles").clicked() {
                    frame.close();
                }
            });
            ui.horizontal(|ui| {
                ui.label("seed:");
                ui.text_edit_singleline(&mut self.seed);
                if ui.button("apply").clicked() {
                    self.update_history();

                    self.apply_seed();
                    self.spawn();
                }
            });

            ui.horizontal(|ui| {
                ui.label("world radius:");
                let world_radius = ui.add(Slider::new(
                    &mut self.world_radius,
                    MIN_WORLD_RADIUS..=MAX_WORLD_RADIUS,
                ));
                let reset = ui.button("reset");
                if reset.clicked() {
                    self.world_radius = DEFAULT_WORLD_RADIUS;
                }
                if world_radius.changed() || reset.clicked() {
                    self.seed = self.export();
                    self.spawn();
                }
            });

            ui.horizontal(|ui| {
                ui.label("particle classes:");
                let class_count = ui.add(Slider::new(
                    &mut self.class_count,
                    MIN_CLASSES..=MAX_CLASSES,
                ));
                let reset = ui.button("reset");
                if reset.clicked() {
                    self.class_count = MAX_CLASSES;
                }
                if class_count.changed() || reset.clicked() {
                    self.seed = self.export();
                    self.spawn();
                }
            });

            ui.horizontal(|ui| {
                ui.label("total particle count:");

                let total_particle_count: usize =
                    self.classes.iter().map(|c| c.particle_count).sum();
                ui.code(total_particle_count.to_string());
            });

            ui.horizontal(|ui| {
                ui.label("calculation time:");
                ui.code(calc_duration.to_string() + "ms");
            });

            if self.history.len() > 1 {
                ui.collapsing("seed history", |ui| {
                    if ComboBox::from_id_source("seed history")
                        .width(200.)
                        .show_index(
                            ui,
                            &mut self.selected_history_entry,
                            self.history.len(),
                            |i| self.history[i].to_owned(),
                        )
                        .changed()
                    {
                        self.apply_seed();
                        self.spawn();
                    };
                });
            }

            ui.collapsing("particle inspector", |ui| {
                ui.horizontal(|ui| {
                    ui.label("class:");
                    ComboBox::from_id_source("class").show_index(
                        ui,
                        &mut self.selected_particle.0,
                        self.classes.len(),
                        |i| self.classes[i].heading.to_owned(),
                    );
                    ui.label("particle index:");
                    ui.add(Slider::new(
                        &mut self.selected_particle.1,
                        0..=(self.classes[self.selected_particle.0].particle_count - 1),
                    ));
                });

                ui.horizontal(|ui| {
                    ui.label("position:");
                    ui.code(format!("{:?}", self.particles[self.selected_particle].pos));
                });

                ui.horizontal(|ui| {
                    ui.label("velocity:");
                    ui.code(
                        self.particles[self.selected_particle]
                            .vel
                            .length()
                            .to_string(),
                    );
                    ui.code(format!("{:?}", self.particles[self.selected_particle].vel));
                });
            });

            ui.collapsing(
                "velocity elementary variation with respect to distance between particles",
                |ui| {
                    let points: PlotPoints = (0..1000)
                        .map(|i| {
                            let x = i as f32 * 0.1;
                            [
                                x as f64,
                                get_dv(
                                    Vec2::new(x, 0.),
                                    self.param_matrix[self.selected_param].radius,
                                    self.param_matrix[self.selected_param].power * POWER_FACTOR,
                                )
                                .x as f64,
                            ]
                        })
                        .collect();
                    let line = Line::new(points);
                    Plot::new("activation function")
                        .view_aspect(2.0)
                        .show(ui, |plot_ui| plot_ui.line(line));
                },
            );

            ScrollArea::vertical().show(ui, |ui| {
                for i in 0..self.class_count {
                    ui.add_space(10.);
                    ui.colored_label(self.classes[i].color, &self.classes[i].heading);
                    ui.separator();

                    // ui.horizontal(|ui| {
                    //     ui.label("color:");
                    //     let mut rgb = [
                    //         self.classes[i].color.r(),
                    //         self.classes[i].color.g(),
                    //         self.classes[i].color.b(),
                    //     ];
                    //     if ui.color_edit_button_rgb(&mut rgb).changed() {
                    //         self.classes[i].color = Rgba::from_rgb(rgb[0], rgb[1], rgb[2]);
                    //         self.seed = self.export();
                    //     }
                    // });

                    ui.horizontal(|ui| {
                        ui.label("particle count:");
                        if ui
                            .add(Slider::new(
                                &mut self.classes[i].particle_count,
                                MIN_PARTICLE_COUNT..=MAX_PARTICLE_COUNT,
                            ))
                            .changed()
                        {
                            self.seed = self.export();
                            self.spawn();
                        }
                    });

                    ui.collapsing(self.classes[i].heading.to_owned() + " params", |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                for j in 0..self.class_count {
                                    ui.horizontal(|ui| {
                                        ui.label("power (");
                                        ui.colored_label(
                                            self.classes[j].color,
                                            &self.classes[j].name,
                                        );
                                        ui.label(")");
                                        if ui
                                            .add(Slider::new(
                                                &mut self.param_matrix[(i, j)].power,
                                                MIN_POWER..=MAX_POWER,
                                            ))
                                            .changed()
                                        {
                                            self.selected_param = (i, j);
                                            self.seed = self.export();
                                        }
                                    });
                                }
                            });
                            ui.vertical(|ui| {
                                for j in 0..self.class_count {
                                    ui.horizontal(|ui| {
                                        ui.label("radius (");
                                        ui.colored_label(
                                            self.classes[j].color,
                                            &self.classes[j].name,
                                        );
                                        ui.label(")");
                                        if ui
                                            .add(Slider::new(
                                                &mut self.param_matrix[(i, j)].radius,
                                                MIN_RADIUS..=MAX_RADIUS,
                                            ))
                                            .changed()
                                        {
                                            self.selected_param = (i, j);
                                            self.seed = self.export();
                                        }
                                    });
                                }
                            });
                        });
                    });
                }
            });
        });

        CentralPanel::default()
            .frame(egui::Frame {
                fill: Color32::from_rgba_unmultiplied(12, 12, 12, 255),
                ..Default::default()
            })
            .show(ctx, |ui| {
                let (resp, paint) =
                    ui.allocate_painter(ui.available_size_before_wrap(), Sense::hover());

                if resp
                    .rect
                    .contains(ctx.input().pointer.interact_pos().unwrap_or_default())
                {
                    self.view.zoom += ctx.input().scroll_delta.y * ZOOM_FACTOR;
                }

                // This is weird but look at the values.
                self.view.zoom = self.view.zoom.min(MAX_ZOOM).max(MIN_ZOOM);

                if let Some(interact_pos) = ctx.input().pointer.interact_pos() {
                    if ctx.input().pointer.any_down() && resp.rect.contains(interact_pos) {
                        if !self.view.dragging {
                            self.view.dragging = true;
                            self.view.drag_start_pos = interact_pos;
                            self.view.drag_start_view_pos = self.view.pos;
                        }
                    } else {
                        self.view.dragging = false;
                    }
                }

                if self.view.dragging {
                    let drag_delta =
                        ctx.input().pointer.interact_pos().unwrap() - self.view.drag_start_pos;
                    self.view.pos = self.view.drag_start_view_pos + drag_delta / self.view.zoom;
                }

                let min = resp.rect.min
                    + Vec2::new(resp.rect.width(), resp.rect.height()) / 2.
                    + (-Vec2::new(self.world_radius, self.world_radius) + self.view.pos.to_vec2())
                        * self.view.zoom;

                paint.circle_stroke(
                    min + Vec2::new(self.world_radius, self.world_radius) * self.view.zoom,
                    (self.world_radius + 60.) * self.view.zoom,
                    Stroke {
                        width: 1.,
                        color: Color32::from_rgb(200, 200, 200),
                    },
                );

                let center = min + Vec2::new(self.world_radius, self.world_radius) * self.view.zoom;

                for c in 0..self.class_count {
                    let class = &self.classes[c];
                    let col: Color32 = class.color.into();

                    for p in 0..class.particle_count {
                        let pos = center + self.particles[(c, p)].pos * self.view.zoom;
                        if paint.clip_rect().contains(pos) {
                            paint.circle_filled(pos, PARTICLE_SIZE / 2., col);
                        }
                    }
                }

                if self.state != SimulationState::Stopped {
                    paint.text(
                        center + self.particles[self.selected_particle].pos * self.view.zoom,
                        Align2::LEFT_BOTTOM,
                        format!("{:?}", self.selected_particle),
                        FontId::monospace(12.),
                        Color32::WHITE,
                    );
                }
            });

        ctx.request_repaint();
    }
}

fn get_dv(distance: Vec2, action_radius: f32, power: f32) -> Vec2 {
    match distance.length() {
        r if r < action_radius && r > RAMP_START_RADIUS => {
            distance.normalized() * power * ramp_then_const(r, RAMP_START_RADIUS, RAMP_END_RADIUS)
        }
        r if r <= RAMP_START_RADIUS && r > 0. => {
            distance.normalized() * CLOSE_POWER * simple_ramp(r, RAMP_START_RADIUS)
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

#[derive(PartialEq)]
enum SimulationState {
    Stopped,
    Paused,
    Running,
}

struct ClassProps {
    name: String,
    heading: String,
    color: Color32,
    particle_count: usize,
}

#[derive(Debug, Clone)]
struct Param {
    power: f32,
    radius: f32,
}

impl Param {
    pub fn new(power: f32, radius: f32) -> Self {
        Self { power, radius }
    }
}

struct View {
    zoom: f32,
    pos: Pos2,
    dragging: bool,
    drag_start_pos: Pos2,
    drag_start_view_pos: Pos2,
}

impl View {
    const DEFAULT: View = Self {
        zoom: DEFAULT_ZOOM,
        pos: Pos2::ZERO,
        dragging: false,
        drag_start_pos: Pos2::ZERO,
        drag_start_view_pos: Pos2::ZERO,
    };
}

#[derive(Clone, Copy)]
struct Particle {
    pos: Vec2,
    vel: Vec2,
}

impl Default for Particle {
    fn default() -> Self {
        Self {
            pos: Vec2::ZERO,
            vel: Vec2::ZERO,
        }
    }
}
