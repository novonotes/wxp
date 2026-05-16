use std::{fmt::Display, thread};

use crate::{RunLoopSender, SystemThreadId, get_system_thread_id};

/// Lets a thread-affine (`!Send`) value be *moved* between threads while
/// enforcing, at runtime, that it is only accessed (or dropped with its value
/// still inside) on its original thread.
///
/// This is the escape hatch for putting native handles inside `Send` containers
/// (e.g. an `Arc<Mutex<Capsule<_>>>` shared with worker threads): the type
/// system sees `Send`, and the recorded `thread_id` is what actually keeps
/// access sound. Construct with [`new_with_sender`](Self::new_with_sender) when
/// the capsule may be dropped off-thread so the value can be shipped back and
/// dropped on the run loop thread instead of panicking.
pub struct Capsule<T>
where
    T: 'static,
{
    value: Option<T>,
    thread_id: SystemThreadId,
    sender: Option<RunLoopSender>,
}

#[derive(Debug)]
pub enum CapsuleError {
    CapsuleEmpty,
    WrongThread,
}

impl Display for CapsuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CapsuleError::CapsuleEmpty => write!(f, "capsule is empty"),
            CapsuleError::WrongThread => write!(f, "capsule retrieved on wrong thread"),
        }
    }
}

impl std::error::Error for CapsuleError {}

#[allow(dead_code)]
impl<T> Capsule<T>
where
    T: 'static,
{
    // Creates new capsule; If the value is not taken out of capsule, the
    // capsule must be dropped on same thread as it was created, otherwise
    // it will panic
    pub fn new(value: T) -> Self {
        Self {
            value: Some(value),
            thread_id: get_system_thread_id(),
            sender: None,
        }
    }

    // Creates new capsule, If the value is not taken out of capsule and the
    // capsule is dropped on different thread than where it was created, it will
    // be sent to the sender and dropped on the run loop thread
    pub fn new_with_sender(value: T, sender: RunLoopSender) -> Self {
        Self {
            value: Some(value),
            thread_id: get_system_thread_id(),
            sender: Some(sender),
        }
    }

    pub fn get_ref(&self) -> Result<&T, CapsuleError> {
        if self.thread_id == get_system_thread_id() {
            self.value.as_ref().ok_or(CapsuleError::CapsuleEmpty)
        } else {
            Err(CapsuleError::WrongThread)
        }
    }

    pub fn get_mut(&mut self) -> Result<&mut T, CapsuleError> {
        if self.thread_id == get_system_thread_id() {
            self.value.as_mut().ok_or(CapsuleError::CapsuleEmpty)
        } else {
            Err(CapsuleError::WrongThread)
        }
    }

    pub fn take(&mut self) -> Result<T, CapsuleError> {
        if self.thread_id == get_system_thread_id() {
            self.value.take().ok_or(CapsuleError::CapsuleEmpty)
        } else {
            Err(CapsuleError::WrongThread)
        }
    }
}

impl<T> Drop for Capsule<T> {
    fn drop(&mut self) {
        // Dropping `T` here would run its destructor on the wrong thread, which
        // is exactly what this type exists to prevent. If a sender is available,
        // ship the value back to its home thread to be dropped there; otherwise
        // there is no safe option but to panic (unless we are already
        // unwinding, where a second panic would abort the process).
        if self.value.is_some() && self.thread_id != get_system_thread_id() {
            if let Some(sender) = self.sender.as_ref() {
                let carry = Carry(self.value.take().unwrap());
                let thread_id = self.thread_id;
                sender.send(move || {
                    // make sure that sender sent us back to initial thread
                    if thread_id != get_system_thread_id() {
                        panic!("Capsule was created on different thread than sender target")
                    }
                    let _ = carry;
                });
            } else if !thread::panicking() {
                panic!("Capsule was dropped on wrong thread with data still in it!");
            }
        }
    }
}

impl<T: Clone> Clone for Capsule<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            thread_id: self.thread_id,
            sender: self.sender.clone(),
        }
    }
}

// SAFETY: the capsule is sound to send/share because every accessor and the
// `Drop` impl gate on `thread_id` at runtime — the value itself is never
// touched off its home thread.
unsafe impl<T> Send for Capsule<T> {}
unsafe impl<T> Sync for Capsule<T> {}

// Wrapper that lets the closure shipped to the run loop thread own `T`. SAFETY:
// it is only ever moved straight to that thread and dropped there.
struct Carry<T>(T);

unsafe impl<T> Send for Carry<T> {}
