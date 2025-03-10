use std::{
    collections::{hash_map::DefaultHasher, VecDeque},
    hash::{Hash, Hasher},
    sync::mpsc::Receiver,
    thread::JoinHandle,
};

use byteorder::{ReadBytesExt, WriteBytesExt};
use eframe::{
    egui::{
        Align2, Area, CentralPanel, ComboBox, Context, FontId, PointerButton, ScrollArea, Sense,
        SidePanel, Slider, Vec2,
    },
    epaint::Color32,
    App, Frame,
};
use egui_plot::{Line, Plot, PlotPoints};
use rand::{distributions::Open01, rngs::SmallRng, Rng, SeedableRng};
use rayon::prelude::*;

use crate::{
    consts::{
        CLASS_COUNT, DEFAULT_PARTICLE_COUNT, DEFAULT_ZOOM, MAX_HISTORY_LEN, MAX_PARTICLE_COUNT,
        MAX_POWER, MAX_ZOOM, MIN_PARTICLE_COUNT, MIN_POWER, MIN_ZOOM, PARTICLE_DIAMETER,
        ZOOM_FACTOR,
    },
    events::{Event, StateUpdate},
    mat::Mat2D,
    simulation_manager::SimulationState,
    Senders,
};

#[cfg(feature = "cell_map_display")]
use {
    crate::simulation::Cell,
    eframe::egui::{Rect, Rounding, Stroke},
};

pub struct View {
    zoom: f32,
    pos: Vec2,
    dragging: bool,
    drag_start_pos: Vec2,
    drag_start_view_pos: Vec2,
}

impl View {
    const DEFAULT: View = Self {
        zoom: DEFAULT_ZOOM,
        pos: Vec2::ZERO,
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
    enabled: bool,
}

pub struct SmarticlesApp {
    classes: [ClassProps; CLASS_COUNT],

    seed: String,

    show_ui: bool,

    view: View,

    selected_particle: (usize, usize),
    follow_selected_particle: bool,

    history: VecDeque<String>,
    selected_history_entry: usize,

    computation_time_graph: VecDeque<f32>,

    particle_counts: [usize; CLASS_COUNT],
    particle_positions: Mat2D<Vec2>,
    power_matrix: Mat2D<i8>,
    simulation_state: SimulationState,

    #[cfg(feature = "cell_map_display")]
    cell_map: Option<Vec<Cell>>,

    senders: Senders,
    receiver: Receiver<Event>,

    simulation_handle: Option<JoinHandle<()>>,

    words: Vec<String>,
}

impl SmarticlesApp {
    pub fn new<S>(
        classes: [(S, Color32); CLASS_COUNT],
        senders: Senders,
        receiver: Receiver<Event>,
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

        let mut app = Self {
            seed: "".to_string(),

            classes: classes.map(|(name, color)| ClassProps {
                name: name.to_string(),
                heading: "class ".to_string() + &name.to_string(),
                color,
                enabled: true,
            }),

            show_ui: true,

            view: View::DEFAULT,

            selected_particle: (0, 0),
            follow_selected_particle: false,

            history: VecDeque::new(),
            selected_history_entry: 0,

            computation_time_graph: VecDeque::new(),

            particle_counts: [DEFAULT_PARTICLE_COUNT; CLASS_COUNT],
            particle_positions: Mat2D::filled_with(Vec2::ZERO, CLASS_COUNT, MAX_PARTICLE_COUNT),
            power_matrix: Mat2D::filled_with(0, CLASS_COUNT, CLASS_COUNT),
            simulation_state: SimulationState::Paused,

            #[cfg(feature = "cell_map_display")]
            cell_map: None,

            senders,
            receiver,

            simulation_handle,

            words,
        };

        let w1 = rand::random::<usize>() % app.words.len();
        let w2 = rand::random::<usize>() % app.words.len();
        let w3 = rand::random::<usize>() % app.words.len();
        app.seed = format!("{}_{}_{}", app.words[w1], app.words[w2], app.words[w3]);

        app.apply_seed();
        app.senders.send_sim(Event::StateUpdate(
            StateUpdate::new()
                .power_matrix(&app.power_matrix)
                .particle_counts(&app.particle_counts),
        ));
        app.senders.send_sim(Event::SpawnParticles);

        app
    }

    fn apply_seed(&mut self) {
        let mut rand = if self.seed.is_empty() {
            SmallRng::from_entropy()
        } else {
            if self.seed.starts_with('@') {
                if let Ok(bytes) = base64::decode(&self.seed[1..]) {
                    self.apply_custom_seed(&bytes);

                    return;
                }
            }
            let mut hasher = DefaultHasher::new();
            self.seed.hash(&mut hasher);
            SmallRng::seed_from_u64(hasher.finish())
        };
        let mut rand = |min: f32, max: f32| min + (max - min) * rand.sample::<f32, _>(Open01);

        const POW_F: f32 = 1.25;

        for i in 0..CLASS_COUNT {
            for j in 0..CLASS_COUNT {
                let pow = rand(MIN_POWER as f32, MAX_POWER as f32);
                self.power_matrix[(i, j)] = (pow.signum() * pow.abs().powf(1. / POW_F)) as i8;
            }
        }
    }
    fn export_custom_seed(&self) -> String {
        let mut bytes: Vec<u8> = Vec::new();
        self.power_matrix
            .vec()
            .iter()
            .copied()
            .for_each(|power| bytes.write_i8(power).unwrap());

        format!("@{}", base64::encode(bytes))
    }
    fn apply_custom_seed(&mut self, mut bytes: &[u8]) {
        for i in 0..CLASS_COUNT {
            for j in 0..CLASS_COUNT {
                self.power_matrix[(i, j)] = bytes.read_i8().unwrap_or(0);
            }
        }
    }

    fn update_history(&mut self) {
        if self
            .history
            .front()
            .map(|front| &self.seed != front)
            .unwrap_or(true)
        {
            self.history.push_front(self.seed.to_owned());
            if self.history.len() > MAX_HISTORY_LEN {
                self.history.pop_back();
            }
        }
        self.selected_history_entry = 0;
    }

    fn play(&mut self) {
        self.simulation_state = SimulationState::Running;
        self.senders.send_sim(Event::SimulationStart);
    }
    fn pause(&mut self) {
        self.simulation_state = SimulationState::Paused;
        self.senders.send_sim(Event::SimulationPause);
    }
    fn reset(&mut self) {
        self.simulation_state = SimulationState::Paused;
        self.senders.send_sim(Event::SimulationPause);
        self.particle_counts = [DEFAULT_PARTICLE_COUNT; CLASS_COUNT];
        for i in 0..CLASS_COUNT {
            for j in 0..CLASS_COUNT {
                self.power_matrix[(i, j)] = 0;
            }
        }
        self.senders.send_sim(Event::StateUpdate(
            StateUpdate::new()
                .power_matrix(&self.power_matrix)
                .particle_counts(&self.particle_counts),
        ));
        self.senders.send_sim(Event::SpawnParticles);
    }
}

impl App for SmarticlesApp {
    fn update(&mut self, ctx: &Context, _: &mut Frame) {
        let events = self.receiver.try_iter();
        for event in events {
            if let Event::StateUpdate(StateUpdate {
                particle_positions,
                computation_time,

                power_matrix,
                particle_counts,

                #[cfg(feature = "cell_map_display")]
                cell_map,
            }) = event
            {
                if let Some(particle_positions) = particle_positions {
                    self.particle_positions = particle_positions;
                }
                if let Some(computation_time) = computation_time {
                    self.computation_time_graph
                        .push_front(computation_time.as_secs_f32() * 1000.);
                    if self.computation_time_graph.len() > 200 {
                        self.computation_time_graph.truncate(200);
                    }
                }

                if let Some(power_matrix) = power_matrix {
                    self.power_matrix = power_matrix;
                }
                if let Some(particle_counts) = particle_counts {
                    self.particle_counts = particle_counts;
                }

                #[cfg(feature = "cell_map_display")]
                if let Some(cell_map) = cell_map {
                    self.cell_map = Some(cell_map);
                }
            }
        }

        if self.show_ui {
            SidePanel::left("settings").show(ctx, |ui| {
                ui.heading("settings");
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .button("respawn")
                        .on_hover_text("spawn particles again")
                        .clicked()
                    {
                        self.senders.send_sim(Event::SpawnParticles);
                    }

                    if self.simulation_state == SimulationState::Running {
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
                        self.seed =
                            format!("{}_{}_{}", self.words[w1], self.words[w2], self.words[w3]);

                        self.update_history();

                        self.apply_seed();
                        self.senders.send_sim(Event::StateUpdate(
                            StateUpdate::new().power_matrix(&self.power_matrix),
                        ));
                        self.senders.send_sim(Event::SpawnParticles);
                    }

                    if ui
                        .button("reset view")
                        .on_hover_text("reset zoom and position")
                        .clicked()
                    {
                        self.view = View::DEFAULT;
                    }

                    if ui
                        .button("reset")
                        .on_hover_text("reset particle counts and params")
                        .clicked()
                    {
                        self.reset();
                    }

                    if ui.button("hide ui").clicked() {
                        self.show_ui = false;
                    }

                    if ui.button("quit").on_hover_text("exit smarticles").clicked() {
                        self.senders.send_sim(Event::Exit);
                        if let Some(handle) = self.simulation_handle.take() {
                            handle.join().unwrap();
                        };
                        ui.ctx()
                            .send_viewport_cmd(eframe::egui::ViewportCommand::Close);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("seed:");
                    ui.text_edit_singleline(&mut self.seed);
                    if ui.button("apply").clicked() {
                        self.update_history();

                        self.apply_seed();
                        self.senders.send_sim(Event::StateUpdate(
                            StateUpdate::new().power_matrix(&self.power_matrix),
                        ));
                        self.senders.send_sim(Event::SpawnParticles);
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("total particle count:");

                    let total_particle_count: usize = (0..CLASS_COUNT)
                        .filter_map(|c| self.classes[c].enabled.then_some(self.particle_counts[c]))
                        .sum();
                    ui.code(total_particle_count.to_string());
                });

                if let Some(dt) = self.computation_time_graph.front() {
                    ui.horizontal(|ui| {
                        ui.label("computation time:");
                        ui.code(format!("{:.1}ms", dt));
                    });

                    ui.horizontal(|ui| {
                        ui.label("average over last 200 computations:");
                        ui.code(format!(
                            "{:.1}ms",
                            self.computation_time_graph.iter().sum::<f32>()
                                / self.computation_time_graph.len() as f32
                        ));
                    });
                }

                ui.horizontal(|ui| {
                    Plot::new("computation time")
                        .show_axes(false)
                        .y_axis_label("computation time")
                        .show_x(false)
                        .height(60.)
                        .allow_drag(false)
                        .allow_zoom(false)
                        .allow_boxed_zoom(false)
                        .allow_scroll(false)
                        .allow_double_click_reset(false)
                        .label_formatter(|_, value| format!("{:.1}ms", value.y))
                        .show(ui, |ui| {
                            ui.line(Line::new(PlotPoints::from_iter(
                                self.computation_time_graph
                                    .iter()
                                    .rev()
                                    .enumerate()
                                    .map(|(i, v)| [i as f64, *v as f64]),
                            )));
                        });
                });

                if self.history.len() > 1 {
                    ui.collapsing("seed history", |ui| {
                        if ComboBox::from_id_salt("seed history")
                            .width(200.)
                            .show_index(
                                ui,
                                &mut self.selected_history_entry,
                                self.history.len(),
                                |i| self.history[i].to_owned(),
                            )
                            .changed()
                        {
                            self.history[self.selected_history_entry].clone_into(&mut self.seed);
                            self.apply_seed();
                            self.senders.send_sim(Event::StateUpdate(
                                StateUpdate::new().power_matrix(&self.power_matrix),
                            ));
                            self.senders.send_sim(Event::SpawnParticles);
                        };
                    });
                }

                ui.collapsing("particle inspector", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("class:");
                        ComboBox::from_id_salt("class").show_index(
                            ui,
                            &mut self.selected_particle.0,
                            self.classes.len(),
                            |i| self.classes[i].heading.to_owned(),
                        );
                        ui.label("particle index:");
                        ui.add(Slider::new(
                            &mut self.selected_particle.1,
                            0..=(self.particle_counts[self.selected_particle.0] - 1),
                        ));
                    });

                    ui.horizontal(|ui| {
                        ui.label("position:");
                        ui.code(format!(
                            "{:?}",
                            self.particle_positions[self.selected_particle]
                        ));
                    });

                    ui.horizontal(|ui| {
                        if self.follow_selected_particle {
                            if ui.button("stop following selected particle").clicked() {
                                self.view.pos -= self.particle_positions[self.selected_particle];
                                self.follow_selected_particle = false;
                            }
                        } else if ui.button("focus and follow selected particle").clicked() {
                            self.view.pos *= 0.;
                            self.follow_selected_particle = true;
                        }
                    });
                });

                ScrollArea::vertical().show(ui, |ui| {
                    for i in 0..CLASS_COUNT {
                        ui.add_space(10.);
                        ui.horizontal(|ui| {
                            ui.colored_label(self.classes[i].color, &self.classes[i].heading);

                            if self.classes[i].enabled {
                                if ui.button("disable").clicked() {
                                    self.classes[i].enabled = false;
                                    self.senders.send_sim(Event::DisableClass(i));
                                }
                            } else if ui.button("enable").clicked() {
                                self.classes[i].enabled = true;
                                self.senders.send_sim(Event::EnableClass(i));
                            }
                        });
                        ui.separator();

                        if self.classes[i].enabled {
                            ui.horizontal(|ui| {
                                ui.label("particle count:");
                                if ui
                                    .add(Slider::new(
                                        &mut self.particle_counts[i],
                                        MIN_PARTICLE_COUNT..=MAX_PARTICLE_COUNT,
                                    ))
                                    .changed()
                                {
                                    self.senders.send_sim(Event::StateUpdate(
                                        StateUpdate::new()
                                            .power_matrix(&self.power_matrix)
                                            .particle_counts(&self.particle_counts),
                                    ));
                                    self.senders.send_sim(Event::SpawnParticles);
                                }
                                if ui.button("reset").clicked() {
                                    self.particle_counts[i] = DEFAULT_PARTICLE_COUNT;
                                    self.senders.send_sim(Event::StateUpdate(
                                        StateUpdate::new()
                                            .power_matrix(&self.power_matrix)
                                            .particle_counts(&self.particle_counts),
                                    ));
                                    self.senders.send_sim(Event::SpawnParticles);
                                }
                            });

                            ui.collapsing(self.classes[i].heading.to_owned() + " params", |ui| {
                                ui.vertical(|ui| {
                                    for j in 0..CLASS_COUNT {
                                        if self.classes[j].enabled {
                                            ui.horizontal(|ui| {
                                                ui.label("power of the force applied on");
                                                let class_name = ui.colored_label(
                                                    self.classes[j].color,
                                                    &self.classes[j].name,
                                                );
                                                ui.add_space(5. - class_name.rect.width() / 2.);
                                                ui.label(":");
                                                ui.add_space(5. - class_name.rect.width() / 2.);
                                                if ui
                                                    .add(Slider::new(
                                                        &mut self.power_matrix[(i, j)],
                                                        MIN_POWER..=MAX_POWER,
                                                    ))
                                                    .changed()
                                                {
                                                    self.seed = self.export_custom_seed();
                                                    self.senders.send_sim(Event::StateUpdate(
                                                        StateUpdate::new()
                                                            .power_matrix(&self.power_matrix),
                                                    ));
                                                }
                                                if ui.button("reset").clicked() {
                                                    self.power_matrix[(i, j)] = 0;
                                                    self.senders.send_sim(Event::StateUpdate(
                                                        StateUpdate::new()
                                                            .power_matrix(&self.power_matrix),
                                                    ));
                                                }
                                            });
                                        }
                                    }
                                });
                            });
                        }
                    }
                });
            });
        } else {
            Area::new("show_ui_button_area".into())
                .anchor(Align2::LEFT_TOP, [10., 10.])
                .show(ctx, |ui| {
                    if ui.button("show ui").clicked() {
                        self.show_ui = true;
                    }
                });
        }

        CentralPanel::default()
            .frame(eframe::egui::Frame {
                fill: Color32::from_rgba_unmultiplied(12, 10, 10, 255),
                ..Default::default()
            })
            .show(ctx, |ui| {
                let (resp, paint) =
                    ui.allocate_painter(ui.available_size_before_wrap(), Sense::hover());

                if resp
                    .rect
                    .contains(ctx.input(|i| i.pointer.interact_pos()).unwrap_or_default())
                {
                    let scroll_delta = ctx.input(|i| i.smooth_scroll_delta).y;
                    if scroll_delta > 0. {
                        self.view.zoom *= ZOOM_FACTOR;
                    } else if scroll_delta < 0. {
                        self.view.zoom /= ZOOM_FACTOR;
                    }
                }

                self.view.zoom = self.view.zoom.clamp(MIN_ZOOM, MAX_ZOOM);

                let center = resp.rect.center().to_vec2()
                    + if self.follow_selected_particle {
                        self.view.pos - self.particle_positions[self.selected_particle]
                    } else {
                        self.view.pos
                    } * self.view.zoom;

                if let Some(interact_pos) = ctx.input(|i| i.pointer.interact_pos()) {
                    if self.view.dragging {
                        let drag_delta = interact_pos - self.view.drag_start_pos;
                        self.view.pos =
                            self.view.drag_start_view_pos + drag_delta.to_vec2() / self.view.zoom;
                    }
                    if ctx.input(|i| i.pointer.button_down(PointerButton::Primary))
                        && resp.rect.contains(interact_pos)
                    {
                        if !self.view.dragging {
                            self.view.dragging = true;
                            self.view.drag_start_pos = interact_pos.to_vec2();
                            self.view.drag_start_view_pos = self.view.pos;
                        }
                    } else {
                        self.view.dragging = false;
                    }
                }

                #[cfg(feature = "cell_map_display")]
                if let Some(cell_map) = &self.cell_map {
                    for c in cell_map {
                        let pos = (center
                            + Vec2::new(c.0 as f32, c.1 as f32) * Cell::CELL_SIZE * self.view.zoom)
                            .to_pos2();
                        paint
                            .clip_rect()
                            .extend_with(pos - Vec2::splat(Cell::CELL_SIZE * self.view.zoom));
                        paint.rect_stroke(
                            Rect::from_min_size(pos, Vec2::splat(Cell::CELL_SIZE) * self.view.zoom),
                            Rounding::ZERO,
                            Stroke::new(1., Color32::from_rgba_unmultiplied(20, 20, 20, 255)),
                        );
                    }
                }

                // Displayed particles are only collected in this vec if zoom
                // is more than 10. This guarantees that this vec is filled
                // with a small number of elements.
                let mut displayed_particles = (self.view.zoom > 10.).then_some(Vec::new());

                for c in (0..CLASS_COUNT).filter(|c| self.classes[*c].enabled) {
                    let class = &self.classes[c];

                    for p in 0..self.particle_counts[c] {
                        let pos =
                            (center + self.particle_positions[(c, p)] * self.view.zoom).to_pos2();
                        if let Some(v) = &mut displayed_particles {
                            if resp.rect.contains(pos) {
                                v.push((c, p));
                            };
                        }

                        let is_selected =
                            (c, p) == self.selected_particle && self.classes[c].enabled;

                        paint.circle_filled(
                            pos,
                            if is_selected {
                                PARTICLE_DIAMETER * 3.
                            } else {
                                PARTICLE_DIAMETER
                            } * self.view.zoom,
                            class.color,
                        );
                    }
                }

                // Prevent particles overlapping text.
                if let Some(v) = &displayed_particles {
                    if self.view.zoom > 10. {
                        for &(c, p) in v {
                            let pos = (center + self.particle_positions[(c, p)] * self.view.zoom)
                                .to_pos2();

                            let is_selected =
                                (c, p) == self.selected_particle && self.classes[c].enabled;

                            paint.text(
                                pos + Vec2::splat(if is_selected { 1.3 } else { 0.4 })
                                    * self.view.zoom,
                                Align2::LEFT_TOP,
                                format!("{}:{}", self.classes[c].name, p),
                                FontId::monospace(10.),
                                Color32::WHITE,
                            );
                        }
                    }
                }
            });

        ctx.request_repaint();
    }
}
