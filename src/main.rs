use std::array;
use std::collections::hash_map::DefaultHasher;
use std::f32::consts::TAU;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use byteorder::{ReadBytesExt, WriteBytesExt, LE};
use eframe::epaint::Color32;
use eframe::{App, Frame, NativeOptions};
use egui::{CentralPanel, Context, Pos2, Rgba, ScrollArea, Sense, SidePanel, Slider, Stroke, Vec2};
use rand::distributions::OpenClosed01;
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

/// Tick per second: update rate of the simulation.
const TPS: f32 = 1. / 90.;
/// Frame per second: update rate of the UI.
const FPS: f32 = 1. / 60.;

/// Minimum number of particle types in the simulation.
const MIN_TYPES: usize = 3;
/// Maximum number of particle types in the simulation.
const MAX_TYPES: usize = 8;

/// Size of the particles in the simulation.
const PARTICLE_SIZE: f32 = 2.;

/// Default world width the simulation.
const DEFAULT_WORLD_RADIUS: f32 = 1000.;
/// Minimum world width the simulation.
const MIN_WORLD_RADIUS: f32 = 200.;
/// Maximum world width the simulation.
const MAX_WORLD_RADIUS: f32 = DEFAULT_WORLD_RADIUS * 1.5;

const DEFAULT_SPAWN_RADIUS: f32 = 20.;

/// Minimum particle count.
const MIN_COUNT: usize = 0;
/// When randomizing particle counts, this is the lowest
/// value possible, this prevent particle counts from being
/// under this value.
const RANDOM_MIN_COUNT: usize = 50;

/// Total minimum number of particles in the simulation.
const MIN_TOTAL_COUNT: usize = RANDOM_MIN_COUNT * MAX_TYPES;
/// Total maximum number of particles in the simulation.
const MAX_TOTAL_COUNT: usize = 12000;
/// Default total maximum number of particles in the simulation.
const DEFAULT_MAX_TOTAL_COUNT: usize = 8000;

const DEFAULT_POWER: f32 = 0.;
const MAX_POWER: f32 = 100.;
const MIN_POWER: f32 = -MAX_POWER;
/// Scales power.
const POWER_FACTOR: f32 = 1. / 500.;

const DEFAULT_RADIUS: f32 = 80.;
const MIN_RADIUS: f32 = 5.;
const MAX_RADIUS: f32 = 100.;
/// Below this radius, particles repel each other (see [`get_dv`]).
const MID_RADIUS: f32 = 40.;
/// The power with which the particles repel each other when
/// below [`MIN_RADIUS`]. It is scaled depending on the distance
/// between particles (see [`get_dv`]).
const CLOSE_POWER: f32 = 20.;

const DEFAULT_SPEED_FACTOR: f32 = 40.;
const MIN_SPEED_FACTOR: f32 = 2.;
const MAX_SPEED_FACTOR: f32 = 80.;

const DEFAULT_DAMPING_FACTOR: f32 = 0.5;

const DEFAULT_ZOOM: f32 = 1.;
const MIN_ZOOM: f32 = 0.5;
const MAX_ZOOM: f32 = 10.;
const ZOOM_FACTOR: f32 = 0.02;

const HISTORY_LENGTH: usize = 100;

fn main() {
    let options = NativeOptions {
        // initial_window_size: Some(Vec2::new(1600., 900.)),
        fullscreen: true,
        ..Default::default()
    };

    let smarticles = Smarticles::new([
        ("α", Rgba::from_srgba_unmultiplied(255, 0, 0, 255)),
        ("β", Rgba::from_srgba_unmultiplied(255, 140, 0, 255)),
        ("γ", Rgba::from_srgba_unmultiplied(225, 255, 0, 255)),
        ("δ", Rgba::from_srgba_unmultiplied(68, 255, 0, 255)),
        ("ε", Rgba::from_srgba_unmultiplied(0, 247, 255, 255)),
        ("ζ", Rgba::from_srgba_unmultiplied(40, 60, 255, 255)),
        ("η", Rgba::from_srgba_unmultiplied(166, 0, 255, 255)),
        ("θ", Rgba::from_srgba_unmultiplied(247, 0, 243, 255)),
    ]);

    // smarticles.

    eframe::run_native("Smarticles", options, Box::new(|_| Box::new(smarticles)));
}

struct Smarticles {
    world_radius: f32,

    play: bool,
    type_count: usize,
    max_total_count: usize,
    speed_factor: f32,
    seed: String,
    history: History,

    params: [Params; MAX_TYPES],
    dots: [Vec<Dot>; MAX_TYPES],
    prev_time: Instant,
    prev_frame_time: Instant,
    view: View,
    words: Vec<String>,
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

struct Params {
    name: String,
    heading: String,
    color: Rgba,
    count: usize,
    power: [f32; MAX_TYPES],
    radius: [f32; MAX_TYPES],
}

struct History {
    values: [String; HISTORY_LENGTH],
    current: usize,
}

impl History {
    pub fn new() -> Self {
        Self {
            values: array::from_fn(|_| String::new()),
            current: 0,
        }
    }

    pub fn add<S>(&mut self, value: S)
    where
        S: ToString,
    {
        self.current = (self.current + 1) % HISTORY_LENGTH;
        self.values[self.current] = value.to_string();
    }

    pub fn prev(&mut self) -> String {
        if self.current != 0 {
            self.current -= 1;
        }
        self.values[self.current].to_owned()
    }
}

#[derive(Clone, Copy)]
struct Dot {
    pos: Vec2,
    vel: Vec2,
}

impl Smarticles {
    fn new<S>(types: [(S, Rgba); MAX_TYPES]) -> Self
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

            play: false,
            type_count: MAX_TYPES,
            max_total_count: DEFAULT_MAX_TOTAL_COUNT,
            speed_factor: DEFAULT_SPEED_FACTOR,
            seed: "".to_string(),
            history: History::new(),

            params: types.map(|(name, color)| Params {
                name: name.to_string(),
                heading: "Type ".to_string() + &name.to_string(),
                color,
                count: 0,
                power: [DEFAULT_POWER; MAX_TYPES],
                radius: [DEFAULT_RADIUS; MAX_TYPES],
            }),
            dots: std::array::from_fn(|_| Vec::new()),
            prev_time: Instant::now(),
            prev_frame_time: Instant::now(),
            view: View::DEFAULT,
            words,
        }
    }

    fn play(&mut self) {
        self.prev_time = Instant::now();
        self.play = true;
    }

    fn stop(&mut self) {
        self.play = false;
    }

    fn restart(&mut self) {
        self.world_radius = DEFAULT_WORLD_RADIUS;
        // self.world_h = DEFAULT_HEIGHT;
        self.max_total_count = DEFAULT_MAX_TOTAL_COUNT;
        self.speed_factor = DEFAULT_SPEED_FACTOR;
        self.view = View::DEFAULT;
        for p in &mut self.params {
            p.count = 0;
            p.radius.iter_mut().for_each(|r| *r = DEFAULT_RADIUS);
            p.power.iter_mut().for_each(|p| *p = DEFAULT_POWER);
        }
        self.clear();
    }

    fn clear(&mut self) {
        for i in 0..MAX_TYPES {
            self.dots[i].clear();
        }
    }

    fn spawn(&mut self) {
        self.clear();

        let mut rand = SmallRng::from_entropy();

        for i in 0..self.type_count {
            self.dots[i].clear();
            for _ in 0..self.params[i].count {
                self.dots[i].push(Dot {
                    pos: Vec2::new(self.world_radius, self.world_radius)
                        + Vec2::angled(TAU * rand.sample::<f32, _>(OpenClosed01))
                            * DEFAULT_SPAWN_RADIUS
                            * rand.sample::<f32, _>(OpenClosed01),
                    vel: Vec2::ZERO,
                });
            }
        }
    }

    fn simulate(&mut self, dt: f32) {
        let dots_clone = self.dots.to_owned();
        for i in 0..self.type_count {
            for j in 0..self.type_count {
                let g = -self.params[i].power[j] * POWER_FACTOR;
                self.dots[i].par_iter_mut().for_each(|p1| {
                    let mut v = Vec2::ZERO;
                    for p2 in dots_clone[j].iter() {
                        v += get_dv(p2.pos - p1.pos, self.params[i].radius[j], g);
                    }

                    p1.vel = (p1.vel + v) * DEFAULT_DAMPING_FACTOR;
                    p1.pos += p1.vel * self.speed_factor * dt;

                    let d = (Vec2::new(self.world_radius, self.world_radius)) - p1.pos;
                    if d.length() >= self.world_radius {
                        p1.pos = Vec2::new(self.world_radius, self.world_radius)
                            - d.normalized() * self.world_radius;
                        p1.vel = d.normalized() * 10.;
                    }
                });
            }
        }

        // previous
        // if p1.pos.x < 0. {
        //     p1.pos.x = 0.;
        //     p1.vel.x = 10.;
        // } else if p1.pos.x >= world_w {
        //     p1.pos.x = world_w;
        //     p1.vel.x = -10.;
        // }
        // if p1.pos.y < 0. {
        //     p1.pos.y = 0.;
        //     p1.vel.y = 10.;
        // } else if p1.pos.y >= world_h {
        //     p1.pos.y = world_h;
        //     p1.vel.y = -10.;
        // }

        // previous previous
        // if (p1.pos.x < 10. && p1.vel.x < 0.) || (p1.pos.x > world_w - 10. && p1.vel.x > 0.) {
        //     p1.vel.x *= -8.;
        // }
        // if (p1.pos.y < 10. && p1.vel.y < 0.) || (p1.pos.y > world_h - 10. && p1.vel.y > 0.) {
        //     p1.vel.y *= -8.;
        // }

        // previous previous alternative: wrap
        // if p1.pos.x < 0. {
        //     p1.pos.x += world_w;
        // } else if p1.pos.x >= world_w {
        //     p1.pos.x -= world_w;
        // }
        // if p1.pos.y < 0. {
        //     p1.pos.y += world_h;
        // } else if p1.pos.y >= world_h {
        //     p1.pos.y -= world_h;
        // }
    }

    fn apply_seed(&mut self) {
        self.clear();

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
        let mut rand = |min: f32, max: f32| min + (max - min) * rand.sample::<f32, _>(OpenClosed01);

        const POW_F: f32 = 1.25;
        const RAD_F: f32 = 1.1;

        for i in 0..self.type_count {
            self.params[i].count = rand(
                RANDOM_MIN_COUNT as f32,
                (self.max_total_count / self.type_count) as f32,
            ) as usize;
            for j in 0..self.type_count {
                let pow = rand(MIN_POWER, MAX_POWER);
                self.params[i].power[j] = if pow >= 0. {
                    pow.powf(1. / POW_F)
                } else {
                    -pow.abs().powf(1. / POW_F)
                };
                //self.params[i].power[j] = rand(MIN_POWER, MAX_POWER);
                self.params[i].radius[j] = rand(MIN_RADIUS, MAX_RADIUS).powf(1. / RAD_F);
            }
        }
    }

    fn export(&self) -> String {
        let mut bytes: Vec<u8> = Vec::new();
        bytes.write_u16::<LE>(self.world_radius as u16).unwrap();
        bytes.write_u8(self.type_count as u8).unwrap();
        bytes.write_u16::<LE>(self.speed_factor as u16).unwrap();
        for p in &self.params {
            bytes.write_u8((p.color.r() * 255.) as u8).unwrap();
            bytes.write_u8((p.color.g() * 255.) as u8).unwrap();
            bytes.write_u8((p.color.b() * 255.) as u8).unwrap();
            bytes.write_u16::<LE>(p.count as u16).unwrap();
            for &p in &p.power {
                bytes.write_i8(p as i8).unwrap();
            }
            for &r in &p.radius {
                bytes.write_u16::<LE>(r as u16).unwrap();
            }
        }
        format!("@{}", base64::encode(bytes))
    }

    fn import(&mut self, mut bytes: &[u8]) {
        self.world_radius = bytes
            .read_u16::<LE>()
            .unwrap_or(DEFAULT_WORLD_RADIUS as u16) as f32;
        self.type_count = bytes.read_u8().unwrap_or(MAX_TYPES as u8) as usize;
        self.speed_factor = bytes
            .read_u16::<LE>()
            .unwrap_or(DEFAULT_SPEED_FACTOR as u16) as f32;
        for p in &mut self.params {
            let r = (bytes.read_u8().unwrap_or((p.color.r() * 255.) as u8) as f32) / 255.;
            let g = (bytes.read_u8().unwrap_or((p.color.g() * 255.) as u8) as f32) / 255.;
            let b = (bytes.read_u8().unwrap_or((p.color.b() * 255.) as u8) as f32) / 255.;
            p.color = Rgba::from_rgb(r, g, b);
            p.count = bytes.read_u16::<LE>().unwrap_or(0) as usize;
            for p in &mut p.power {
                *p = bytes.read_i8().unwrap_or(0) as f32;
            }
            for r in &mut p.radius {
                *r = bytes.read_u16::<LE>().unwrap_or(0) as f32;
            }
        }
    }
}

fn get_dv(distance: Vec2, action_radius: f32, power: f32) -> Vec2 {
    match distance.length() {
        r if r < action_radius && r > MID_RADIUS => distance / r * power,
        r if r < MID_RADIUS && r > 0. => {
            distance / r * (((CLOSE_POWER / MID_RADIUS) * r - CLOSE_POWER) * POWER_FACTOR)
        }
        // r if r < 50. && r > 0. => distance.normalized() * (50. - r),
        _ => return Vec2::ZERO,
    }
}

impl App for Smarticles {
    fn update(&mut self, ctx: &Context, frame: &mut Frame) {
        if self.play {
            let time = Instant::now();
            let dt = time - self.prev_time;
            if dt > Duration::from_secs_f32(TPS) {
                self.simulate(dt.as_secs_f32());
                self.prev_time = time;
            }
        }

        let frame_time = Instant::now();
        let dt = frame_time - self.prev_frame_time;
        if dt > Duration::from_secs_f32(FPS) {
            ctx.request_repaint();
            self.prev_frame_time = frame_time;
        }

        SidePanel::left("settings").show(ctx, |ui| {
            ui.heading("Settings");
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Respawn").clicked() {
                    self.spawn();
                }

                if self.play {
                    if ui.button("Pause").clicked() {
                        self.stop();
                    }
                } else if ui.button("Play").clicked() {
                    self.play();
                }

                if ui.button("Randomize").clicked() {
                    let w1 = rand::random::<usize>() % self.words.len();
                    let w2 = rand::random::<usize>() % self.words.len();
                    let w3 = rand::random::<usize>() % self.words.len();
                    self.seed = format!("{}_{}_{}", self.words[w1], self.words[w2], self.words[w3]);

                    self.apply_seed();
                    self.history.add(self.seed.to_owned());
                    self.spawn();
                }
                if ui.button("Previous Seed").clicked() {
                    self.seed = self.history.prev();
                    self.apply_seed();
                    self.spawn();
                }

                if ui.button("Reset View").clicked() {
                    self.view = View::DEFAULT;
                }

                if ui.button("Reset").clicked() {
                    self.restart();
                }

                if ui.button("Quit").clicked() {
                    frame.close();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Seed:");
                if ui.text_edit_singleline(&mut self.seed).changed() {
                    self.apply_seed();
                    self.history.add(self.seed.to_owned());
                    self.spawn();
                    self.stop();
                }
            });

            ui.horizontal(|ui| {
                ui.label("World Radius:");
                let world_radius = ui.add(Slider::new(
                    &mut self.world_radius,
                    MIN_WORLD_RADIUS..=MAX_WORLD_RADIUS,
                ));
                let reset = ui.button("Reset");
                if reset.clicked() {
                    self.world_radius = DEFAULT_WORLD_RADIUS;
                }
                if world_radius.changed() || reset.clicked() {
                    self.seed = self.export();
                    self.spawn();
                }
            });
            // ui.horizontal(|ui| {
            //     ui.label("World Height:");
            //     let world_h = ui.add(Slider::new(&mut self.world_h, MIN_WORLD_H..=MAX_WORLD_H));
            //     let reset = ui.button("Reset");
            //     if reset.clicked() {
            //         self.world_h = DEFAULT_HEIGHT;
            //     }
            //     if world_h.changed() || reset.clicked() {
            //         self.seed = self.export();
            //         self.spawn();
            //     }
            // });
            ui.horizontal(|ui| {
                ui.label("Speed Factor:");
                let speed_factor = ui.add(Slider::new(
                    &mut self.speed_factor,
                    MIN_SPEED_FACTOR..=MAX_SPEED_FACTOR,
                ));
                let reset = ui.button("Reset");
                if reset.clicked() {
                    self.speed_factor = DEFAULT_SPEED_FACTOR;
                }
                if speed_factor.changed() || reset.clicked() {
                    self.seed = self.export();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Particle Types:");
                let type_count = ui.add(Slider::new(&mut self.type_count, MIN_TYPES..=MAX_TYPES));
                let reset = ui.button("Reset");
                if reset.clicked() {
                    self.type_count = MAX_TYPES;
                }
                if type_count.changed() || reset.clicked() {
                    self.seed = self.export();
                    self.spawn();
                }
            });
            ui.horizontal(|ui| {
                let max_total_count = ui.label("Maximum Total Particle Count:");
                ui.add(Slider::new(
                    &mut self.max_total_count,
                    MIN_TOTAL_COUNT..=MAX_TOTAL_COUNT,
                ));
                let reset = ui.button("Reset");
                if reset.clicked() {
                    self.max_total_count = DEFAULT_MAX_TOTAL_COUNT;
                }
                if max_total_count.changed() || reset.clicked() {
                    self.spawn();
                }
            });

            ScrollArea::vertical().show(ui, |ui| {
                for i in 0..self.type_count {
                    ui.add_space(10.);
                    ui.colored_label(self.params[i].color, &self.params[i].heading);
                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.label("Color:");
                        let mut rgb = [
                            self.params[i].color.r(),
                            self.params[i].color.g(),
                            self.params[i].color.b(),
                        ];
                        if ui.color_edit_button_rgb(&mut rgb).changed() {
                            self.params[i].color = Rgba::from_rgb(rgb[0], rgb[1], rgb[2]);
                            self.seed = self.export();
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Count:");
                        if ui
                            .add(Slider::new(
                                &mut self.params[i].count,
                                MIN_COUNT..=(self.max_total_count / self.type_count),
                            ))
                            .changed()
                        {
                            self.seed = self.export();
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            for j in 0..self.type_count {
                                ui.horizontal(|ui| {
                                    ui.label("Power (");
                                    ui.colored_label(self.params[j].color, &self.params[j].name);
                                    ui.label(")");
                                    if ui
                                        .add(Slider::new(
                                            &mut self.params[i].power[j],
                                            MIN_POWER..=MAX_POWER,
                                        ))
                                        .changed()
                                    {
                                        self.seed = self.export();
                                    }
                                });
                            }
                        });
                        ui.vertical(|ui| {
                            for j in 0..self.type_count {
                                ui.horizontal(|ui| {
                                    ui.label("Radius (");
                                    ui.colored_label(self.params[j].color, &self.params[j].name);
                                    ui.label(")");
                                    if ui
                                        .add(Slider::new(
                                            &mut self.params[i].radius[j],
                                            MIN_RADIUS..=MAX_RADIUS,
                                        ))
                                        .changed()
                                    {
                                        self.seed = self.export();
                                    }
                                });
                            }
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

                let mut min = resp.rect.min
                    + Vec2::new(
                        (resp.rect.width() / 2.) - self.world_radius * self.view.zoom,
                        (resp.rect.height() / 2.) - self.world_radius * self.view.zoom,
                    );

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

                min += self.view.pos.to_vec2() * self.view.zoom;

                paint.circle_stroke(
                    min + Vec2::new(self.world_radius, self.world_radius) * self.view.zoom,
                    self.world_radius * self.view.zoom,
                    Stroke {
                        width: 1.,
                        color: Color32::from_rgb(200, 200, 200),
                    },
                );

                for i in 0..self.type_count {
                    let p = &self.params[i];
                    let col: Color32 = p.color.into();
                    for dot in &self.dots[i] {
                        let pos = min + dot.pos * self.view.zoom;
                        if paint.clip_rect().contains(pos) {
                            paint.circle_filled(
                                pos,
                                (PARTICLE_SIZE / 2.) * self.view.zoom.sqrt(),
                                col,
                            );
                        }
                    }
                }
            });
    }
}
