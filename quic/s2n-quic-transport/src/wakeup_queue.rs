//! A queue which allows to wake up a QUIC endpoint which is blocked on packet reception or timers.
//! This queue is used in case connections inside the endpoint change their readiness state change
//! their readiness state (e.g. they get ready to write).

use alloc::{collections::VecDeque, sync::Arc};
use core::task::{Context, Waker};
use std::sync::Mutex;

/// The shared state of the [`WakeupQueue`].
struct QueueState<T> {
    /// The IDs of connections which have been woken
    woken_connections: VecDeque<T>,
    /// The waker which should be used to wake up the connection
    waker: Option<Waker>,
    /// Whether a wakeup is already in progress
    wakeup_in_progress: bool,
}

impl<T: Copy> QueueState<T> {
    fn new() -> Self {
        Self {
            woken_connections: VecDeque::new(),
            waker: None,
            wakeup_in_progress: false,
        }
    }

    fn queue_wakeup(&mut self, wakeup_handle_id: T) -> Option<Waker> {
        self.woken_connections.push_back(wakeup_handle_id);
        // If pushing another handle already notified the processing thread that it should dequeue
        // pending notifications there is no need to do this again.
        if self.wakeup_in_progress {
            return None;
        }

        self.wakeup_in_progress = true;
        self.waker.clone()
    }

    /// Polls for queued wakeup events.
    /// The method gets passed a queued which is used to store further wakeup events.
    /// It will returns a queue of occurred events.
    /// If no wakeup occurred, the method will store the passed [`Waker`] and notify it as soon as
    /// a wakeup occured.
    fn poll_pending_wakeups(&mut self, swap_queue: VecDeque<T>, context: &Context) -> VecDeque<T> {
        let result = core::mem::replace(&mut self.woken_connections, swap_queue);

        self.wakeup_in_progress = false;
        if result.is_empty() {
            // If no wakeup was pending, store or update the `Waker`
            match &self.waker {
                Some(w) => {
                    if !w.will_wake(context.waker()) {
                        self.waker = Some(context.waker().clone());
                    }
                }
                None => self.waker = Some(context.waker().clone()),
            }
        }

        // Clear the passed queue in case the caller did not clean it
        self.woken_connections.clear();

        result
    }
}

/// A queue which allows individual components to wakeups to a common blocked thread.
///
/// Multiple components can notify the thread to unblocked and to dequeue handles of components.///
/// Each component is identified by a handle of type `T`.
///
/// A single thread is expected to deque the handles of blocked components and to inform those.
pub struct WakeupQueue<T> {
    state: Arc<Mutex<QueueState<T>>>,
}

impl<T: Copy> WakeupQueue<T> {
    /// Creates a new `WakeupQueue`.
    ///
    /// If a wakeup is triggered, the given [`Waker`] will be notified.
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(QueueState::new())),
        }
    }

    /// Creates a new [`WakeupHandle`] which will wake up this [`WakeupQueue`] if
    /// [`WakeupHandle::wakeup`] is called.
    pub fn create_wakeup_handle(&self, wakeup_handle_id: T) -> WakeupHandle<T> {
        WakeupHandle::new(self.state.clone(), wakeup_handle_id)
    }

    /// Returns the list of component handles which need to get woken.
    /// Those component handles are retrieved inside a `VecDeque`. In order to avoid
    /// memory allocations, the caller is expected to pass in a new `VecDequeue` which will
    /// by utilized for further queueing. Thereby a double-buffering approach for wakeups is
    /// enabled.
    pub fn poll_pending_wakeups(
        &mut self,
        swap_queue: VecDeque<T>,
        context: &Context,
    ) -> VecDeque<T> {
        let mut guard = self
            .state
            .lock()
            .expect("Locking can only fail if locks are poisoned");
        guard.poll_pending_wakeups(swap_queue, context)
    }
}

/// A handle which refers to a wakeup queue. The handles allows to notify the
/// queue that a wakeup is required, and that after the wakeup the owner of the handle
/// wants to be notified.
pub struct WakeupHandle<T> {
    /// The queue this handle is referring to
    queue: Arc<Mutex<QueueState<T>>>,
    /// The internal ID of this wakeup handle. This can be used to distinguish which
    /// handle had woken up the [`WakeupQueue`].
    wakeup_handle_id: T,
    /// Whether a wakeup for this handle had already been queued since the last time
    /// the wakeup handler was called
    wakeup_queued: bool,
}

impl<T: Copy> WakeupHandle<T> {
    /// Creates a new [`WakeupHandle`] which delegates wakeups to the given `queue`.
    fn new(queue: Arc<Mutex<QueueState<T>>>, wakeup_handle_id: T) -> Self {
        Self {
            queue,
            wakeup_handle_id,
            wakeup_queued: false,
        }
    }

    /// Notifies the queue to wake up. If a `wakeup()` had been issued for the same
    /// [`WakeupHandle`] without having been handled yet, the new [`wakeup()`] request will be
    /// ignored, since the wakeup will already be pending.
    pub fn wakeup(&mut self) {
        // Check if a wakeup had been queued earlier
        if self.wakeup_queued {
            return;
        }

        // Enqueue the wakeup request
        self.wakeup_queued = true;
        let maybe_waker = {
            let mut guard = self
                .queue
                .lock()
                .expect("Locking can only fail if locks are poisoned");
            guard.queue_wakeup(self.wakeup_handle_id)
        };

        // If the queue handling thread wasn't notified earlier by another thread,
        // notify it now.
        if let Some(waker) = maybe_waker {
            waker.wake();
        }
    }

    /// Notifies the `WakeupHandle` that a wakeup for this handle had been processed.
    ///
    /// Further calls to [`wakeup`] will be queued again.
    pub fn wakeup_handled(&mut self) {
        self.wakeup_queued = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::iter::FromIterator;
    use futures_test::task::new_count_waker;

    #[test]
    fn queue_wakeups() {
        let (waker, counter) = new_count_waker();
        let mut queue = WakeupQueue::new();
        let pending = VecDeque::new();

        let mut handle1 = queue.create_wakeup_handle(1u32);
        let mut handle2 = queue.create_wakeup_handle(2u32);
        assert_eq!(counter, 0);

        // Initially no wakeup should be signalled - but the Waker should be stored
        let pending = queue.poll_pending_wakeups(pending, &Context::from_waker(&waker));
        assert_eq!(VecDeque::<u32>::from_iter(&mut [].iter().cloned()), pending);

        // After a wakeup the waker should be notified
        handle1.wakeup();
        assert_eq!(counter, 1);
        // A second wakeup on the same handle should not lead to another global wakeup
        handle1.wakeup();
        assert_eq!(counter, 1);

        // Even a second wakeup on the other handle should not lead to a global wakeup
        handle2.wakeup();
        assert_eq!(counter, 1);

        // The pending wakeups should be signaled
        let pending = queue.poll_pending_wakeups(pending, &Context::from_waker(&waker));
        assert_eq!(
            VecDeque::<u32>::from_iter(&mut [1u32, 2u32].iter().cloned()),
            pending
        );

        // In the next query no wakeups should be signaled
        let pending = queue.poll_pending_wakeups(pending, &Context::from_waker(&waker));
        assert_eq!(VecDeque::<u32>::from_iter(&mut [].iter().cloned()), pending);

        // As long as wakeups are not handled, no new ones are enqueued
        handle2.wakeup();
        assert_eq!(counter, 1);
        let pending = queue.poll_pending_wakeups(pending, &Context::from_waker(&waker));
        assert_eq!(VecDeque::<u32>::from_iter(&mut [].iter().cloned()), pending);

        // If wakeups are handled, wakeups are forwarded again
        handle1.wakeup_handled();
        handle2.wakeup_handled();

        handle2.wakeup();
        assert_eq!(counter, 2);
        let pending = queue.poll_pending_wakeups(pending, &Context::from_waker(&waker));
        assert_eq!(
            VecDeque::<u32>::from_iter(&mut [2u32].iter().cloned()),
            pending
        );
    }
}