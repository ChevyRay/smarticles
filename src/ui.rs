use std::collections::hash_map::DefaultHasher;
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::sync::mpsc::{Receiver, Sender};
use std::thread::JoinHandle;

use array2d::Array2D;
use byteorder::{ReadBytesExt, WriteBytesExt, LE};
use eframe::epaint::Color32;
use eframe::{App, Frame};
use egui::plot::{Line, Plot, PlotPoints};
use egui::{
    Align2, CentralPanel, ComboBox, Context, FontId, ScrollArea, Sense, SidePanel, Slider, Stroke,
    Vec2,
};
use rand::distributions::Open01;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use rayon::prelude::*;

use crate::simulation::{get_partial_velocity, SimulationState};
use crate::{
    SharedState, SimResults, UiEvent, UpdateSharedState, DEFAULT_WORLD_RADIUS, FORCE_FACTOR,
    MAX_CLASSES, MAX_FORCE, MAX_PARTICLE_COUNT, MAX_RADIUS, MAX_WORLD_RADIUS, MIN_CLASSES,
    MIN_FORCE, MIN_PARTICLE_COUNT, MIN_RADIUS, MIN_WORLD_RADIUS, RANDOM_MAX_PARTICLE_COUNT,
    RANDOM_MIN_PARTICLE_COUNT,
};

/// Display diameter of the particles in the simulation (in
/// pixels).
const PARTICLE_DIAMETER: f32 = 1.;

const DEFAULT_ZOOM: f32 = 1.2;
const MIN_ZOOM: f32 = 0.5;
const MAX_ZOOM: f32 = 10.;
const ZOOM_FACTOR: f32 = 0.02;

const MAX_HISTORY_LEN: usize = 10;

pub struct View {
    zoom: f32,
    pos: Vec2,
    prev_follow_pos: Vec2,
    dragging: bool,
    drag_start_pos: Vec2,
    drag_start_view_pos: Vec2,
}

impl View {
    const DEFAULT: View = Self {
        zoom: DEFAULT_ZOOM,
        pos: Vec2::ZERO,
        prev_follow_pos: Vec2::ZERO,
        dragging: false,
        drag_start_pos: Vec2::ZERO,
        drag_start_view_pos: Vec2::ZERO,
    };
}

#[derive(Debug)]
struct ClassProps {
    name: String,
    heading: String,
    color: Color32,
}

pub struct Smarticles {
    shared: SharedState,

    classes: [ClassProps; MAX_CLASSES],
    particle_positions: Array2D<Vec2>,

    seed: String,

    view: View,

    selected_param: (usize, usize),
    selected_particle: (usize, usize),
    follow_selected_particle: bool,

    history: VecDeque<String>,
    selected_history_entry: usize,

    calculation_time: u128,

    words: Vec<String>,

    ui_send: Sender<UiEvent>,
    sim_rcv: Receiver<SimResults>,

    simulation_handle: Option<JoinHandle<()>>,
}

impl Smarticles {
    pub fn new<S>(
        classes: [(S, Color32); MAX_CLASSES],
        ui_send: Sender<UiEvent>,
        sim_rcv: Receiver<SimResults>,
        simulation_handle: Option<JoinHandle<()>>,
    ) -> Self
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
            shared: SharedState::new(),

            seed: "".to_string(),

            classes: classes.map(|(name, color)| ClassProps {
                name: name.to_string(),
                heading: "class ".to_string() + &name.to_string(),
                color,
            }),
            particle_positions: Array2D::filled_with(Vec2::ZERO, MAX_CLASSES, MAX_PARTICLE_COUNT),

            // prev_time: Instant::now(),
            view: View::DEFAULT,

            selected_param: (0, 0),
            selected_particle: (0, 0),
            follow_selected_particle: false,

            history: VecDeque::new(),
            selected_history_entry: 0,

            calculation_time: 0,

            words,

            ui_send,
            sim_rcv,

            simulation_handle,
        }
    }

    fn apply_seed(&mut self) {
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

        for i in 0..self.shared.class_count {
            self.shared.particle_counts[i] = rand(
                RANDOM_MIN_PARTICLE_COUNT as f32,
                RANDOM_MAX_PARTICLE_COUNT as f32,
            ) as usize;
            for j in 0..self.shared.class_count {
                let pow = rand(MIN_FORCE, MAX_FORCE);
                self.shared.param_matrix[(i, j)].force = pow.signum() * pow.abs().powf(1. / POW_F);
                self.shared.param_matrix[(i, j)].radius =
                    rand(MIN_RADIUS, MAX_RADIUS).powf(1. / RAD_F);
            }
        }

        self.send_params();
        self.send_class_count();
        self.send_particle_counts();
    }

    fn send_params(&self) {
        self.ui_send
            .send(UiEvent::ParamsUpdate(self.shared.param_matrix.to_owned()))
            .unwrap();
    }
    fn send_class_count(&self) {
        self.ui_send
            .send(UiEvent::ClassCountUpdate(self.shared.class_count))
            .unwrap();
    }
    fn send_particle_counts(&self) {
        self.ui_send
            .send(UiEvent::ParticleCountsUpdate(
                self.shared.particle_counts.to_owned(),
            ))
            .unwrap();
    }
    fn send_world_radius(&self) {
        self.ui_send
            .send(UiEvent::WorldRadiusUpdate(self.shared.world_radius))
            .unwrap();
    }

    fn export(&self) -> String {
        let mut bytes: Vec<u8> = Vec::new();
        bytes
            .write_u16::<LE>(self.shared.world_radius as u16)
            .unwrap();
        bytes.write_u8(self.shared.class_count as u8).unwrap();
        for count in &self.shared.particle_counts {
            bytes.write_u16::<LE>(*count as u16).unwrap();
        }
        self.shared
            .param_matrix
            .elements_row_major_iter()
            .for_each(|p| {
                bytes.write_i8(p.force as i8).unwrap();
                bytes.write_i8(p.radius as i8).unwrap();
            });

        format!("@{}", base64::encode(bytes))
    }

    fn import(&mut self, mut bytes: &[u8]) {
        self.shared.world_radius = bytes
            .read_u16::<LE>()
            .unwrap_or(DEFAULT_WORLD_RADIUS as u16) as f32;
        self.shared.class_count = bytes.read_u8().unwrap_or(MAX_CLASSES as u8) as usize;
        for count in &mut self.shared.particle_counts {
            // let r = (bytes.read_u8().unwrap_or((p.color.r() * 255.) as u8) as f32) / 255.;
            // let g = (bytes.read_u8().unwrap_or((p.color.g() * 255.) as u8) as f32) / 255.;
            // let b = (bytes.read_u8().unwrap_or((p.color.b() * 255.) as u8) as f32) / 255.;
            // p.color = Rgba::from_rgb(r, g, b);
            *count = bytes.read_u16::<LE>().unwrap_or(0) as usize;
        }

        for i in 0..MAX_CLASSES {
            for j in 0..MAX_CLASSES {
                self.shared.param_matrix[(i, j)].force = bytes.read_i8().unwrap_or(0) as f32;
                self.shared.param_matrix[(i, j)].radius = bytes.read_i8().unwrap_or(0) as f32;
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

impl UpdateSharedState for Smarticles {
    fn play(&mut self) {
        self.shared.simulation_state = SimulationState::Running;
        self.ui_send.send(UiEvent::Play).unwrap();
    }
    fn pause(&mut self) {
        self.shared.simulation_state = SimulationState::Paused;
        self.ui_send.send(UiEvent::Pause).unwrap();
    }
    fn reset(&mut self) {
        self.shared.simulation_state = SimulationState::Stopped;
        self.ui_send.send(UiEvent::Reset).unwrap();
    }
    fn spawn(&mut self) {
        self.ui_send.send(UiEvent::Spawn).unwrap();
    }
}

impl App for Smarticles {
    fn update(&mut self, ctx: &Context, frame: &mut Frame) {
        if let Some(SimResults(elapsed, positions)) = self.sim_rcv.try_iter().last() {
            if let Some(elapsed) = elapsed {
                self.calculation_time = elapsed.as_millis();
            }
            self.particle_positions = positions;
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

                if self.shared.simulation_state == SimulationState::Running {
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
                    self.ui_send.send(UiEvent::Quit).unwrap();
                    if let Some(handle) = self.simulation_handle.take() {
                        handle.join().unwrap();
                    }
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
                    &mut self.shared.world_radius,
                    MIN_WORLD_RADIUS..=MAX_WORLD_RADIUS,
                ));
                let reset = ui.button("reset");
                if reset.clicked() {
                    self.shared.world_radius = DEFAULT_WORLD_RADIUS;
                }
                if world_radius.changed() || reset.clicked() {
                    self.seed = self.export();
                    self.spawn();

                    self.send_world_radius();
                }
            });

            ui.horizontal(|ui| {
                ui.label("particle classes:");
                let class_count = ui.add(Slider::new(
                    &mut self.shared.class_count,
                    MIN_CLASSES..=MAX_CLASSES,
                ));
                let reset = ui.button("reset");
                if reset.clicked() {
                    self.shared.class_count = MAX_CLASSES;
                }
                if class_count.changed() || reset.clicked() {
                    self.seed = self.export();
                    self.spawn();

                    self.send_class_count();
                }
            });

            ui.horizontal(|ui| {
                ui.label("total particle count:");

                let total_particle_count: usize = self.shared.particle_counts.iter().sum();
                ui.code(total_particle_count.to_string());
            });

            ui.horizontal(|ui| {
                ui.label("calculation time:");
                ui.code(self.calculation_time.to_string() + "ms");
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
                        self.seed = self.history[self.selected_history_entry].to_owned();
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
                        0..=(self.shared.particle_counts[self.selected_particle.0] - 1),
                    ));
                });

                ui.horizontal(|ui| {
                    ui.label("position:");
                    ui.code(format!(
                        "{:?}",
                        self.particle_positions[self.selected_particle]
                    ));
                });

                // ui.horizontal(|ui| {
                //     ui.label("velocity:");
                //     ui.code(
                //         self.particles[self.selected_particle]
                //             .vel
                //             .length()
                //             .to_string(),
                //     );
                //     ui.code(format!("{:?}", self.particles[self.selected_particle].vel));
                // });

                ui.horizontal(|ui| {
                    if self.follow_selected_particle {
                        if ui.button("stop following selected particle").clicked() {
                            self.follow_selected_particle = false;
                        }
                    } else {
                        if ui.button("follow selected particle").clicked() {
                            self.follow_selected_particle = true;
                        }
                    }
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
                                get_partial_velocity(
                                    Vec2::new(x, 0.),
                                    self.shared.param_matrix[self.selected_param].radius,
                                    self.shared.param_matrix[self.selected_param].force
                                        * FORCE_FACTOR,
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
                for i in 0..self.shared.class_count {
                    ui.add_space(10.);
                    ui.colored_label(self.classes[i].color, &self.classes[i].heading);
                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.label("particle count:");
                        if ui
                            .add(Slider::new(
                                &mut self.shared.particle_counts[i],
                                MIN_PARTICLE_COUNT..=MAX_PARTICLE_COUNT,
                            ))
                            .changed()
                        {
                            self.seed = self.export();
                            self.spawn();

                            self.send_particle_counts();
                        }
                    });

                    ui.collapsing(self.classes[i].heading.to_owned() + " params", |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                for j in 0..self.shared.class_count {
                                    ui.horizontal(|ui| {
                                        ui.label("force (");
                                        ui.colored_label(
                                            self.classes[j].color,
                                            &self.classes[j].name,
                                        );
                                        ui.label(")");
                                        if ui
                                            .add(Slider::new(
                                                &mut self.shared.param_matrix[(i, j)].force,
                                                MIN_FORCE..=MAX_FORCE,
                                            ))
                                            .changed()
                                        {
                                            self.selected_param = (i, j);
                                            self.seed = self.export();

                                            self.send_params();
                                        }
                                    });
                                }
                            });
                            ui.vertical(|ui| {
                                for j in 0..self.shared.class_count {
                                    ui.horizontal(|ui| {
                                        ui.label("radius (");
                                        ui.colored_label(
                                            self.classes[j].color,
                                            &self.classes[j].name,
                                        );
                                        ui.label(")");
                                        if ui
                                            .add(Slider::new(
                                                &mut self.shared.param_matrix[(i, j)].radius,
                                                MIN_RADIUS..=MAX_RADIUS,
                                            ))
                                            .changed()
                                        {
                                            self.selected_param = (i, j);
                                            self.seed = self.export();

                                            self.send_params();
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
                            self.view.drag_start_pos = interact_pos.to_vec2();
                            self.view.drag_start_view_pos = self.view.pos;
                        }
                    } else {
                        self.view.dragging = false;
                    }
                }

                if self.view.dragging {
                    let drag_delta =
                        ctx.input().pointer.interact_pos().unwrap() - self.view.drag_start_pos;
                    self.view.pos =
                        self.view.drag_start_view_pos + drag_delta.to_vec2() / self.view.zoom;
                }

                if self.follow_selected_particle {
                    self.view.pos +=
                        self.view.prev_follow_pos - self.particle_positions[self.selected_particle];
                    self.view.prev_follow_pos = self.particle_positions[self.selected_particle];
                }

                let diag = Vec2::new(self.shared.world_radius, self.shared.world_radius);

                let min = resp.rect.min
                    + Vec2::new(resp.rect.width(), resp.rect.height()) / 2.
                    + (-diag + self.view.pos) * self.view.zoom;

                paint.circle_stroke(
                    min + diag * self.view.zoom,
                    (self.shared.world_radius + 60.) * self.view.zoom,
                    Stroke {
                        width: 1.,
                        color: Color32::from_rgb(200, 200, 200),
                    },
                );

                let center = min + diag * self.view.zoom;

                for c in 0..self.shared.class_count {
                    let class = &self.classes[c];
                    let col: Color32 = class.color.into();

                    for p in 0..self.shared.particle_counts[c] {
                        let pos = center + self.particle_positions[(c, p)] * self.view.zoom;
                        if paint.clip_rect().contains(pos) {
                            paint.circle_filled(pos, PARTICLE_DIAMETER, col);
                        }
                    }
                }

                if self.shared.simulation_state != SimulationState::Stopped {
                    paint.text(
                        center + self.particle_positions[self.selected_particle] * self.view.zoom,
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
