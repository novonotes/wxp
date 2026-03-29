// 環境検出
export function detectEnvironment(): "tauri" | "wxp" | "unknown" {
	if (typeof window !== "undefined") {
		// wxpプラグインの検出
		if ("__WXP_INTERNALS__" in window) {
			return "wxp";
		}
		// Tauri 2.0の検出
		if ("__TAURI__" in window || "__TAURI_INTERNALS__" in window) {
			return "tauri";
		}
	}
	return "unknown";
}

// invoke関数の実装
export async function invoke<T = unknown>(
	cmd: string,
	args?: Record<string, unknown>,
): Promise<T> {
	const env = detectEnvironment();

	switch (env) {
		case "tauri": {
			// Tauriの場合は動的インポート
			const { invoke: tauriInvoke } = await import("@tauri-apps/api/core");
			return tauriInvoke<T>(cmd, args);
		}

		case "wxp": {
			// wxpプラグインの場合
			// wxpはwindow.invokeを直接提供する
			const windowWithInvoke = window as any;
			if (windowWithInvoke.invoke) {
				return windowWithInvoke.invoke(cmd, args);
			}
			throw new Error("wxp invoke not available");
		}

		default:
			throw new Error("No compatible WebView environment detected");
	}
}

// Channel実装
export class Channel<T = unknown> {
	private channel: any;
	private _onmessage?: (message: T) => void;
	private pendingId: string;
	// private initializationPromise: Promise<void>;

	constructor(onmessage?: (message: T) => void) {
		this._onmessage = onmessage;
		// 一時的なIDを生成
		this.pendingId = `__CHANNEL__:pending_${Math.random()
			.toString(36)
			.substr(2, 9)}`;

		// デバッグ用ログ
		console.log("[webview-bridge] Channel constructor called");
		console.log("[webview-bridge] Environment detection:", {
			__TAURI__: "__TAURI__" in window,
			__TAURI_INTERNALS__: "__TAURI_INTERNALS__" in window,
			__WXP_INTERNALS__: "__WXP_INTERNALS__" in window,
			Channel: "Channel" in window,
		});

		// 同期的な初期化を試みる
		const env = detectEnvironment();
		console.log("[webview-bridge] Detected environment:", env);

		if (env === "tauri") {
			// Tauriの場合は非同期で初期化
			//this.initializationPromise =
			this.initializeTauriChannel();
		} else if (env === "wxp") {
			// wxpの場合は同期的に初期化を試みる
			const wxpChannel = (window as any).Channel;
			if (wxpChannel) {
				this.channel = new wxpChannel(this._onmessage);
				// this.initializationPromise =
				Promise.resolve();
			} else {
				// 非同期で再試行
				// this.initializationPromise =
				this.initializeWxpChannel();
			}
		} else {
			// 環境が不明な場合は非同期で再試行
			// this.initializationPromise =
			this.initializeWithRetry();
		}
	}

	private async initializeTauriChannel() {
		try {
			console.log("[webview-bridge] Initializing Tauri channel...");
			const { Channel: TauriChannel } = await import("@tauri-apps/api/core");
			console.log("[webview-bridge] TauriChannel imported successfully");

			// TauriのChannelはコンストラクタでonmessageを渡す必要がある
			this.channel = new TauriChannel<T>();
			if (this._onmessage) {
				console.log(
					"[webview-bridge] Setting onmessage handler on Tauri channel",
				);
				this.channel.onmessage = this._onmessage;
			}

			console.log("[webview-bridge] Tauri channel created:", this.channel);
			console.log("[webview-bridge] Channel ID:", this.channel.id);
		} catch (error) {
			console.error(
				"[webview-bridge] Failed to initialize Tauri channel:",
				error,
			);
		}
	}

	private async initializeWxpChannel() {
		// 少し待ってから再試行
		await new Promise((resolve) => setTimeout(resolve, 100));
		const retryChannel = (window as any).Channel;
		if (retryChannel) {
			this.channel = new retryChannel(this._onmessage);
			console.log("[webview-bridge] wxp channel created after retry");
		} else {
			console.error("[webview-bridge] wxp Channel not available after retry");
		}
	}

	private async initializeWithRetry() {
		// 環境検出を少し待ってから再試行
		await new Promise((resolve) => setTimeout(resolve, 100));
		const retryEnv = detectEnvironment();
		console.log("[webview-bridge] Retry environment detection:", retryEnv);

		if (retryEnv === "tauri") {
			await this.initializeTauriChannel();
		} else if (retryEnv === "wxp") {
			const wxpChannel = (window as any).Channel;
			if (wxpChannel) {
				this.channel = new wxpChannel(this._onmessage);
				console.log("[webview-bridge] wxp channel created in retry");
			} else {
				await this.initializeWxpChannel();
			}
		} else {
			console.error(
				"[webview-bridge] No compatible WebView environment detected after retry",
			);
		}
	}

	get onmessage() {
		return this.channel?.onmessage;
	}

	set onmessage(handler: ((message: T) => void) | undefined) {
		// 常に内部値を更新
		this._onmessage = handler;

		// チャンネルが初期化されていれば設定
		if (this.channel) {
			console.log("[webview-bridge] Setting onmessage on existing channel");
			this.channel.onmessage = handler;
		} else {
			console.log(
				"[webview-bridge] Channel not yet initialized, storing onmessage for later",
			);
		}
	}

	// Tauriと同じtoJSON実装
	toJSON(): string {
		// 初期化が完了していない場合でも、Tauriはpending IDを処理できるはず
		if (!this.channel) {
			console.warn(
				"[webview-bridge] toJSON called before channel initialization",
			);
			return this.pendingId;
		}

		if (typeof this.channel.toJSON === "function") {
			return this.channel.toJSON();
		}
		// wxpの場合の互換性実装
		if (typeof this.channel.id === "number") {
			return `__CHANNEL__:${this.channel.id}`;
		}

		// wxpのChannelオブジェクトでtoIPCメソッドがある場合
		if (typeof this.channel.toIPC === "function") {
			return this.channel.toIPC();
		}

		throw new Error("Channel not properly initialized");
	}
}

// convertFileSrc関// 環境情報を提供
export const webviewBridge = {
	detectEnvironment,
	invoke,
	Channel,
};

// デフォルトエクスポート
export default webviewBridge;
