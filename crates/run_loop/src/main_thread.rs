use std::sync::Mutex;

use crate::{
    Result, RunLoop, RunLoopSender,
    platform::{self, PlatformThreadId},
};

/// Records which thread is "the main thread" and how to post work onto it.
///
/// A plugin host owns the real main thread, so the crate cannot assume it. This
/// enum captures the strategy chosen at init time:
/// - `EngineContext`: defer the decision to Flutter's engine context.
/// - `Manual`: the thread that called `init` is the main thread, reachable via
///   its `RunLoopSender`.
pub(crate) enum MainThreadFacilitator {
    #[cfg(feature = "flutter")]
    EngineContext,
    Manual {
        thread_id: PlatformThreadId,
        sender: RunLoopSender,
    },
}

// Process-global: there is exactly one main thread per process. `None` until a
// strategy is chosen, so misuse before `init` is detectable rather than silent.
static MAIN_THREAD_FACILITATOR: Mutex<Option<MainThreadFacilitator>> = Mutex::new(None);

impl MainThreadFacilitator {
    /// Pins the current thread as the main thread for the current init
    /// generation (cleared by [`reset`](Self::reset) on `deinit`).
    ///
    /// Calling it again from the *same* thread within a generation is a no-op.
    /// A *conflicting* re-set — a different thread, or after the Flutter engine
    /// context took over — is a programming error and panics loudly rather than
    /// silently routing work to the wrong thread.
    pub(crate) fn set_for_current_thread() {
        let mut facilitator = MAIN_THREAD_FACILITATOR.lock().unwrap();

        if let Some(existing) = facilitator.as_ref() {
            match existing {
                #[cfg(feature = "flutter")]
                MainThreadFacilitator::EngineContext => {
                    panic!("RunLoop::set_as_main_thread() was called after other RunLoop methods.");
                }
                MainThreadFacilitator::Manual {
                    thread_id,
                    sender: _,
                } => {
                    if *thread_id != platform::get_system_thread_id() {
                        panic!(
                            "RunLoop::set_as_main_thread() was already called on another thread."
                        );
                    }
                }
            }
        } else {
            *facilitator = Some(MainThreadFacilitator::Manual {
                thread_id: platform::get_system_thread_id(),
                sender: RunLoop::current().new_sender(),
            });
        }
    }

    /// Clears the pinned main thread so a later `init` can pick a new one.
    ///
    /// Paired with `deinit`; required because the same process (e.g. a host that
    /// reloads the plugin) may go through several init/deinit cycles.
    pub(crate) fn reset() {
        let mut facilitator = MAIN_THREAD_FACILITATOR.lock().unwrap();
        *facilitator = None;
    }

    fn with_facilitator<F, R>(f: F) -> R
    where
        F: FnOnce(&MainThreadFacilitator) -> R,
    {
        #[cfg_attr(not(feature = "flutter"), allow(unused_mut))]
        let mut facilitator = MAIN_THREAD_FACILITATOR.lock().unwrap();
        // Under Flutter the engine sets the main thread up implicitly, so a
        // missing facilitator just means "use the engine context". Without
        // Flutter, an unset facilitator is a real bug (forgot `init`) and must
        // not be papered over.
        if facilitator.is_none() {
            #[cfg(feature = "flutter")]
            {
                *facilitator = Some(MainThreadFacilitator::EngineContext);
            }
            #[cfg(not(feature = "flutter"))]
            {
                panic!("MainThreadFacilitator is not initialized. Call RunLoop::init() first.");
            }
        }
        f(facilitator.as_ref().unwrap())
    }

    pub(crate) fn is_main_thread() -> Result<bool> {
        Self::with_facilitator(|f| match f {
            #[cfg(feature = "flutter")]
            MainThreadFacilitator::EngineContext => {
                Ok(irondash_engine_context::EngineContext::is_main_thread()?)
            }
            MainThreadFacilitator::Manual {
                thread_id,
                sender: _,
            } => Ok(*thread_id == platform::get_system_thread_id()),
        })
    }

    pub(crate) fn perform_on_main_thread(f: impl FnOnce() + Send + 'static) -> Result<()> {
        Self::with_facilitator(|facilitator| match facilitator {
            #[cfg(feature = "flutter")]
            MainThreadFacilitator::EngineContext => {
                Ok(irondash_engine_context::EngineContext::perform_on_main_thread(f)?)
            }
            MainThreadFacilitator::Manual {
                thread_id: _,
                sender,
            } => {
                sender.send(f);
                Ok(())
            }
        })
    }
}
