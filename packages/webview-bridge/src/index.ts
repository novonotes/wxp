// Environment detection
export function detectEnvironment(): "tauri" | "wxp" | "unknown" {
	if (typeof window !== "undefined") {
		// Detect wxp plugin
		if ("__WXP_INTERNALS__" in window) {
			return "wxp";
		}
		// Detect Tauri 2.0
		if ("__TAURI__" in window || "__TAURI_INTERNALS__" in window) {
			return "tauri";
		}
	}
	return "unknown";
}

// invoke function implementation
export async function invoke<T = unknown>(
	cmd: string,
	args?: Record<string, unknown>,
): Promise<T> {
	const env = detectEnvironment();

	switch (env) {
		case "tauri": {
			// Use dynamic import for Tauri
			const { invoke: tauriInvoke } = await import("@tauri-apps/api/core");
			return tauriInvoke<T>(cmd, args);
		}

		case "wxp": {
			// For wxp plugin
			// wxp provides window.invoke directly
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

// Channel implementation
export class Channel<T = unknown> {
	private channel: any;
	private _onmessage?: (message: T) => void;
	private pendingId: string;
	// private initializationPromise: Promise<void>;

	constructor(onmessage?: (message: T) => void) {
		this._onmessage = onmessage;
		// Generate a temporary ID
		this.pendingId = `__CHANNEL__:pending_${Math.random()
			.toString(36)
			.substr(2, 9)}`;

		// Debug logging
		console.log("[webview-bridge] Channel constructor called");
		console.log("[webview-bridge] Environment detection:", {
			__TAURI__: "__TAURI__" in window,
			__TAURI_INTERNALS__: "__TAURI_INTERNALS__" in window,
			__WXP_INTERNALS__: "__WXP_INTERNALS__" in window,
			Channel: "Channel" in window,
		});

		// Attempt synchronous initialization
		const env = detectEnvironment();
		console.log("[webview-bridge] Detected environment:", env);

		if (env === "tauri") {
			// Initialize asynchronously for Tauri
			//this.initializationPromise =
			this.initializeTauriChannel();
		} else if (env === "wxp") {
			// Attempt synchronous initialization for wxp
			const wxpChannel = (window as any).Channel;
			if (wxpChannel) {
				this.channel = new wxpChannel(this._onmessage);
				// this.initializationPromise =
				Promise.resolve();
			} else {
				// Retry asynchronously
				// this.initializationPromise =
				this.initializeWxpChannel();
			}
		} else {
			// Retry asynchronously if environment is unknown
			// this.initializationPromise =
			this.initializeWithRetry();
		}
	}

	private async initializeTauriChannel() {
		try {
			console.log("[webview-bridge] Initializing Tauri channel...");
			const { Channel: TauriChannel } = await import("@tauri-apps/api/core");
			console.log("[webview-bridge] TauriChannel imported successfully");

			// Tauri's Channel requires onmessage to be passed in the constructor
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
		// Wait briefly then retry
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
		// Wait briefly before retrying environment detection
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
		// Always update the internal value
		this._onmessage = handler;

		// Set on the channel if it has already been initialized
		if (this.channel) {
			console.log("[webview-bridge] Setting onmessage on existing channel");
			this.channel.onmessage = handler;
		} else {
			console.log(
				"[webview-bridge] Channel not yet initialized, storing onmessage for later",
			);
		}
	}

	// Same toJSON implementation as Tauri
	toJSON(): string {
		// Even if initialization is not complete, Tauri should be able to handle a pending ID
		if (!this.channel) {
			console.warn(
				"[webview-bridge] toJSON called before channel initialization",
			);
			return this.pendingId;
		}

		if (typeof this.channel.toJSON === "function") {
			return this.channel.toJSON();
		}
		// Compatibility implementation for wxp
		if (typeof this.channel.id === "number") {
			return `__CHANNEL__:${this.channel.id}`;
		}

		// If the wxp Channel object has a toIPC method
		if (typeof this.channel.toIPC === "function") {
			return this.channel.toIPC();
		}

		throw new Error("Channel not properly initialized");
	}
}

// Provides environment information
export const webviewBridge = {
	detectEnvironment,
	invoke,
	Channel,
};

// Default export
export default webviewBridge;
