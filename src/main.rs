use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use byteorder::{ReadBytesExt, WriteBytesExt, LE};
use eframe::epaint::Color32;
use eframe::{App, Frame, NativeOptions};
use egui::{CentralPanel, Context, Pos2, Rgba, ScrollArea, Sense, SidePanel, Slider, Vec2};
use rand::distributions::OpenClosed01;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use rayon::prelude::*;

// IDEA Add undo/redo feature or at least a seed history (only)
// for the current session)

// IDEA Add recordings ? By exporting positions of all the
// particles each frame ? That would make around 8000 postions
// every 1/60 second that is to say 60*8000=480,000 positions
// per second, let's assume a position is 8 bytes (from Vec2),
// then one second of simulation is 8*480,000=3,840,000 bytes
// this is around 4MB. 1min of simulation is 60*4=240MB.
// This seems possible, although not for long recordings.

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
const DEFAULT_WIDTH: f32 = DEFAULT_HEIGHT * 1.618;
/// Default world height the simulation.
const DEFAULT_HEIGHT: f32 = 600.;

/// Minimum world width the simulation.
const MIN_WORLD_W: f32 = 100.;
/// Maximum world width the simulation.
const MAX_WORLD_W: f32 = 1000.;
/// Minimum world height the simulation.
const MIN_WORLD_H: f32 = MIN_WORLD_W;
/// Maximum world height the simulation.
const MAX_WORLD_H: f32 = MAX_WORLD_W;

const MIN_COUNT: usize = 50;
/// Total maximum number of particles in the simulation.
const MAX_TOTAL_COUNT: usize = 10000;
const DEFAULT_MAX_TOTAL_COUNT: usize = 8000;

const MAX_POWER: f32 = 50.;
const MIN_POWER: f32 = -MAX_POWER;

const MIN_RADIUS: f32 = 10.;
const MAX_RADIUS: f32 = 100.;

const DEFAULT_SPEED_FACTOR: f32 = 40.;
const MIN_SPEED_FACTOR: f32 = 2.;
const MAX_SPEED_FACTOR: f32 = 80.;

const DEFAULT_DAMPING_FACTOR: f32 = 0.5;

const DEFAULT_ZOOM: f32 = 1.;
const MIN_ZOOM: f32 = 1.;
const MAX_ZOOM: f32 = 10.;
const ZOOM_FACTOR: f32 = 0.02;

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

    eframe::run_native("Smarticles", options, Box::new(|_| Box::new(smarticles)));
}

struct Smarticles {
    world_w: f32,
    world_h: f32,

    play: bool,
    type_count: usize,
    max_total_count: usize,
    speed_factor: f32,
    seed: String,

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
        let words = words
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
            world_w: DEFAULT_WIDTH,
            world_h: DEFAULT_HEIGHT,

            play: false,
            type_count: MAX_TYPES,
            max_total_count: DEFAULT_MAX_TOTAL_COUNT,
            speed_factor: DEFAULT_SPEED_FACTOR,
            seed: String::new(),

            params: types.map(|(name, color)| Params {
                name: name.to_string(),
                heading: "Type ".to_string() + &name.to_string(),
                color,
                count: 0,
                power: [0.; MAX_TYPES],
                radius: [MIN_RADIUS; MAX_TYPES],
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
        self.world_w = DEFAULT_WIDTH;
        self.world_h = DEFAULT_HEIGHT;
        self.max_total_count = DEFAULT_MAX_TOTAL_COUNT;
        self.speed_factor = DEFAULT_SPEED_FACTOR;
        self.view = View::DEFAULT;
        for p in &mut self.params {
            p.count = 0;
            p.radius.iter_mut().for_each(|r| *r = 0.);
            p.power.iter_mut().for_each(|p| *p = 0.);
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
                    pos: Vec2::new(
                        self.world_w * rand.sample::<f32, _>(OpenClosed01),
                        self.world_h * rand.sample::<f32, _>(OpenClosed01),
                    ),
                    vel: Vec2::ZERO,
                });
            }
        }
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
                MIN_COUNT as f32,
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

    fn simulate(&mut self, dt: f32) {
        let dots_clone = self.dots.to_owned();
        self.dots
            .par_iter_mut()
            .enumerate()
            .for_each(|(i, dots_i)| {
                for (j, dot) in dots_clone.iter().enumerate().take(self.type_count) {
                    interaction(
                        dots_i,
                        dot,
                        dt,
                        self.speed_factor,
                        self.params[i].power[j],
                        self.params[i].radius[j],
                        self.world_w,
                        self.world_h,
                    );
                }
            });
    }

    fn export(&self) -> String {
        let mut bytes: Vec<u8> = Vec::new();
        bytes.write_u16::<LE>(self.world_w as u16).unwrap();
        bytes.write_u16::<LE>(self.world_h as u16).unwrap();
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
        self.world_w = bytes.read_u16::<LE>().unwrap_or(DEFAULT_HEIGHT as u16) as f32;
        self.world_h = bytes.read_u16::<LE>().unwrap_or(DEFAULT_WIDTH as u16) as f32;
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

fn interaction(
    group1: &mut [Dot],
    group2: &[Dot],
    dt: f32,
    speed_factor: f32,
    g: f32,
    radius: f32,
    world_w: f32,
    world_h: f32,
) {
    let g = g / -100.;
    group1.par_iter_mut().for_each(|p1| {
        let mut f = Vec2::ZERO;
        for p2 in group2 {
            let d = p1.pos - p2.pos;
            let r = d.length();
            if r < radius && r > 0. {
                f += d / r;
            }
        }

        p1.vel = (p1.vel + f * g) * DEFAULT_DAMPING_FACTOR;
        p1.pos += p1.vel * speed_factor * dt;

        if p1.pos.x < 0. {
            p1.pos.x = 0.;
            p1.vel.x = 10.;
        } else if p1.pos.x >= world_w {
            p1.pos.x = world_w;
            p1.vel.x = -10.;
        }
        if p1.pos.y < 0. {
            p1.pos.y = 0.;
            p1.vel.y = 10.;
        } else if p1.pos.y >= world_h {
            p1.pos.y = world_h;
            p1.vel.y = -10.;
        }

        // if (p1.pos.x < 10. && p1.vel.x < 0.) || (p1.pos.x > world_w - 10. && p1.vel.x > 0.) {
        //     p1.vel.x *= -8.;
        // }
        // if (p1.pos.y < 10. && p1.vel.y < 0.) || (p1.pos.y > world_h - 10. && p1.vel.y > 0.) {
        //     p1.vel.y *= -8.;
        // }

        // alternative: wrap
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
    });
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
                    self.seed = format!("{}_{}", self.words[w1], self.words[w2]);

                    self.apply_seed();
                    self.type_count = MAX_TYPES;
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
                    self.spawn();
                    self.stop();
                }
            });

            ui.horizontal(|ui| {
                ui.label("World Width:");
                let world_w = ui.add(Slider::new(&mut self.world_w, MIN_WORLD_W..=MAX_WORLD_W));
                let reset = ui.button("Reset");
                if reset.clicked() {
                    self.world_w = DEFAULT_WIDTH;
                }
                if world_w.changed() || reset.clicked() {
                    self.seed = self.export();
                    self.spawn();
                }
            });
            ui.horizontal(|ui| {
                ui.label("World Height:");
                let world_h = ui.add(Slider::new(&mut self.world_h, MIN_WORLD_H..=MAX_WORLD_H));
                let reset = ui.button("Reset");
                if reset.clicked() {
                    self.world_h = DEFAULT_HEIGHT;
                }
                if world_h.changed() || reset.clicked() {
                    self.seed = self.export();
                    self.spawn();
                }
            });
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
                    MIN_COUNT..=MAX_TOTAL_COUNT,
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
                        (resp.rect.width() - self.world_w * self.view.zoom) / 2.,
                        (resp.rect.height() - self.world_h * self.view.zoom) / 2.,
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

                for i in 0..self.type_count {
                    let p = &self.params[i];
                    let col: Color32 = p.color.into();
                    for dot in &self.dots[i] {
                        let pos = min + dot.pos * self.view.zoom;
                        if pos.x >= resp.rect.min.x
                            && pos.x <= resp.rect.max.x
                            && pos.y >= resp.rect.min.y
                            && pos.y <= resp.rect.max.y
                        {
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
