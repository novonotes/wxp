//! WebView初期化スクリプトの管理モジュール

/// コア機能の初期化スクリプト
/// __WXP_INTERNALS__オブジェクトとコールバック管理機能を提供
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

/// チャンネル機能の初期化スクリプト
/// Channelクラスとfetchメソッドを追加
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
            this.id = window.__WXP_INTERNALS__.transformCallback((rawMessage) => {
                if ('end' in rawMessage && rawMessage.end) {
                    this.#messageEndIndex = rawMessage.index;
                    this.#drainPendingMessages();
                    // Only remove callback if channel is not alive on JS side
                    if (!this.#alive) {
                        window.__WXP_INTERNALS__.callbacks.delete(this.id);
                        window.__WXP_INTERNALS__.channels.delete(this.id);
                    }
                } else if ('message' in rawMessage) {
                    const { message, index } = rawMessage;
                    
                    if (index === this.#nextMessageIndex) {
                        if (this.#onmessage) {
                            this.#onmessage(message);
                        }
                        this.#nextMessageIndex += 1;
                        this.#drainPendingMessages();
                    } else {
                        this.#pendingMessages[index] = message;
                    }
                }
            });
            
            this.#onmessage = onmessage;
            this.#nextMessageIndex = 0;
            this.#pendingMessages = [];
            this.#messageEndIndex = undefined;
            this.#alive = true;
            
            // Register channel globally
            window.__WXP_INTERNALS__.channels.set(this.id, this);
        }
        
        #onmessage;
        #nextMessageIndex;
        #pendingMessages;
        #messageEndIndex;
        #alive;
        
        #drainPendingMessages() {
            while (this.#pendingMessages[this.#nextMessageIndex] !== undefined) {
                const message = this.#pendingMessages[this.#nextMessageIndex];
                delete this.#pendingMessages[this.#nextMessageIndex];
                
                if (this.#onmessage) {
                    this.#onmessage(message);
                }
                this.#nextMessageIndex += 1;
            }
            
            if (this.#messageEndIndex !== undefined && 
                this.#nextMessageIndex >= this.#messageEndIndex) {
                // Only remove callback if channel is not alive on JS side
                if (!this.#alive) {
                    window.__WXP_INTERNALS__.callbacks.delete(this.id);
                    window.__WXP_INTERNALS__.channels.delete(this.id);
                }
            }
        }
        
        set onmessage(handler) {
            this.#onmessage = handler;
        }
        
        close() {
            if (this.#alive) {
                this.#alive = false;
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

/// invoke機能の初期化スクリプト
/// window.invoke関数を追加
const INVOKE_INIT_SCRIPT: &str = r#"
(function() {
    if (!window.__WXP_INTERNALS__ || window.invoke) {
        return;
    }
    
    // invokeコールバック用のストレージを追加
    window.__WXP_INTERNALS__.invoke = Object.create(null);
    
    // IPCキューと待機状態の管理
    const ipcQueue = [];
    let isWaitingForIpc = false;
    
    function waitForIpc() {
        if ('ipc' in window) {
            // IPCが利用可能になったらキューを処理
            for (const action of ipcQueue) {
                action();
            }
            ipcQueue.length = 0; // キューをクリア
        } else {
            // 50ms後に再チェック
            setTimeout(waitForIpc, 50);
        }
    }
    
    // invoke関数をwindowに直接追加
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
                // IPCが既に利用可能なら即実行
                action();
            } else {
                // IPCがまだない場合はキューに追加
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

/// すべての初期化スクリプトを結合して返す
pub(crate) fn get_initialization_scripts(with_invoke: bool) -> String {
    let mut scripts = vec![CORE_INIT_SCRIPT, CHANNEL_INIT_SCRIPT];

    if with_invoke {
        scripts.push(INVOKE_INIT_SCRIPT);
    }

    scripts.join("\n")
}
