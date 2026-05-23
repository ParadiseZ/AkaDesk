#[derive(Debug, Default)]
pub struct HostSupervisor {
    pub ready: bool,
}

impl HostSupervisor {
    pub fn start(&mut self) {
        self.ready = true;
    }
}
