use std::{fmt::Display, sync::mpsc::Sender, time::Duration};

use eframe::egui::Vec2;

use crate::{consts::LOG, mat::Mat2D, CLASS_COUNT};

#[cfg(feature = "cell_map_display")]
use crate::simulation::Cell;

#[derive(Debug, Clone)]
pub enum Event {
    Exit,

    SpawnParticles,
    EnableClass(usize),
    DisableClass(usize),

    SimulationStart,
    SimulationPause,

    StateUpdate(StateUpdate),
}

impl Display for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Event::Exit => write!(f, "Exit"),
            Event::SpawnParticles => write!(f, "SpawnParticles"),
            Event::EnableClass(i) => write!(f, "EnsableClass({i})"),
            Event::DisableClass(i) => write!(f, "DisableClass({i})"),
            Event::SimulationStart => write!(f, "SimulationStart"),
            Event::SimulationPause => write!(f, "SimulationPause"),
            Event::StateUpdate(state_update) => write!(f, "{}", state_update),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Recipient {
    App,
    Sim,
}

#[derive(Debug, Clone)]
pub struct Senders {
    origin: Option<Recipient>,

    pub app_sender: Sender<Event>,
    pub sim_sender: Sender<Event>,
}

impl Senders {
    pub fn new(app_sender: Sender<Event>, sim_sender: Sender<Event>) -> Self {
        Senders {
            origin: None,

            app_sender,
            sim_sender,
        }
    }

    pub fn send_app(&self, event: Event) {
        self.send(Recipient::App, event);
    }

    pub fn send_sim(&self, event: Event) {
        self.send(Recipient::Sim, event);
    }

    fn send(&self, send_to: Recipient, event: Event) {
        if LOG {
            println!(
                "{}to {:?}: {}",
                self.origin
                    .clone()
                    .map_or(String::new(), |origin| format!("{:?} ", origin)),
                send_to,
                event
            );
        }
        match send_to {
            Recipient::App => self.app_sender.send(event).unwrap(),
            Recipient::Sim => self.sim_sender.send(event).unwrap(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StateUpdate {
    pub particle_positions: Option<Mat2D<Vec2>>,
    pub computation_time: Option<Duration>,

    pub power_matrix: Option<Mat2D<i8>>,
    pub particle_counts: Option<[usize; CLASS_COUNT]>,

    #[cfg(feature = "cell_map_display")]
    pub cell_map: Option<Vec<Cell>>,
}

impl StateUpdate {
    pub fn new() -> Self {
        StateUpdate {
            particle_positions: None,
            computation_time: None,
            power_matrix: None,
            particle_counts: None,
            #[cfg(feature = "cell_map_display")]
            cell_map: None,
        }
    }

    pub fn particle_positions(mut self, particle_positions: &Mat2D<Vec2>) -> StateUpdate {
        self.particle_positions = Some(particle_positions.clone());
        self
    }
    pub fn computation_duration(mut self, computation_duration: Duration) -> StateUpdate {
        self.computation_time = Some(computation_duration);
        self
    }
    pub fn power_matrix(mut self, power_matrix: &Mat2D<i8>) -> StateUpdate {
        self.power_matrix = Some(power_matrix.clone());
        self
    }
    pub fn particle_counts(mut self, particle_counts: &[usize; CLASS_COUNT]) -> StateUpdate {
        self.particle_counts = Some(*particle_counts);
        self
    }
    #[cfg(feature = "cell_map_display")]
    pub fn cell_map(mut self, cell_map: Vec<Cell>) -> StateUpdate {
        self.cell_map = Some(cell_map.clone());
        self
    }
}

impl Display for StateUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut fields = Vec::new();

        if self.particle_positions.is_some() {
            fields.push("particle_positions");
        }
        if self.computation_time.is_some() {
            fields.push("computation_duration");
        }
        if self.power_matrix.is_some() {
            fields.push("power_matrix");
        }
        if self.particle_counts.is_some() {
            fields.push("particle_counts");
        }
        #[cfg(feature = "cell_map_display")]
        if self.cell_map.is_some() {
            fields.push("cell_map");
        }

        write!(f, "StateUpdate {{ {} }}", fields.join(", "))
    }
}
