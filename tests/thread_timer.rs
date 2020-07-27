use std::sync::mpsc::{self, TryRecvError};
use std::thread;
use std::time::Duration;

use timer::*;

#[test]
fn executes_thunk_after_wait() {
    let timer = ThreadTimer::new();
    let (sender, receiver) = mpsc::channel();
    let f = move || {
	sender.send(true).unwrap();
    };
    assert_eq!(timer.start(Duration::from_millis(50), f), Ok(()));
    assert_eq!(receiver.try_recv(), Err(TryRecvError::Empty));
    thread::sleep(Duration::from_millis(60));
    assert_eq!(receiver.try_recv(), Ok(true));
}
