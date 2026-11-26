//! Cancellation registry for desktop build requests. A cancellation can arrive before
//! its compile command, so remember early cancellations until registration.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use tokio::sync::watch;

#[derive(Default)]
pub struct BuildCancellation {
    inner: Mutex<Registry>,
}

#[derive(Default)]
struct Registry {
    active: BTreeMap<String, watch::Sender<bool>>,
    early: BTreeSet<String>,
}

impl BuildCancellation {
    pub fn register(&self, id: &str) -> watch::Receiver<bool> {
        let mut state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let (sender, receiver) = watch::channel(state.early.remove(id));
        state.active.insert(id.to_owned(), sender);
        receiver
    }

    pub fn cancel(&self, id: &str) {
        let mut state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(sender) = state.active.get(id) {
            let _ = sender.send(true);
        } else {
            // Bound memory if a caller cancels an unknown request.
            if state.early.len() >= 256 { state.early.pop_first(); }
            state.early.insert(id.to_owned());
        }
    }

    pub fn finish(&self, id: &str) {
        let mut state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        state.active.remove(id);
        state.early.remove(id);
    }
}

pub struct Registration<'a> { pub registry: &'a BuildCancellation, pub id: &'a str }
impl Drop for Registration<'_> {
    fn drop(&mut self) { self.registry.finish(self.id); }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancel_before_and_after_registration() {
        let registry = BuildCancellation::default();
        registry.cancel("early");
        let early = registry.register("early");
        assert!(*early.borrow());
        registry.finish("early");
        let mut active = registry.register("active");
        assert!(!*active.borrow());
        registry.cancel("active");
        active.changed().await.unwrap();
        assert!(*active.borrow());
        registry.finish("active");
        assert!(!*registry.register("active").borrow());
    }
}
