# thread_timer

A simple, cancelable timer implementation with no external dependencies.

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/GregOwen/thread-timer)

## ThreadTimer

The main interface of this crate is the struct `ThreadTimer`, which is a simple,
cancelable timer that can run a thunk after waiting for an arbitrary duration.

Waiting is accomplished by using a helper thread (the "wait thread") that
listens for incoming wait requests and then executes the requested thunk after
blocking for the requested duration. Because each `ThreadTimer` keeps only one
wait thread, each `ThreadTimer` may only be waiting for a single thunk at a
time.

 ```
 use std::sync::mpsc::{self, TryRecvError};
 use std::thread;
 use std::time::Duration;
 use thread_timer::ThreadTimer;

 let (sender, receiver) = mpsc::channel::<bool>();
 let timer = ThreadTimer::new();

 timer.start(Duration::from_millis(50), move || { sender.send(true).unwrap() }).unwrap();

 thread::sleep(Duration::from_millis(60));
 assert_eq!(receiver.try_recv(), Ok(true));
 ```

If a ThreadTimer is currently waiting to execute a thunk, the wait can be
canceled, in which case the thunk will not be run.

 ```
 use std::sync::mpsc::{self, TryRecvError};
 use std::thread;
 use std::time::Duration;
 use thread_timer::ThreadTimer;

 let (sender, receiver) = mpsc::channel::<bool>();
 let timer = ThreadTimer::new();

 timer.start(Duration::from_millis(50), move || { sender.send(true).unwrap() }).unwrap();

 thread::sleep(Duration::from_millis(10));
 timer.cancel().unwrap();

 thread::sleep(Duration::from_millis(60));
 assert_eq!(receiver.try_recv(), Err(TryRecvError::Disconnected));
 ```
