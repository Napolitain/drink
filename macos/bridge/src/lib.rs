//! Native adapter entry points. CXX owns the cross-language calling convention.
struct Scheduler(drink_core::Scheduler);

fn new_scheduler(now: u64) -> Box<Scheduler> {
    Box::new(Scheduler(drink_core::Scheduler::new(now)))
}

impl Scheduler {
    fn update(&mut self, now: u64, present: bool, blocked: bool) {
        self.0.update(now, present, blocked);
    }
    fn request_break(&mut self, now: u64, present: bool, blocked: bool) {
        self.0.request_break(now, present, blocked);
    }
    fn pause(&mut self, now: u64) {
        self.0.pause(now);
    }
    fn restart(&mut self, now: u64) {
        self.0.restart(now);
    }
    fn deadline(&self) -> u64 {
        self.0.deadline()
    }
    fn visible(&self) -> bool {
        self.0.visible()
    }
    fn pending(&self) -> bool {
        self.0.pending()
    }
    fn paused(&self) -> bool {
        self.0.paused()
    }
}

#[cxx::bridge(namespace = "drink")]
mod ffi {
    extern "Rust" {
        type Scheduler;
        fn new_scheduler(now: u64) -> Box<Scheduler>;
        fn update(self: &mut Scheduler, now: u64, present: bool, blocked: bool);
        fn request_break(self: &mut Scheduler, now: u64, present: bool, blocked: bool);
        fn pause(self: &mut Scheduler, now: u64);
        fn restart(self: &mut Scheduler, now: u64);
        fn deadline(self: &Scheduler) -> u64;
        fn visible(self: &Scheduler) -> bool;
        fn pending(self: &Scheduler) -> bool;
        fn paused(self: &Scheduler) -> bool;
    }
}
