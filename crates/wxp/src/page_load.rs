use std::{cell::Cell, rc::Rc};

use wry::PageLoadEvent;

/// How far the current page has loaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageLoadState {
    /// No content has started loading. Native WebView creation may still be running, because some
    /// backends continue it asynchronously after the builder has returned.
    NotStarted,
    Started,
    /// Navigation has ended. This also covers a failed navigation.
    Finished,
}

/// Handle for reading [`PageLoadState`] on the thread that owns the WebView.
///
/// It can outlive the WebView, so a watchdog can hold it without extending the WebView's lifetime.
#[derive(Clone)]
pub struct PageLoadMonitor {
    state: Rc<Cell<PageLoadState>>,
}

impl PageLoadMonitor {
    pub(crate) fn new() -> Self {
        Self {
            state: Rc::new(Cell::new(PageLoadState::NotStarted)),
        }
    }

    /// Returns the latest page load state.
    pub fn state(&self) -> PageLoadState {
        self.state.get()
    }

    pub(crate) fn record(&self, event: PageLoadEvent) {
        let state = match event {
            PageLoadEvent::Started => PageLoadState::Started,
            PageLoadEvent::Finished => PageLoadState::Finished,
        };
        log::info!("WebView page load state: {state:?}");
        self.state.set(state);
    }
}
