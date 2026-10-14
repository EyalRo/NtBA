// Thin host for the NtBA wasm core: keyboard in, fixed-timestep ticks, pixels out.

const BUTTONS = {
  UP: 1 << 0, DOWN: 1 << 1, LEFT: 1 << 2, RIGHT: 1 << 3,
  PASS: 1 << 4, SHOOT: 1 << 5, SWITCH: 1 << 6, START: 1 << 7,
};

const KEYMAP = {
  ArrowUp: BUTTONS.UP, KeyW: BUTTONS.UP,
  ArrowDown: BUTTONS.DOWN, KeyS: BUTTONS.DOWN,
  ArrowLeft: BUTTONS.LEFT, KeyA: BUTTONS.LEFT,
  ArrowRight: BUTTONS.RIGHT, KeyD: BUTTONS.RIGHT,
  KeyZ: BUTTONS.PASS, KeyJ: BUTTONS.PASS,
  KeyX: BUTTONS.SHOOT, KeyK: BUTTONS.SHOOT,
  KeyC: BUTTONS.SWITCH, Space: BUTTONS.SWITCH,
  Enter: BUTTONS.START,
};

const status = document.getElementById('status');
const canvas = document.getElementById('screen');
const ctx = canvas.getContext('2d');

let held = 0;
// Presses shorter than one tick (a quick tap between frames) must still register.
let pending = 0;

window.addEventListener('keydown', (e) => {
  const bit = KEYMAP[e.code];
  if (bit === undefined) return;
  e.preventDefault();
  held |= bit;
  pending |= bit;
});
window.addEventListener('keyup', (e) => {
  const bit = KEYMAP[e.code];
  if (bit === undefined) return;
  e.preventDefault();
  held &= ~bit;
});
window.addEventListener('blur', () => { held = 0; pending = 0; });

async function main() {
  const response = await fetch('ntba.wasm');
  if (!response.ok) throw new Error(`ntba.wasm: HTTP ${response.status} (run scripts/build-web.sh first)`);
  const { instance } = await WebAssembly.instantiate(await response.arrayBuffer(), {});
  const wasm = instance.exports;

  const width = wasm.ntba_width();
  const height = wasm.ntba_height();
  const tickMs = 1000 / wasm.ntba_tick_hz();
  canvas.width = width;
  canvas.height = height;
  const image = ctx.createImageData(width, height);

  wasm.ntba_init((Date.now() & 0xffffffff) >>> 0);
  status.textContent = '';
  canvas.focus();

  let last = performance.now();
  let acc = 0;
  function frame(now) {
    // Clamp so a background tab doesn't trigger a huge catch-up burst.
    acc += Math.min(now - last, 250);
    last = now;
    while (acc >= tickMs) {
      wasm.ntba_step(held | pending);
      pending = 0;
      acc -= tickMs;
    }
    wasm.ntba_render();
    // Memory may have moved; always re-create the view from the current buffer.
    const pixels = new Uint8ClampedArray(wasm.memory.buffer, wasm.ntba_framebuffer(), width * height * 4);
    image.data.set(pixels);
    ctx.putImageData(image, 0, 0);
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);
}

main().catch((err) => {
  console.error(err);
  status.textContent = `Failed to start: ${err.message}`;
});
