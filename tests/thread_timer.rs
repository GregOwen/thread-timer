use std::sync::mpsc::{self, Sender, TryRecvError};
use std::thread;
use std::time::Duration;

use timer::*;

fn get_test_thunk(sender: &Sender<bool>) -> impl FnOnce() + Send + 'static {
    let sender_clone = sender.clone();
    move || {
	sender_clone.send(true).unwrap();
    }
}

#[test]
fn executes_thunk_after_wait() {
    let timer = ThreadTimer::new();
    let (sender, receiver) = mpsc::channel();
    let f = get_test_thunk(&sender);
    assert_eq!(timer.start(Duration::from_millis(50), f), Ok(()));
    assert_eq!(receiver.try_recv(), Err(TryRecvError::Empty));
    thread::sleep(Duration::from_millis(60));
    assert_eq!(receiver.try_recv(), Ok(true));
}

#[test]
fn can_be_reused() {
    let timer = ThreadTimer::new();
    let (sender, receiver) = mpsc::channel();
    let f1 = get_test_thunk(&sender);
    assert_eq!(timer.start(Duration::from_millis(50), f1), Ok(()));
    thread::sleep(Duration::from_millis(60));
    assert_eq!(receiver.try_recv(), Ok(true));

    let f2 = get_test_thunk(&sender);
    assert_eq!(timer.start(Duration::from_millis(50), f2), Ok(()));
    thread::sleep(Duration::from_millis(60));
    assert_eq!(receiver.try_recv(), Ok(true));
}
