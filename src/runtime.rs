use crate::actor::Actor;

pub struct Runtime {
    actors: Vec<Box<dyn Actor>>,
}
impl Runtime {
    pub fn new() -> Self {
        Self { actors: Vec::new() }
    }

    pub fn with<A: Actor>(mut self, actor: A) -> Self {
        self.actors.push(Box::new(actor));
        self
    }

    pub fn start(self) {
        for actor in self.actors {
            tokio::spawn(actor.run());
        }
    }
}
