mod app;
mod consts;
mod events;
mod mat;
mod simulation;
mod simulation_manager;

use std::{sync::mpsc::channel, thread, time::Duration};

use consts::CLASS_COUNT;
use eframe::{
    egui::ViewportBuilder,
    epaint::{Color32, Hsva},
    NativeOptions,
};
use events::{Event, Senders};

use crate::{app::SmarticlesApp, simulation_manager::SimulationManager};

fn main() {
    let options = NativeOptions {
        viewport: ViewportBuilder::default().with_fullscreen(true),
        ..Default::default()
    };

    let (ui_sender, ui_receiver) = channel::<Event>();
    let (sim_sender, sim_receiver) = channel::<Event>();

    let senders = Senders::new(ui_sender, sim_sender);

    eframe::run_native(
        "Smarticles",
        options,
        Box::new(|cc| {
            let frame = cc.egui_ctx.clone();

            let senders_clone = senders.clone();
            let simulation_handle = thread::spawn(move || {
                let mut sim_backend = SimulationManager::new(senders_clone, sim_receiver);

                thread::sleep(Duration::from_millis(500));

                loop {
                    if !sim_backend.update() {
                        break;
                    };
                    frame.request_repaint();
                }
            });

            Ok(Box::new(SmarticlesApp::new(
                ["α", "β", "γ", "δ", "ε", "ζ", "η", "θ"]
                    .iter()
                    .take(CLASS_COUNT)
                    .copied()
                    .enumerate()
                    .map(|(i, class_name)| {
                        let [r, g, b] = Hsva::new(
                            // tinkering to make purple appear first. I like purple.
                            0.2 + 0.8 * (((CLASS_COUNT - i - 1) % CLASS_COUNT) as f32)
                                / (CLASS_COUNT as f32),
                            0.9,
                            0.9,
                            1.,
                        )
                        .to_srgb();
                        (class_name, Color32::from_rgb(r, g, b))
                    })
                    .collect::<Vec<(&str, Color32)>>()
                    .try_into()
                    .unwrap(),
                senders,
                ui_receiver,
                Some(simulation_handle),
            )))
        }),
    )
    .unwrap();
}
