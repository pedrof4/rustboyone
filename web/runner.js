const romInput = document.querySelector("#rom");
const status = document.querySelector("#status");
const pause = document.querySelector("#pause");
const audio = document.querySelector("#audio");
const canvas = document.querySelector("#screen");
const context = canvas.getContext("2d", { alpha: false });
const image = context.createImageData(160, 144);
const keyboard = new Set();
const pointers = new Map();
const keyMap = new Map([
  ["ArrowRight", 0], ["ArrowLeft", 1], ["ArrowUp", 2], ["ArrowDown", 3],
  ["KeyZ", 4], ["KeyX", 5], ["ShiftLeft", 6], ["ShiftRight", 6], ["Enter", 7],
]);

let core;
let game;
let paused = false;
let crashed = false;
let loadRequest = 0;
let sound;
let audioNext = 0;
const playing = new Set();
let previousTime;
let balance = 0;
let lastSaveTime = 0;

function paint() {
  const pixels = new Uint8Array(core.memory.buffer, core.dmg_frame(), 160 * 144);
  for (let i = 0; i < pixels.length; i++) {
    const gray = 255 - 85 * pixels[i];
    const index = i * 4;
    image.data[index] = gray;
    image.data[index + 1] = gray;
    image.data[index + 2] = gray;
    image.data[index + 3] = 255;
  }
  context.putImageData(image, 0, 0);
}

function keys() {
  let pressed = 0;
  for (const code of keyboard) pressed |= 1 << keyMap.get(code);
  for (const bit of pointers.values()) pressed |= 1 << bit;
  return pressed;
}

function loadBytes(bytes) {
  const pointer = core.dmg_staging(bytes.length);
  if (!pointer) throw new Error("O arquivo excede o tamanho máximo de uma ROM DMG.");
  new Uint8Array(core.memory.buffer, pointer, bytes.length).set(bytes);
}

function haltAudio() {
  for (const node of playing) node.stop();
  playing.clear();
  audioNext = 0;
}

function drainAudio() {
  const length = core.dmg_audio_len();
  if (length && sound?.state === "running" && audio.getAttribute("aria-pressed") === "true") {
    const samples = new Int16Array(core.memory.buffer, core.dmg_audio_ptr(), length);
    const buffer = sound.createBuffer(2, length / 2, core.dmg_audio_rate());
    for (let side = 0; side < 2; side++) {
      const channel = buffer.getChannelData(side);
      for (let i = 0; i < channel.length; i++) channel[i] = samples[2 * i + side] / 32768;
    }
    if (audioNext <= sound.currentTime) audioNext = sound.currentTime + .045;
    if (audioNext < sound.currentTime + .16) {
      const node = sound.createBufferSource();
      node.buffer = buffer;
      node.connect(sound.destination);
      node.start(audioNext);
      audioNext += buffer.duration;
      playing.add(node);
      node.onended = () => playing.delete(node);
    }
  }
  core.dmg_audio_clear();
}

function save() {
  if (!game || !core.dmg_save_len()) return;
  try {
    const bytes = new Uint8Array(core.memory.buffer, core.dmg_save_ptr(), core.dmg_save_len());
    let text = "";
    for (let i = 0; i < bytes.length; i += 4096) text += String.fromCharCode(...bytes.subarray(i, i + 4096));
    const encoded = btoa(text);
    if (encoded !== game.saved) {
      localStorage.setItem(game.key, encoded);
      game.saved = encoded;
    }
  } catch {
    // Private browsing, quota or local file policies can disable storage.
  }
}

async function romKey(bytes) {
  if (globalThis.crypto?.subtle) {
    const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
    return "dmg-emu:" + Array.from(digest, n => n.toString(16).padStart(2, "0")).join("");
  }
  let hash = 2166136261;
  for (const byte of bytes) hash = Math.imul(hash ^ byte, 16777619);
  return `dmg-emu:${bytes.length}:${hash >>> 0}`;
}

romInput.disabled = true;
try {
  const response = await fetch("dmg_emu.wasm");
  if (!response.ok) throw new Error("Build WebAssembly indisponível.");
  core = (await WebAssembly.instantiate(await response.arrayBuffer())).instance.exports;
  romInput.disabled = false;
  status.textContent = "Abra uma ROM de Game Boy.";
} catch (error) {
  status.textContent = `${error.message} Execute python scripts/serve_web.py.`;
}

romInput.addEventListener("change", async () => {
  const file = romInput.files[0];
  const request = ++loadRequest;
  if (!file || !core) return;
  try {
    if (file.size > 8 * 1024 * 1024) throw new Error("O arquivo excede 8 MiB.");
    const bytes = new Uint8Array(await file.arrayBuffer());
    const key = await romKey(bytes);
    if (request !== loadRequest) return;
    save();
    loadBytes(bytes);
    if (!core.dmg_load()) {
      const error = new Uint8Array(core.memory.buffer, core.dmg_error_ptr(), core.dmg_error_len());
      throw new Error(new TextDecoder().decode(error));
    }
    game = { key, name: file.name };
    try {
      const saved = localStorage.getItem(key);
      if (saved && core.dmg_save_len()) {
        const ram = Uint8Array.from(atob(saved), letter => letter.charCodeAt(0));
        if (ram.length === core.dmg_save_len()) {
          loadBytes(ram);
          core.dmg_restore();
          game.saved = saved;
        }
      }
    } catch {
      // Storage is optional.
    }
    paused = false;
    crashed = false;
    balance = 0;
    pause.textContent = "Pausar";
    pause.disabled = false;
    audio.disabled = false;
    haltAudio();
    status.textContent = game.name;
    paint();
    canvas.focus({ preventScroll: true });
  } catch (error) {
    if (request === loadRequest) status.textContent = `Erro ao abrir ROM. ${error.message}`;
  } finally {
    if (request === loadRequest) romInput.value = "";
  }
});

pause.addEventListener("click", () => {
  paused = !paused;
  pause.textContent = paused ? "Continuar" : "Pausar";
  status.textContent = paused ? `${game.name} · pausado` : game.name;
  if (paused) {
    save();
    haltAudio();
  }
});

audio.addEventListener("click", async () => {
  if (audio.getAttribute("aria-pressed") === "true") {
    audio.setAttribute("aria-pressed", "false");
    haltAudio();
    return;
  }
  try {
    sound ??= new AudioContext();
    await sound.resume();
    audio.setAttribute("aria-pressed", "true");
  } catch {
    status.textContent = "O navegador não conseguiu ativar o som.";
  }
});

window.addEventListener("keydown", event => {
  if (!game || !keyMap.has(event.code) ||
      (event.code === "Enter" && event.target.closest("button,input"))) return;
  keyboard.add(event.code);
  event.preventDefault();
});
window.addEventListener("keyup", event => keyboard.delete(event.code));
function releaseInputs() {
  keyboard.clear();
  pointers.clear();
  for (const button of document.querySelectorAll("[data-active]")) delete button.dataset.active;
}
window.addEventListener("blur", releaseInputs);
window.addEventListener("pagehide", save);
document.addEventListener("visibilitychange", () => {
  if (document.hidden) {
    releaseInputs();
    haltAudio();
    save();
  }
  previousTime = undefined;
});
for (const button of document.querySelectorAll("[data-key]")) {
  button.addEventListener("pointerdown", event => {
    event.preventDefault();
    button.setPointerCapture(event.pointerId);
    pointers.set(event.pointerId, Number(button.dataset.key));
    button.dataset.active = "";
  });
  for (const type of ["pointerup", "pointercancel", "lostpointercapture"]) {
    button.addEventListener(type, event => {
      pointers.delete(event.pointerId);
      if (![...pointers.values()].includes(Number(button.dataset.key))) delete button.dataset.active;
    });
  }
}

function animate(now) {
  const dt = previousTime === undefined ? 0 : Math.min(now - previousTime, 30);
  previousTime = now;
  if (game && !paused && !crashed && !document.hidden) {
    balance += dt * core.dmg_clock() / 1000;
    if (balance >= 4) {
      const elapsed = core.dmg_run(Math.floor(balance), keys());
      if (elapsed < 0) {
        crashed = true;
        pause.disabled = true;
        status.textContent = `${game.name} · CPU travada por opcode inválido`;
        haltAudio();
      } else {
        balance -= elapsed;
        paint();
        drainAudio();
        if (now - lastSaveTime > 1500) {
          lastSaveTime = now;
          save();
        }
      }
    }
  }
  requestAnimationFrame(animate);
}
requestAnimationFrame(animate);
