//! Hand-written GLib/GTK FFI for the Linux backend.
//!
//! Only the few symbols this crate uses are declared, to avoid pulling in a
//! full glib/gtk-sys dependency. Types and names mirror the C API verbatim;
//! keep them in sync with GLib rather than "tidying" them.

#[allow(non_camel_case_types)]
pub(crate) mod glib {
    use std::os::raw::{c_int, c_uint, c_void};
    pub(crate) type gboolean = c_int;
    pub(crate) type gpointer = *mut c_void;
    pub(crate) type GSourceFunc = Option<unsafe extern "C" fn(gpointer) -> gboolean>;
    pub(crate) type GDestroyNotify = Option<unsafe extern "C" fn(gpointer)>;
    pub(crate) const GFALSE: c_int = 0;
    pub(crate) const GTRUE: c_int = 1;
    pub(crate) const G_SOURCE_REMOVE: gboolean = GFALSE;

    #[repr(C)]
    pub(crate) struct GSource(c_void);

    #[repr(C)]
    pub(crate) struct GMainContext(c_void);

    #[repr(C)]
    pub(crate) struct GMainLoop(c_void);

    #[link(name = "glib-2.0")]
    unsafe extern "C" {
        pub(crate) fn g_main_loop_new(
            context: *mut GMainContext,
            is_running: gboolean,
        ) -> *mut GMainLoop;
        pub(crate) fn g_main_loop_unref(loop_: *mut GMainLoop);
        pub(crate) fn g_main_loop_run(loop_: *mut GMainLoop);
        pub(crate) fn g_main_loop_quit(loop_: *mut GMainLoop);
        pub(crate) fn g_main_context_push_thread_default(context: *mut GMainContext);
        pub(crate) fn g_main_context_pop_thread_default(context: *mut GMainContext);

        pub(crate) fn g_timeout_source_new(interval: c_uint) -> *mut GSource;
        pub(crate) fn g_source_set_callback(
            source: *mut GSource,
            func: GSourceFunc,
            data: gpointer,
            notify: GDestroyNotify,
        );
        pub(crate) fn g_source_attach(source: *mut GSource, context: *mut GMainContext) -> c_uint;
        pub(crate) fn g_source_unref(source: *mut GSource);
        pub(crate) fn g_source_destroy(source: *mut GSource);
        pub(crate) fn g_main_context_find_source_by_id(
            context: *mut GMainContext,
            source_id: c_uint,
        ) -> *mut GSource;
        pub(crate) fn g_main_context_ref(context: *mut GMainContext) -> *mut GMainContext;
        pub(crate) fn g_main_context_unref(context: *mut GMainContext);
        pub(crate) fn g_main_context_invoke_full(
            context: *mut GMainContext,
            priority: c_int,
            function: GSourceFunc,
            data: gpointer,
            notify: GDestroyNotify,
        );
        pub(crate) fn g_main_context_default() -> *mut GMainContext;
        pub(crate) fn g_main_context_new() -> *mut GMainContext;
        pub(crate) fn g_main_context_get_thread_default() -> *mut GMainContext;
        pub(crate) fn g_main_context_is_owner(context: *mut GMainContext) -> gboolean;
        pub(crate) fn g_main_context_iteration(
            context: *mut GMainContext,
            may_block: gboolean,
        ) -> gboolean;
    }
    #[link(name = "gtk-3")]
    unsafe extern "C" {
        pub(crate) fn gtk_main();
        pub(crate) fn gtk_main_iteration();
        pub(crate) fn gtk_main_quit();
    }
}

#[allow(non_camel_case_types)]
pub(crate) mod libc {
    unsafe extern "C" {
        pub(crate) fn pthread_self() -> usize;
    }
}
