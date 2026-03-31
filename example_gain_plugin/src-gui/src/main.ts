import { Channel, invoke } from "@novonotes/webview-bridge";
import "./style.css";

type GainState = {
  type: "gain-state";
  value: number;
  dbText: string;
};

const MIN_GAIN = 0;
const MAX_GAIN = 2;
const MIN_ANGLE = -135;
const MAX_ANGLE = 135;

const valueLabel = document.querySelector<HTMLDivElement>("#gain-value");
const dbLabel = document.querySelector<HTMLDivElement>("#gain-db");
const knob = document.querySelector<HTMLButtonElement>("#gain-knob");
const indicator = document.querySelector<HTMLDivElement>("#knob-indicator");
const fill = document.querySelector<HTMLDivElement>("#knob-fill");

if (!valueLabel || !dbLabel || !knob || !indicator || !fill) {
  throw new Error("required elements not found");
}

let gain = 1;
let dragging = false;
let dragStartY = 0;
let dragStartGain = gain;
let gestureActive = false;

const channel = new Channel<GainState>((message) => {
  if (message && message.type === "gain-state") {
    render(message);
  }
});

void (async () => {
  const initialState = await invoke<GainState>("get_gain_state");
  render(initialState);
  await invoke("subscribe_gain", { channel });
})();

function clamp(value: number): number {
  return Math.min(MAX_GAIN, Math.max(MIN_GAIN, value));
}

function gainToAngle(value: number): number {
  const normalized = (value - MIN_GAIN) / (MAX_GAIN - MIN_GAIN);
  return MIN_ANGLE + normalized * (MAX_ANGLE - MIN_ANGLE);
}

function render(state: GainState): void {
  gain = clamp(state.value);
  valueLabel.textContent = `${gain.toFixed(2)}x`;
  dbLabel.textContent = state.dbText;
  const angle = gainToAngle(gain);
  indicator.style.transform = `rotate(${angle}deg)`;
  fill.style.transform = `rotate(${angle}deg)`;
}

function beginGesture(): void {
  if (gestureActive) {
    return;
  }
  gestureActive = true;
  void invoke("begin_parameter_gesture");
}

function endGesture(): void {
  if (!gestureActive) {
    return;
  }
  gestureActive = false;
  void invoke("end_parameter_gesture");
}

function applyGain(nextGain: number): void {
  const value = clamp(nextGain);
  render({
    type: "gain-state",
    value,
    dbText:
      value <= 0 ? "-inf dB" : `${(20 * Math.log10(value)).toFixed(1)} dB`,
  });
  void invoke("set_gain", { value });
}

knob.addEventListener("pointerdown", (event) => {
  dragging = true;
  dragStartY = event.clientY;
  dragStartGain = gain;
  knob.setPointerCapture(event.pointerId);
  beginGesture();
});

knob.addEventListener("pointermove", (event) => {
  if (!dragging) {
    return;
  }
  const delta = (dragStartY - event.clientY) / 180;
  applyGain(dragStartGain + delta);
});

const finishDrag = (event: PointerEvent) => {
  if (!dragging) {
    return;
  }
  dragging = false;
  knob.releasePointerCapture(event.pointerId);
  endGesture();
};

knob.addEventListener("pointerup", finishDrag);
knob.addEventListener("pointercancel", finishDrag);

knob.addEventListener("wheel", (event) => {
  event.preventDefault();
  beginGesture();
  applyGain(gain - event.deltaY * 0.0015);
  window.clearTimeout((knob as unknown as { wheelTimer?: number }).wheelTimer);
  (knob as unknown as { wheelTimer?: number }).wheelTimer = window.setTimeout(
    () => {
      endGesture();
    },
    120,
  );
});

window.addEventListener("beforeunload", () => {
  endGesture();
  void invoke("unsubscribe_gain");
});
