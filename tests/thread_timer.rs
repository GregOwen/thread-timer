use std::sync::mpsc::{self, Sender, TryRecvError};
use std::thread;
use std::time::Duration;

use thread_timer::*;

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

#[test]
fn cannot_cancel_if_not_waiting() {
  let timer = ThreadTimer::new();
  assert_eq!(timer.cancel(), Err(TimerCancelError::NotWaiting));
}

#[test]
fn thunk_does_not_run_if_canceled() {
  let timer = ThreadTimer::new();
  let (sender, receiver) = mpsc::channel();
  let f = get_test_thunk(&sender);
  assert_eq!(timer.start(Duration::from_millis(50), f), Ok(()));
  thread::sleep(Duration::from_millis(10));
  assert_eq!(timer.cancel(), Ok(()));
  thread::sleep(Duration::from_millis(75));
  assert_eq!(receiver.try_recv(), Err(TryRecvError::Empty));
}

#[test]
fn does_not_deadlock_when_canceling_long_task() {
  // TODO(greg): put a timeout on this test so that we get a failure rather than a hang
  let timer = ThreadTimer::new();
  let (sender, _) = mpsc::channel();
  let f = move || {
    thread::sleep(Duration::from_secs(10));
    sender.send(true).unwrap();
  };
  assert_eq!(timer.start(Duration::from_millis(10), f), Ok(()));
  thread::sleep(Duration::from_millis(20));
  assert_eq!(timer.cancel(), Err(TimerCancelError::NotWaiting));
}

#[test]
fn can_start_cancel_start() {
  let timer = ThreadTimer::new();
  let (sender, receiver) = mpsc::channel();
  let f1 = get_test_thunk(&sender);
  assert_eq!(timer.start(Duration::from_millis(50), f1), Ok(()));
  thread::sleep(Duration::from_millis(10));
  assert_eq!(timer.cancel(), Ok(()));
  thread::sleep(Duration::from_millis(75));
  assert_eq!(receiver.try_recv(), Err(TryRecvError::Empty));
  let f2 = get_test_thunk(&sender);
  assert_eq!(timer.start(Duration::from_millis(50), f2), Ok(()));
  thread::sleep(Duration::from_millis(60));
  assert_eq!(receiver.try_recv(), Ok(true))
}
