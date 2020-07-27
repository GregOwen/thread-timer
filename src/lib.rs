use std::sync::{Arc, Mutex};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;

#[derive(Debug, Eq, PartialEq)]
pub enum TimerStartError {
    AlreadyWaiting,
}

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
    responder: Sender<bool>,
}

/// Implementation of Timer that maintains a helper thread to wait for the requested duration.
pub struct ThreadTimer {
    // Allow only one operation at a time so that we don't need to worry about interleaving
    op_lock: Arc<Mutex<()>>,
    // Record whether or not the timer is currently waiting
    is_waiting: Arc<Mutex<bool>>,
    sender: Sender<StartWaitMessage>,
}

impl Timer for ThreadTimer {
    fn new() -> Self {
	let (sender, receiver) = mpsc::channel::<StartWaitMessage>();

	thread::spawn(move || {
	    loop {
		match receiver.recv() {
		    Ok(msg) => {
			// Acknowledge that we've received the message and that we're starting to wait
			msg.responder.send(true).unwrap();
			thread::sleep(msg.dur);
			(msg.f)();
		    },
		    // If the sender has disconnected, break out of the loop
		    Err(_) => break,
		}
	    }
	});

	ThreadTimer {
	    op_lock: Arc::new(Mutex::new(())),
	    is_waiting: Arc::new(Mutex::new(false)),
	    sender,
	}
    }

    fn start<F>(&self, dur: Duration, f: F) -> Result<(), TimerStartError>
    where F: FnOnce() + Send + 'static {
	let _guard = self.op_lock.lock().unwrap();
	let mut is_waiting = *self.is_waiting.lock().unwrap();
	if is_waiting {
	    return Err(TimerStartError::AlreadyWaiting);
	}
	is_waiting = true;
	let (responder, receiver) = mpsc::channel();
	let msg = StartWaitMessage {
	    dur,
	    f: Box::new(f),
	    responder,
	};
	self.sender.send(msg).unwrap();
	let _ack = receiver.recv().unwrap();
	Ok(())
    }

    fn cancel(&self) -> Result<(), TimerCancelError> {
	Ok(())
    }
}
