use std::{sync::mpsc::Receiver, thread::sleep, time::Instant};

use crate::{
    consts::{PAUSED_UPDATE_INTERVAL, UPDATE_INTERVAL},
    events::{Event, StateUpdate},
    simulation::Simulation,
    Senders,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SimulationState {
    Paused,
    Running,
}

pub struct SimulationManager {
    simulation_state: SimulationState,

    simulation: Simulation,

    senders: Senders,
    receiver: Receiver<Event>,
}

impl SimulationManager {
    pub fn new(senders: Senders, receiver: Receiver<Event>) -> Self {
        Self {
            simulation_state: SimulationState::Paused,

            simulation: Simulation::default(),

            senders,
            receiver,
        }
    }

    pub fn update(&mut self) -> bool {
        let events = self.receiver.try_iter().collect::<Vec<_>>();
        for event in events {
            match event {
                Event::StateUpdate(StateUpdate {
                    power_matrix,
                    particle_counts,
                    ..
                }) => {
                    if let Some(power_matrix) = power_matrix {
                        self.simulation.power_matrix = power_matrix;
                    }
                    if let Some(particle_counts) = particle_counts {
                        self.simulation.particle_counts = particle_counts;
                    }
                }

                Event::SpawnParticles => {
                    self.simulation.spawn();
                    self.senders.send_app(Event::StateUpdate(
                        StateUpdate::new().particle_positions(&self.simulation.particle_positions),
                    ))
                }

                Event::SimulationStart => self.simulation_state = SimulationState::Running,
                Event::SimulationPause => self.simulation_state = SimulationState::Paused,

                Event::EnableClass(c) => {
                    self.simulation.enabled_classes[c] = true;
                    self.simulation.spawn();
                }
                Event::DisableClass(c) => {
                    self.simulation.enabled_classes[c] = false;
                }

                Event::Exit => return false,
            }
        }

        if self.simulation_state == SimulationState::Running {
            let start_time = Instant::now();
            self.simulation.move_particles();
            let elapsed = start_time.elapsed();

            self.senders.send_app(Event::StateUpdate(
                StateUpdate::new()
                    .particle_positions(&self.simulation.particle_positions)
                    .computation_duration(elapsed),
            ));

            #[cfg(feature = "cell_map_display")]
            self.senders
                .send_app(Event::StateUpdate(StateUpdate::new().cell_map(
                    self.simulation.cell_map.keys().copied().collect::<Vec<_>>(),
                )));

            if elapsed < UPDATE_INTERVAL {
                sleep(UPDATE_INTERVAL - elapsed);
            }
        } else {
            sleep(PAUSED_UPDATE_INTERVAL);
        }

        true
    }
}
