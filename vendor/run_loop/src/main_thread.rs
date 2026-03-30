use std::sync::Mutex;

use crate::{
    Result, RunLoop, RunLoopSender,
    platform::{self, PlatformThreadId},
};

pub(crate) enum MainThreadFacilitator {
    #[cfg(feature = "flutter")]
    EngineContext,
    Manual {
        thread_id: PlatformThreadId,
        sender: RunLoopSender,
    },
}

static MAIN_THREAD_FACILITATOR: Mutex<Option<MainThreadFacilitator>> = Mutex::new(None);

impl MainThreadFacilitator {
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

    /// 現在のスレッドの設定をリセットする
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
