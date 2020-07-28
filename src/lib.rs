use std::sync::{Arc, Condvar, Mutex, TryLockError};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;

#[derive(Debug, Eq, PartialEq)]
pub enum TimerStartError {
    AlreadyWaiting,
}

#[derive(Debug, Eq, PartialEq)]
pub enum TimerCancelError {
    NotWaiting,
}

pub trait Timer {
    fn new() -> Self;

    fn start<F>(&self, dur: Duration, f: F) -> Result<(), TimerStartError>
    where F: FnOnce() + Send + 'static;

    fn cancel(&self) -> Result<(), TimerCancelError>;
}

/// Message sent to tell the timer thread to start waiting
struct StartWaitMessage {
    dur: Duration,
    f: Box<dyn FnOnce() + Send + 'static>,
}

/// Implementation of Timer that maintains a helper thread to wait for the requested duration.
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

impl Timer for ThreadTimer {
    fn new() -> Self {
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

    fn start<F>(&self, dur: Duration, f: F) -> Result<(), TimerStartError>
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

    fn cancel(&self) -> Result<(), TimerCancelError> {
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
