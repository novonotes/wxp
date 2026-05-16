type WxpWindow = Window &
	typeof globalThis & {
		invoke?: <T = unknown>(
			cmd: string,
			args?: Record<string, unknown>,
		) => Promise<T>;
		Channel?: new <T = unknown>(
			onmessage?: (message: T) => void,
		) => WxpChannel<T>;
	};

type WxpChannel<T> = {
	onmessage?: (message: T) => void;
	toIPC?: () => string;
};

function wxpWindow(): WxpWindow {
	if (typeof window === "undefined") {
		throw new Error("wxp WebView APIs are only available in a browser window");
	}

	return window as WxpWindow;
}

export async function invoke<T = unknown>(
	cmd: string,
	args?: Record<string, unknown>,
): Promise<T> {
	const bridge = wxpWindow().invoke;
	if (!bridge) {
		throw new Error("wxp invoke is not available");
	}

	return bridge<T>(cmd, args);
}

export class Channel<T = unknown> {
	private channel: WxpChannel<T>;

	constructor(onmessage?: (message: T) => void) {
		const WxpChannel = wxpWindow().Channel;
		if (!WxpChannel) {
			throw new Error("wxp Channel is not available");
		}

		this.channel = new WxpChannel<T>(onmessage);
	}

	get onmessage() {
		return this.channel.onmessage;
	}

	set onmessage(handler: ((message: T) => void) | undefined) {
		this.channel.onmessage = handler;
	}

	toJSON(): string {
		const ipc = this.channel.toIPC?.();
		if (!ipc) {
			throw new Error("wxp Channel does not expose toIPC()");
		}

		return ipc;
	}
}

export const webviewBridge = {
	invoke,
	Channel,
};

export default webviewBridge;
