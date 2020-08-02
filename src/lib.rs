//! A simple, cancelable timer implementation with no external dependencies.

#![deny(missing_docs)]

use std::sync::{Arc, Condvar, Mutex, TryLockError};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;

/// Errors that may be thrown by ThreadTimer::start()
#[derive(Debug, Eq, PartialEq)]
pub enum TimerStartError {
    /// The timer is already waiting to execute some other thunk
    AlreadyWaiting,
}

/// Errors that may be thrown by ThreadTimer::cancel()
#[derive(Debug, Eq, PartialEq)]
pub enum TimerCancelError {
    /// The timer is not currently waiting, so there is nothing to cancel
    NotWaiting,
}

/// Message sent to tell the timer thread to start waiting
struct StartWaitMessage {
    dur: Duration,
    f: Box<dyn FnOnce() + Send + 'static>,
}

/// A simple, cancelable timer that can run a thunk after waiting for an
/// arbitrary duration.
///
/// Waiting is accomplished by using a helper thread (the "wait thread") that
/// listens for incoming wait requests and then executes the requested thunk
/// after blocking for the requested duration. Because each ThreadTimer keeps
/// only one wait thread, each ThreadTimer may only be waiting for a single
/// thunk at a time.
///
/// ```
/// use std::sync::mpsc::{self, TryRecvError};
/// use std::thread;
/// use std::time::Duration;
/// use thread_timer::ThreadTimer;
///
/// let (sender, receiver) = mpsc::channel::<bool>();
/// let timer = ThreadTimer::new();
///
/// timer.start(Duration::from_millis(50), move || { sender.send(true).unwrap() }).unwrap();
///
/// thread::sleep(Duration::from_millis(60));
/// assert_eq!(receiver.try_recv(), Ok(true));
/// ```
///
/// If a ThreadTimer is currently waiting to execute a thunk, the wait can be
/// canceled, in which case the thunk will not be run.
///
/// ```
/// use std::sync::mpsc::{self, TryRecvError};
/// use std::thread;
/// use std::time::Duration;
/// use thread_timer::ThreadTimer;
///
/// let (sender, receiver) = mpsc::channel::<bool>();
/// let timer = ThreadTimer::new();
///
/// timer.start(Duration::from_millis(50), move || { sender.send(true).unwrap() }).unwrap();
///
/// thread::sleep(Duration::from_millis(10));
/// timer.cancel().unwrap();
///
/// thread::sleep(Duration::from_millis(60));
/// assert_eq!(receiver.try_recv(), Err(TryRecvError::Disconnected));
/// ```
#[derive(Clone)]
pub struct ThreadTimer {
    // Allow only one operation at a time so that we don't need to worry about interleaving
    op_lock: Arc<Mutex<()>>,
    // Record whether or not the timer is currently waiting
    is_waiting: Arc<Mutex<bool>>,
    // Used to wait for a cancelation signal and avoid spurious wakeups while waiting
    is_canceled: Arc<(Mutex<bool>, Condvar)>,
    // Used to tell the timer thread to start waiting
    sender: Sender<StartWaitMessage>,
}

impl ThreadTimer {
    /// Creates and returns a new ThreadTimer. Spawns a new thread to do the
    /// waiting (the "wait thread").
    ///
    /// ```
    /// use thread_timer::ThreadTimer;
    /// let timer = ThreadTimer::new();
    /// ```
    pub fn new() -> Self {
	let (sender, receiver) = mpsc::channel::<StartWaitMessage>();
	let is_waiting = Arc::new(Mutex::new(false));
	let thread_is_waiting = is_waiting.clone();
	let is_canceled = Arc::new((Mutex::new(false), Condvar::new()));
	let thread_is_canceled = is_canceled.clone();

	thread::spawn(move || {
	    loop {
		match receiver.recv() {
		    Ok(msg) => {
			let (cancel_lock, cancel_condvar) = &*thread_is_canceled;
			let (mut cancel_guard, cancel_res) = cancel_condvar.wait_timeout_while(
			    cancel_lock.lock().unwrap(),
			    msg.dur,
			    |&mut is_canceled| !is_canceled,
			).unwrap();
			if cancel_res.timed_out() {
			    // Only run the thunk if the wait completed (i.e. it
			    // was not canceled)
			    (msg.f)();
			}
			// Always clear the cancel guard (even if the wait
			// completed and we executed the thunk)
			*cancel_guard = false;
			*thread_is_waiting.lock().unwrap() = false;
		    },
		    // If the sender has disconnected, break out of the loop
		    Err(_) => break,
		}
	    }
	});

	ThreadTimer {
	    op_lock: Arc::new(Mutex::new(())),
	    is_waiting,
	    is_canceled,
	    sender,
	}
    }

    /// Start waiting. Wait for `dur` to elapse then execute `f`. Will not
    /// execute `f` if the timer is canceled before `dur` elapses.
    /// Returns [TimerStartError](enum.TimerStartError.html)::AlreadyWaiting if
    /// the timer is already waiting to execute a thunk.
    /// ```
    /// use std::sync::mpsc::{self, TryRecvError};
    /// use std::thread;
    /// use std::time::Duration;
    /// use thread_timer::ThreadTimer;
    ///
    /// let (sender, receiver) = mpsc::channel::<bool>();
    /// let timer = ThreadTimer::new();
    ///
    /// timer.start(Duration::from_millis(50), move || { sender.send(true).unwrap() }).unwrap();
    /// assert_eq!(
    ///   receiver.try_recv(),
    ///   Err(TryRecvError::Empty),
    ///   "Received response before wait elapsed!",
    /// );
    ///
    /// thread::sleep(Duration::from_millis(60));
    /// assert_eq!(
    ///   receiver.try_recv(),
    ///   Ok(true),
    ///   "Did not receive response after wait elapsed!",
    /// );
    /// ```
    pub fn start<F>(&self, dur: Duration, f: F) -> Result<(), TimerStartError>
    where F: FnOnce() + Send + 'static {
	let _guard = self.op_lock.lock().unwrap();
	let mut is_waiting = self.is_waiting.lock().unwrap();
	if *is_waiting {
	    return Err(TimerStartError::AlreadyWaiting);
	}
	*is_waiting = true;
	let msg = StartWaitMessage {
	    dur,
	    f: Box::new(f),
	};
	self.sender.send(msg).unwrap();
	Ok(())
    }

    /// Cancel the current timer (the thunk will not be executed and the timer
    /// will be able to start waiting to execute another thunk).
    /// Returns [TimerCancelError](enum.TimerCancelError.html)::NotWaiting if
    /// the timer is not currently waiting.
    /// ```
    /// use std::sync::mpsc::{self, TryRecvError};
    /// use std::thread;
    /// use std::time::Duration;
    /// use thread_timer::ThreadTimer;
    ///
    /// let (sender, receiver) = mpsc::channel::<bool>();
    /// let timer = ThreadTimer::new();
    ///
    /// timer.start(Duration::from_millis(50), move || { sender.send(true).unwrap() }).unwrap();
    ///
    /// // Make sure the wait has actually started before we cancel
    /// thread::sleep(Duration::from_millis(10));
    /// timer.cancel().unwrap();
    ///
    /// thread::sleep(Duration::from_millis(60));
    /// assert_eq!(
    ///   receiver.try_recv(),
    ///   // When the wait is canceled, the thunk and its Sender will be dropped
    ///   Err(TryRecvError::Disconnected),
    ///   "Received response from canceled wait!",
    /// );
    /// ```
    pub fn cancel(&self) -> Result<(), TimerCancelError> {
	let _guard = self.op_lock.lock().unwrap();
	let is_waiting = self.is_waiting.lock().unwrap();
	if !*is_waiting {
	    return Err(TimerCancelError::NotWaiting);
	}

	let (cancel_lock, cancel_condvar) = &*self.is_canceled;

	// This must be try_lock() not lock() in order to avoid a deadlock with
	// the wait thread. At this point the client thread holds the wait
	// lock. If the wait thread holds the cancel lock (if it has finished
	// waiting and is running the task), then we will not be able to get the
	// cancel lock here and the wait thread will not be able to get the wait
	// lock to indicate that it has finished waiting.
	match cancel_lock.try_lock() {
	    // We were able to acquire the cancel lock, so cancel the wait
	    Ok(mut cancel_guard) => {
		*cancel_guard = true;
		cancel_condvar.notify_one();
		// We do not clear the cancel or waiting flags here since those
		// are cleared in the wait thread
		Ok(())
	    },
	    // The wait thread holds the cancel lock, so return an error
	    // indicating that we were unable to cancel
	    Err(TryLockError::WouldBlock) => Err(TimerCancelError::NotWaiting),
	    Err(TryLockError::Poisoned(_)) => panic!("Cancel lock was poisoned"),
	}
    }
}

impl Default for ThreadTimer {
    fn default() -> Self {
	Self::new()
    }
}
