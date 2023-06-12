use base64::Engine;
use byteorder::{ReadBytesExt, WriteBytesExt, LE};
use eframe::{epaint::Color32, App, Frame, NativeOptions};
use egui::{CentralPanel, Context, Rgba, Sense, SidePanel, Slider, Vec2};
use rand::{distributions::OpenClosed01, rngs::SmallRng, Rng, SeedableRng};
use rayon::prelude::*;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    time::{Duration, Instant},
};

const INIT_SIZE: f32 = 800.0;
const MIN_COUNT: usize = 0;
const MAX_COUNT: usize = 1000;
const MIN_POWER: f32 = -100.0;
const MAX_POWER: f32 = 100.0;
const MIN_RADIUS: f32 = 0.0;
const MAX_RADIUS: f32 = 500.0;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    eframe::run_native(
        "Smarticles",
        NativeOptions {
            initial_window_size: Some(Vec2::new(1600.0, 900.0)),
            //fullscreen: true,
            ..Default::default()
        },
        Box::new(|_cc| {
            Box::new(Smarticles::new(
                INIT_SIZE,
                INIT_SIZE,
                [
                    ("α", Rgba::from_rgb(1.0, 0.0, 0.0)),
                    ("β", Rgba::from_rgb(0.0, 1.0, 0.0)),
                    ("γ", Rgba::from_rgb(1.0, 1.0, 1.0)),
                    ("δ", Rgba::from_rgb(0.0, 0.0, 1.0)),
                ],
            ))
        }),
    )?;

    Ok(())
}

struct Smarticles<const N: usize> {
    world_w: f32,
    world_h: f32,
    params: [Params<N>; N],
    dots: [Vec<Dot>; N],
    play: bool,
    prev_time: Instant,
    seed: String,
    words: Vec<String>,
}

struct Params<const N: usize> {
    name: String,
    heading: String,
    color: Rgba,
    count: usize,
    power: [f32; N],
    radius: [f32; N],
}

#[derive(Clone)]
struct Dot {
    pos: Vec2,
    vel: Vec2,
}

impl<const N: usize> Smarticles<N> {
    fn new<S>(world_w: f32, world_h: f32, colors: [(S, Rgba); N]) -> Self
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
            world_w,
            world_h,
            params: colors.map(|(name, color)| Params {
                name: name.to_string(),
                heading: "Type ".to_string() + &name.to_string(),
                color,
                count: 0,
                power: [0.0; N],
                radius: [MIN_RADIUS; N],
            }),
            dots: std::array::from_fn(|_| Vec::new()),
            play: false,
            prev_time: Instant::now(),
            seed: String::new(),
            words,
        }
    }

    fn play(&mut self) {
        self.play = true;
    }

    fn stop(&mut self) {
        self.play = false;
    }

    fn restart(&mut self) {
        self.world_w = INIT_SIZE;
        self.world_h = INIT_SIZE;
        for p in &mut self.params {
            p.count = 0;
            p.radius.iter_mut().for_each(|r| *r = 0.0);
            p.power.iter_mut().for_each(|p| *p = 0.0);
        }
    }

    fn clear(&mut self) {
        for i in 0..N {
            self.dots[i].clear();
        }
    }

    fn spawn(&mut self) {
        self.clear();

        let mut rand = SmallRng::from_entropy();

        for i in 0..N {
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
                self.import(
                    base64::prelude::BASE64_STANDARD
                        .encode(&self.seed[1..])
                        .as_bytes(),
                );
                return;
            }
            let mut hasher = DefaultHasher::new();
            self.seed.hash(&mut hasher);
            SmallRng::seed_from_u64(hasher.finish())
        };
        let mut rand = |min: f32, max: f32| min + (max - min) * rand.sample::<f32, _>(OpenClosed01);

        const POW_F: f32 = 1.25;
        const RAD_F: f32 = 1.1;

        for i in 0..N {
            self.params[i].count = rand(MIN_COUNT as f32, MAX_COUNT as f32) as usize;
            for j in 0..N {
                let pow = rand(MIN_POWER, MAX_POWER);
                self.params[i].power[j] = if pow >= 0.0 {
                    pow.powf(1.0 / POW_F)
                } else {
                    -pow.abs().powf(1.0 / POW_F)
                };
                //self.params[i].power[j] = rand(MIN_POWER, MAX_POWER);
                self.params[i].radius[j] = rand(MIN_RADIUS, MAX_RADIUS).powf(1.0 / RAD_F);
            }
        }
    }

    fn simulate(&mut self) {
        let mut dots: [Vec<Dot>; N] = std::array::from_fn(|i| self.dots[i].clone());
        dots.par_iter_mut().enumerate().for_each(|(i, dots_i)| {
            for j in 0..N {
                interaction(
                    dots_i,
                    &self.dots[j],
                    self.params[i].power[j],
                    self.params[i].radius[j],
                    self.world_w,
                    self.world_h,
                );
            }
        });
        self.dots = dots;
    }

    fn export(&self) -> String {
        let mut bytes: Vec<u8> = Vec::new();
        bytes.write_u16::<LE>(self.world_w as u16).unwrap();
        bytes.write_u16::<LE>(self.world_h as u16).unwrap();
        for p in &self.params {
            bytes.write_u8((p.color.r() * 255.0) as u8).unwrap();
            bytes.write_u8((p.color.g() * 255.0) as u8).unwrap();
            bytes.write_u8((p.color.b() * 255.0) as u8).unwrap();
            bytes.write_u16::<LE>(p.count as u16).unwrap();
            for &p in &p.power {
                bytes.write_i8(p as i8).unwrap();
            }
            for &r in &p.radius {
                bytes.write_u16::<LE>(r as u16).unwrap();
            }
        }
        format!("@{}", base64::prelude::BASE64_STANDARD.encode(bytes))
    }

    fn import(&mut self, mut bytes: &[u8]) {
        self.world_w = bytes.read_u16::<LE>().unwrap_or(1000) as f32;
        self.world_h = bytes.read_u16::<LE>().unwrap_or(1000) as f32;
        for p in &mut self.params {
            let r = (bytes.read_u8().unwrap_or((p.color.r() * 255.0) as u8) as f32) / 255.0;
            let g = (bytes.read_u8().unwrap_or((p.color.g() * 255.0) as u8) as f32) / 255.0;
            let b = (bytes.read_u8().unwrap_or((p.color.b() * 255.0) as u8) as f32) / 255.0;
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
    g: f32,
    radius: f32,
    world_w: f32,
    world_h: f32,
) {
    let g = g / -100.0;
    group1.par_iter_mut().for_each(|p1| {
        let mut f = Vec2::ZERO;
        for p2 in group2 {
            let d = p1.pos - p2.pos;
            let r = d.length();
            if r < radius && r > 0.0 {
                f += d / r;
            }
        }

        p1.vel = (p1.vel + f * g) * 0.5;
        p1.pos += p1.vel;

        if (p1.pos.x < 10.0 && p1.vel.x < 0.0) || (p1.pos.x > world_w - 10.0 && p1.vel.x > 0.0) {
            p1.vel.x *= -1.0;
        }
        if (p1.pos.y < 10.0 && p1.vel.y < 0.0) || (p1.pos.y > world_h - 10.0 && p1.vel.y > 0.0) {
            p1.vel.y *= -1.0;
        }

        // alternative: wrap
        /*if p1.pos.x < 0.0 {
            p1.pos.x += world_w;
        } else if p1.pos.x >= world_w {
            p1.pos.x -= world_w;
        }
        if p1.pos.y < 0.0 {
            p1.pos.y += world_h;
        } else if p1.pos.y >= world_h {
            p1.pos.y -= world_h;
        }*/
    });
}

impl<const N: usize> App for Smarticles<N> {
    fn update(&mut self, ctx: &Context, frame: &mut Frame) {
        if self.play {
            let time = Instant::now();
            let delta = time - self.prev_time;
            if delta > Duration::from_secs_f32(1.0 / 60.0) {
                self.prev_time = time;
                self.simulate();
            }
            ctx.request_repaint();
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
                    self.spawn();
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
                if ui
                    .add(Slider::new(&mut self.world_w, 100.0..=1000.0))
                    .changed()
                {
                    self.seed = self.export();
                    self.spawn();
                }
            });
            ui.horizontal(|ui| {
                ui.label("World Height:");
                if ui
                    .add(Slider::new(&mut self.world_h, 100.0..=1000.0))
                    .changed()
                {
                    self.seed = self.export();
                    self.spawn();
                }
            });

            for i in 0..N {
                ui.add_space(10.0);
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
                            MIN_COUNT..=MAX_COUNT,
                        ))
                        .changed()
                    {
                        self.seed = self.export();
                    }
                });

                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        for j in 0..N {
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
                        for j in 0..N {
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

        CentralPanel::default().show(ctx, |ui| {
            let (resp, paint) =
                ui.allocate_painter(ui.available_size_before_wrap(), Sense::hover());

            let min = resp.rect.min
                + Vec2::new(
                    (resp.rect.width() - self.world_w) / 2.0,
                    (resp.rect.height() - self.world_h) / 2.0,
                );

            for i in 0..N {
                let p = &self.params[i];
                let col: Color32 = p.color.into();
                for dot in &self.dots[i] {
                    paint.circle_filled(min + dot.pos, 2.0, col);
                }
            }
        });
    }
}
