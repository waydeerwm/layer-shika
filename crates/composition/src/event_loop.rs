use std::cell::RefCell;
use std::os::unix::io::AsFd;
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

use layer_shika_adapters::errors::EventLoopError;
use layer_shika_adapters::platform::calloop::{
    EventSource, Generic, Interest, Mode, PostAction, RegistrationToken, TimeoutAction, Timer,
    channel,
};
use layer_shika_adapters::{AppState, WaylandSystemOps};

use crate::{Error, Result};

pub trait FromAppState<'a> {
    fn from_app_state(app_state: &'a mut AppState) -> Self;
}

/// Main event loop for the shell runtime
///
/// Manages the Wayland event loop and custom event sources.
/// Created internally by `Shell` and started via `Shell::run()`.
pub struct ShellEventLoop {
    inner: Rc<RefCell<dyn WaylandSystemOps>>,
}

impl ShellEventLoop {
    pub fn new(inner: Rc<RefCell<dyn WaylandSystemOps>>) -> Self {
        Self { inner }
    }

    pub fn run(&mut self) -> Result<()> {
        self.inner.borrow_mut().run()?;
        Ok(())
    }

    pub fn get_handle(&self) -> EventLoopHandle {
        EventLoopHandle::new(Rc::downgrade(&self.inner))
    }
}

/// Handle for registering custom event sources with the event loop
///
/// Supports timers, channels, file descriptors, and custom event sources.
pub struct EventLoopHandle {
    system: Weak<RefCell<dyn WaylandSystemOps>>,
}

impl EventLoopHandle {
    pub fn new(system: Weak<RefCell<dyn WaylandSystemOps>>) -> Self {
        Self { system }
    }

    /// Register a custom event source with the event loop
    ///
    /// Returns a registration token that can be used to remove the source later.
    pub fn insert_source<S, F, R>(&self, source: S, callback: F) -> Result<RegistrationToken>
    where
        S: EventSource<Ret = R> + 'static,
        F: FnMut(S::Event, &mut S::Metadata, &mut AppState) -> R + 'static,
    {
        let system = self.system.upgrade().ok_or(Error::SystemDropped)?;
        let loop_handle = system.borrow().event_loop_handle();

        loop_handle.insert_source(source, callback).map_err(|e| {
            Error::Adapter(
                EventLoopError::InsertSource {
                    message: format!("{e:?}"),
                }
                .into(),
            )
        })
    }

    /// Add a timer that fires after the specified duration
    ///
    /// Return `TimeoutAction::Drop` for one-shot, `ToDuration(d)` to repeat, or `ToInstant(i)` for next deadline.
    pub fn add_timer<F>(&self, duration: Duration, mut callback: F) -> Result<RegistrationToken>
    where
        F: FnMut(Instant, &mut AppState) -> TimeoutAction + 'static,
    {
        let timer = Timer::from_duration(duration);
        self.insert_source(timer, move |deadline, (), app_state| {
            callback(deadline, app_state)
        })
    }

    /// Add a timer that fires at a specific instant
    ///
    /// Callback receives the deadline and can return `TimeoutAction::ToInstant` to reschedule.
    pub fn add_timer_at<F>(&self, deadline: Instant, mut callback: F) -> Result<RegistrationToken>
    where
        F: FnMut(Instant, &mut AppState) -> TimeoutAction + 'static,
    {
        let timer = Timer::from_deadline(deadline);
        self.insert_source(timer, move |deadline, (), app_state| {
            callback(deadline, app_state)
        })
    }

    /// Add a channel for sending messages to the event loop from any thread
    ///
    /// The sender can be cloned and sent across threads. Messages are queued and processed on the main thread.
    pub fn add_channel<T, F>(
        &self,
        mut callback: F,
    ) -> Result<(RegistrationToken, channel::Sender<T>)>
    where
        T: 'static,
        F: FnMut(T, &mut AppState) + 'static,
    {
        let (sender, receiver) = channel::channel();
        let token = self.insert_source(receiver, move |event, (), app_state| {
            if let channel::Event::Msg(msg) = event {
                callback(msg, app_state);
            }
        })?;
        Ok((token, sender))
    }

    /// Add a file descriptor to be monitored for readability or writability
    ///
    /// Callback is invoked when the file descriptor becomes ready according to the interest.
    pub fn add_fd<F, T>(
        &self,
        fd: T,
        interest: Interest,
        mode: Mode,
        mut callback: F,
    ) -> Result<RegistrationToken>
    where
        T: AsFd + 'static,
        F: FnMut(&mut AppState) + 'static,
    {
        let generic = Generic::new(fd, interest, mode);
        self.insert_source(generic, move |_readiness, _fd, app_state| {
            callback(app_state);
            Ok(PostAction::Continue)
        })
    }
}
