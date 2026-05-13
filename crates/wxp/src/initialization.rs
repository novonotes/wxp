//! Module for managing WebView initialization scripts

/// Core feature initialization script
/// Provides the __WXP_INTERNALS__ object and callback management functionality
const CORE_INIT_SCRIPT: &str = r#"
(function() {
    if (window.__WXP_INTERNALS__) {
        return;
    }

    const callbacks = new Map();
    const channels = new Map();
    let callbackIdCounter = 0;

    window.__WXP_INTERNALS__ = {
        callbacks: callbacks,
        channels: channels,

        transformCallback: function(callback, once = false) {
            const id = callbackIdCounter++;
            callbacks.set(id, { callback, once });
            return id;
        },

        runCallback: function(id, data) {
            const item = callbacks.get(id);
            if (item) {
                item.callback(data);
                if (item.once) {
                    callbacks.delete(id);
                }
            }
        }
    };
})();
"#;

/// Channel feature initialization script
/// Adds the Channel class and fetch method
const CHANNEL_INIT_SCRIPT: &str = r#"
(function() {
    if (!window.__WXP_INTERNALS__ || window.__WXP_INTERNALS__.fetchChannelData) {
        return;
    }

    window.__WXP_INTERNALS__.fetchChannelData = async function(command, headers) {
        // Windows requires http://scheme.localhost format, while other platforms use scheme://localhost
        let url;
        if (navigator.userAgent.includes('Windows')) {
            url = 'http://wxp-channel.localhost/fetch';
        } else {
            url = 'wxp-channel://localhost/fetch';
        }

        const response = await fetch(url, {
            method: 'GET',
            headers: headers
        });
        if (!response.ok) {
            throw new Error(`HTTP error! status: ${response.status}`);
        }

        const contentType = response.headers.get('content-type');
        if (contentType && contentType.includes('application/octet-stream')) {
            return await response.arrayBuffer();
        } else {
            return await response.json();
        }
    };

    class Channel {
        constructor(onmessage) {
            // WKWebView on macOS 11.0.1 (Safari 14.0 equivalent) cannot parse private class
            // fields (`#field`). Use prototype-compatible properties so the entire bridge
            // initialization script does not fail on older engines.
            this._onmessage = onmessage;
            this._nextMessageIndex = 0;
            this._pendingMessages = [];
            this._messageEndIndex = undefined;
            this._alive = true;

            this.id = window.__WXP_INTERNALS__.transformCallback((rawMessage) => {
                if ('end' in rawMessage && rawMessage.end) {
                    this._messageEndIndex = rawMessage.index;
                    this._drainPendingMessages();
                    // Only remove callback if channel is not alive on JS side
                    if (!this._alive) {
                        window.__WXP_INTERNALS__.callbacks.delete(this.id);
                        window.__WXP_INTERNALS__.channels.delete(this.id);
                    }
                } else if ('message' in rawMessage) {
                    const { message, index } = rawMessage;

                    if (index === this._nextMessageIndex) {
                        if (this._onmessage) {
                            this._onmessage(message);
                        }
                        this._nextMessageIndex += 1;
                        this._drainPendingMessages();
                    } else {
                        this._pendingMessages[index] = message;
                    }
                }
            });

            // Register channel globally
            window.__WXP_INTERNALS__.channels.set(this.id, this);
        }

        _drainPendingMessages() {
            while (this._pendingMessages[this._nextMessageIndex] !== undefined) {
                const message = this._pendingMessages[this._nextMessageIndex];
                delete this._pendingMessages[this._nextMessageIndex];

                if (this._onmessage) {
                    this._onmessage(message);
                }
                this._nextMessageIndex += 1;
            }

            if (this._messageEndIndex !== undefined &&
                this._nextMessageIndex >= this._messageEndIndex) {
                // Only remove callback if channel is not alive on JS side
                if (!this._alive) {
                    window.__WXP_INTERNALS__.callbacks.delete(this.id);
                    window.__WXP_INTERNALS__.channels.delete(this.id);
                }
            }
        }

        set onmessage(handler) {
            this._onmessage = handler;
        }

        close() {
            if (this._alive) {
                this._alive = false;
                window.__WXP_INTERNALS__.callbacks.delete(this.id);
                window.__WXP_INTERNALS__.channels.delete(this.id);
            }
        }

        toIPC() {
            return `__CHANNEL__:${this.id}`;
        }
    }

    window.Channel = Channel;
})();
"#;

/// invoke feature initialization script
/// Adds the window.invoke function
const INVOKE_INIT_SCRIPT: &str = r#"
(function() {
    if (!window.__WXP_INTERNALS__ || window.invoke) {
        return;
    }

    // Add storage for invoke callbacks
    window.__WXP_INTERNALS__.invoke = Object.create(null);

    // IPC queue and waiting state management
    const ipcQueue = [];
    let isWaitingForIpc = false;

    function waitForIpc() {
        if ('ipc' in window) {
            // Process the queue once IPC becomes available
            for (const action of ipcQueue) {
                action();
            }
            ipcQueue.length = 0; // Clear the queue
        } else {
            // Check again after 50ms
            setTimeout(waitForIpc, 50);
        }
    }

    // Add the invoke function directly to window
    window.invoke = function(cmd, args = {}) {
        return new Promise((resolve, reject) => {
            const callback = window.__WXP_INTERNALS__.transformCallback((response) => {
                resolve(response);
                delete window.__WXP_INTERNALS__.invoke[callback];
                delete window.__WXP_INTERNALS__.invoke[error];
            }, true);

            const error = window.__WXP_INTERNALS__.transformCallback((e) => {
                reject(e);
                delete window.__WXP_INTERNALS__.invoke[callback];
                delete window.__WXP_INTERNALS__.invoke[error];
            }, true);

            window.__WXP_INTERNALS__.invoke[callback] = resolve;
            window.__WXP_INTERNALS__.invoke[error] = reject;

            const message = {
                cmd: cmd,
                callback: callback,
                error: error,
                inner: args
            };

            const action = () => {
                window.ipc.postMessage(JSON.stringify(message));
            };

            if ('ipc' in window) {
                // Execute immediately if IPC is already available
                action();
            } else {
                // Queue the action if IPC is not yet available
                ipcQueue.push(action);
                if (!isWaitingForIpc) {
                    waitForIpc();
                    isWaitingForIpc = true;
                }
            }
        });
    };
})();
"#;

/// Concatenates and returns all initialization scripts
pub(crate) fn get_initialization_scripts(with_invoke: bool) -> String {
    let mut scripts = vec![CORE_INIT_SCRIPT, CHANNEL_INIT_SCRIPT];

    if with_invoke {
        scripts.push(INVOKE_INIT_SCRIPT);
    }

    scripts.join("\n")
}
