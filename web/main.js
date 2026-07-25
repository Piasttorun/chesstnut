const { invoke } = window.__TAURI__.core;

const FILES = ["a", "b", "c", "d", "e", "f", "g", "h"];
const PIECE_LETTERS = {
  pawn: "P",
  knight: "N",
  bishop: "B",
  rook: "R",
  queen: "Q",
  king: "K",
};

const PROMOTION_CHOICES = ["queen", "rook", "bishop", "knight"];

const PGN_COLLAPSED_ROWS = 10;

const TIME_PRESETS = [
  { label: "1+0", name: "Bullet", minutes: 1, incrementSeconds: 0 },
  { label: "2+1", name: "Bullet", minutes: 2, incrementSeconds: 1 },
  { label: "3+0", name: "Blitz", minutes: 3, incrementSeconds: 0 },
  { label: "3+2", name: "Blitz", minutes: 3, incrementSeconds: 2 },
  { label: "5+0", name: "Blitz", minutes: 5, incrementSeconds: 0 },
  { label: "5+3", name: "Blitz", minutes: 5, incrementSeconds: 3 },
  { label: "10+0", name: "Rapid", minutes: 10, incrementSeconds: 0 },
  { label: "10+5", name: "Rapid", minutes: 10, incrementSeconds: 5 },
  { label: "15+10", name: "Rapid", minutes: 15, incrementSeconds: 10 },
  { label: "30+0", name: "Classical", minutes: 30, incrementSeconds: 0 },
  { label: "30+20", name: "Classical", minutes: 30, incrementSeconds: 20 },
];

// Polling (rather than a client-side countdown timer) keeps the displayed
// clock and the flagging check both driven by the same source of truth —
// Game::remaining_ms on the Rust side, computed from real elapsed time —
// instead of two independently-drifting clocks.
const CLOCK_POLL_INTERVAL_MS = 250;
const CLOCK_LOW_THRESHOLD_MS = 10_000;

const GAME_OVER_STATUSES = new Set([
  "checkmate",
  "stalemate",
  "draw_fifty_move",
  "draw_repetition",
  "resignation",
  "timeout",
]);

// ---------- Settings ----------
//
// Small and deliberately flat — adding a new preference later is a new
// default here, a row in the panel (index.html), and a read of
// settings.<name> wherever it's needed, not a restructure.
const SETTINGS_STORAGE_KEY = "chesstnut-settings";

function loadSettings() {
  const defaults = { interactionMode: "drag", muted: false };
  try {
    return { ...defaults, ...JSON.parse(localStorage.getItem(SETTINGS_STORAGE_KEY)) };
  } catch {
    return defaults;
  }
}

let settings = loadSettings();

function saveSettings() {
  localStorage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify(settings));
}

function renderSettingsPanel() {
  document.querySelectorAll(".settings-choice-option").forEach((button) => {
    button.classList.toggle("active", button.dataset.mode === settings.interactionMode);
  });
  document.getElementById("settings-mute").checked = settings.muted;
}

let selectedSquare = null;
let legalTargets = [];
let pgnExpanded = false;
let clockPollHandle = null;

// Non-null for the duration of a pick-up-and-drag gesture:
// { fromSquare, ghost, startX, startY, hoveredSquare }. render() (below)
// checks this and skips drawing a piece at fromSquare entirely — the
// dragged piece is represented solely by `ghost`, a cloned <img> that
// follows the pointer — so there's never a moment where both the ghost and
// the "real" piece are visible at once.
let dragState = null;

// The from/to squares of the most recently completed move, or null. Drives
// the last-move highlight and the arriving piece's slide-in animation.
// Cleared whenever the position changes for a reason other than a move
// (new game, FEN/PGN load) so a stale highlight never points at squares
// that no longer relate to the current position.
let lastMove = null;

// Whether the slide-in animation for lastMove has already played. render()
// rebuilds every square's DOM node from scratch on every call — including
// renders that aren't a new move at all, like clicking to select a piece —
// and lastMove deliberately stays put across those so the highlight
// persists. Without this flag, every one of those renders would re-add the
// animation class and replay the previous move's slide from scratch.
let lastMoveAnimated = false;

// Board squares are 4.5rem (see .board-wrapper's grid-template-columns/rows
// in style.css) — kept in sync manually since CSS custom properties aren't
// read back into JS.
const SQUARE_SIZE_REM = 4.5;

function fileOf(squareName) {
  return FILES.indexOf(squareName[0]);
}

function rankOf(squareName) {
  return Number(squareName[1]) - 1;
}

function findKingSquare(view, color) {
  for (let index = 0; index < 64; index++) {
    const piece = view.board[index];
    if (piece && piece.kind === "king" && piece.color === color) {
      const rank = Math.floor(index / 8);
      const file = index % 8;
      return `${FILES[file]}${rank + 1}`;
    }
  }
  return null;
}

// ---------- Move/game sounds ----------
//
// Two different live Web Audio API approaches (a fresh oscillator per tone,
// then a small pool of persistent oscillators with rescheduled frequency/
// gain) both degraded over the course of a game under WSLg specifically —
// pops, muffling, eventually going silent — in different ways each time.
// That pattern points at instability in WSLg's audio bridge itself rather
// than either approach's oscillator lifecycle, so this sidesteps live
// synthesis entirely: each sound is rendered to a short PCM WAV exactly
// once (synthesizeWav below), and played back as an ordinary <audio> clip —
// the same "decode once, play a file" path any web page's sound effects
// use, rather than driving a live audio graph move after move.
const AUDIO_SAMPLE_RATE = 22050;

// note: { frequency, duration, type, startTime, gain }. Additive: several
// notes overlapping in time (e.g. the game-over chord) just sum into the
// same buffer, then get clamped to [-1, 1] once at the end.
function synthesizeWav(notes) {
  const totalDuration = Math.max(...notes.map((n) => n.startTime + n.duration)) + 0.05;
  const sampleCount = Math.ceil(totalDuration * AUDIO_SAMPLE_RATE);
  const samples = new Float32Array(sampleCount);

  for (const note of notes) {
    const startSample = Math.floor(note.startTime * AUDIO_SAMPLE_RATE);
    const noteSamples = Math.floor(note.duration * AUDIO_SAMPLE_RATE);
    const attackSamples = Math.max(1, Math.floor(0.008 * AUDIO_SAMPLE_RATE));

    for (let i = 0; i < noteSamples && startSample + i < sampleCount; i++) {
      const t = i / AUDIO_SAMPLE_RATE;
      const phase = 2 * Math.PI * note.frequency * t;
      let wave;
      if (note.type === "square") {
        wave = Math.sign(Math.sin(phase));
      } else if (note.type === "triangle") {
        wave = (2 / Math.PI) * Math.asin(Math.sin(phase));
      } else {
        wave = Math.sin(phase);
      }
      const envelope =
        i < attackSamples ? i / attackSamples : Math.exp((-4 * (i - attackSamples)) / noteSamples);
      samples[startSample + i] += wave * envelope * note.gain;
    }
  }

  return encodeWav(samples, AUDIO_SAMPLE_RATE);
}

function encodeWav(samples, sampleRate) {
  const buffer = new ArrayBuffer(44 + samples.length * 2);
  const view = new DataView(buffer);

  const writeString = (offset, text) => {
    for (let i = 0; i < text.length; i++) view.setUint8(offset + i, text.charCodeAt(i));
  };

  writeString(0, "RIFF");
  view.setUint32(4, 36 + samples.length * 2, true);
  writeString(8, "WAVE");
  writeString(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true); // PCM
  view.setUint16(22, 1, true); // mono
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * 2, true); // byte rate (mono, 16-bit)
  view.setUint16(32, 2, true); // block align
  view.setUint16(34, 16, true); // bits per sample
  writeString(36, "data");
  view.setUint32(40, samples.length * 2, true);

  let offset = 44;
  for (let i = 0; i < samples.length; i++, offset += 2) {
    const clamped = Math.max(-1, Math.min(1, samples[i]));
    view.setInt16(offset, clamped < 0 ? clamped * 0x8000 : clamped * 0x7fff, true);
  }

  return URL.createObjectURL(new Blob([buffer], { type: "audio/wav" }));
}

// Rendered lazily (on first sound, not at page load) and cached — same
// deal as the old AudioContext: fine to compute anytime, but Blob/URL
// creation may as well happen on demand rather than up front.
let soundUrls = null;

function getSoundUrls() {
  if (!soundUrls) {
    soundUrls = {
      move: synthesizeWav([{ frequency: 520, duration: 0.09, type: "triangle", startTime: 0, gain: 0.55 }]),
      capture: synthesizeWav([
        { frequency: 260, duration: 0.11, type: "square", startTime: 0, gain: 0.45 },
        { frequency: 150, duration: 0.16, type: "square", startTime: 0.02, gain: 0.4 },
      ]),
      check: synthesizeWav([
        { frequency: 880, duration: 0.1, type: "sine", startTime: 0, gain: 0.5 },
        { frequency: 660, duration: 0.16, type: "sine", startTime: 0.09, gain: 0.45 },
      ]),
      gameOver: synthesizeWav(
        [523.25, 415.3, 329.63].map((frequency, index) => ({
          frequency,
          duration: 0.3,
          type: "sine",
          startTime: index * 0.12,
          gain: 0.4,
        }))
      ),
    };
  }
  return soundUrls;
}

// A fresh Audio object per play, rather than one reused element — cheap
// (it's decoding a few kilobytes), and means two sounds that happen to
// land close together (a capture immediately followed by check) just play
// as two independent clips instead of one interrupting the other.
function playClip(url) {
  if (settings.muted) return;
  const audio = new Audio(url);
  audio.play().catch((err) => console.error("sound playback failed:", err));
}

function playMoveSound() {
  playClip(getSoundUrls().move);
}

function playCaptureSound() {
  playClip(getSoundUrls().capture);
}

function playCheckSound() {
  playClip(getSoundUrls().check);
}

function playGameOverSound() {
  playClip(getSoundUrls().gameOver);
}

// Tracks the status seen on the previous render() so check/game-over sounds
// fire exactly once per transition, including ones that arrive through
// clock polling (a flag falling) rather than a move the player just made.
let previousStatus = null;

function playSoundForTransition(view, moveContext) {
  if (GAME_OVER_STATUSES.has(view.status)) {
    if (previousStatus !== view.status) playGameOverSound();
  } else if (view.status === "check") {
    if (previousStatus !== "check") playCheckSound();
  } else if (moveContext === "capture") {
    playCaptureSound();
  } else if (moveContext === "move") {
    playMoveSound();
  }
  previousStatus = view.status;
}

// ---------- Sidebar / pages ----------
//
// Every page currently renders identically to the main game view (see
// renderPageContent's switch below) — the point of routing through a page
// id at all, rather than just having the one view, is so a page can later
// diverge (e.g. an Analysis page that swaps the play controls for engine
// lines) by adding a branch here instead of restructuring the app.
const PAGES = [
  { id: "play", label: "Play" },
  { id: "analysis", label: "Analysis" },
];
let activePage = "play";

function renderSidebar() {
  const nav = document.getElementById("sidebar");
  nav.innerHTML = "";
  for (const page of PAGES) {
    const button = document.createElement("button");
    button.className = "sidebar-item" + (page.id === activePage ? " active" : "");
    const marker = document.createElement("span");
    marker.className = "sidebar-item-marker";
    button.append(marker, document.createTextNode(page.label));
    button.addEventListener("click", () => setActivePage(page.id));
    nav.appendChild(button);
  }
}

function updatePageHeading() {
  const page = PAGES.find((p) => p.id === activePage);
  document.getElementById("page-heading-subtitle").textContent = page.label;
}

function setActivePage(pageId) {
  if (pageId === activePage) return;
  activePage = pageId;
  renderSidebar();
  updatePageHeading();
  switch (activePage) {
    case "play":
    case "analysis":
    default:
      if (currentView) render(currentView);
      break;
  }
}

function pieceSpriteSrc(piece) {
  const colorLetter = piece.color === "white" ? "w" : "b";
  return `pieces/${colorLetter}${PIECE_LETTERS[piece.kind]}.svg`;
}

// view.board is a flat 64-entry array, index = rank*8 + file — matching
// engine/board.rs's Square::to_index, so no reshuffling is needed here.
function pieceAt(board, squareName) {
  const file = FILES.indexOf(squareName[0]);
  const rank = Number(squareName[1]) - 1;
  return board[rank * 8 + file];
}

function isPromotionMove(view, from, to) {
  const piece = pieceAt(view.board, from);
  if (!piece || piece.kind !== "pawn") return false;
  const toRank = Number(to[1]);
  return toRank === 1 || toRank === 8;
}

// Resolves once the player clicks a piece in the picker. The promise stays
// pending — and the click handling below just awaits it — for as long as
// the picker is on screen; there's no timeout, the player decides when.
function askPromotionChoice(color) {
  return new Promise((resolve) => {
    const picker = document.getElementById("promotion-picker");
    picker.innerHTML = "";

    for (const kind of PROMOTION_CHOICES) {
      const button = document.createElement("button");
      const img = document.createElement("img");
      img.src = pieceSpriteSrc({ color, kind });
      img.alt = kind;
      button.appendChild(img);
      button.addEventListener("click", () => {
        picker.classList.add("hidden");
        resolve(kind);
      });
      picker.appendChild(button);
    }

    picker.classList.remove("hidden");
  });
}

function statusText(view) {
  const turn = view.turn === "white" ? "White" : "Black";
  const other = view.turn === "white" ? "Black" : "White";
  switch (view.status) {
    case "checkmate":
      return `Checkmate — ${other} wins`;
    case "timeout":
      return `Time's up — ${other} wins on time`;
    case "stalemate":
      return "Stalemate — draw";
    case "draw_fifty_move":
      return "Draw — 50-move rule";
    case "draw_repetition":
      return "Draw — threefold repetition";
    case "resignation":
      return `${turn} resigned — ${other} wins`;
    case "check":
      return `${turn} to move — check!`;
    default:
      return `${turn} to move`;
  }
}

// Same result-classification logic as statusText() above, but phrased for
// the game-over popup: a short title plus a one-line detail, rather than
// one combined sentence.
function gameOverText(view) {
  const turn = view.turn === "white" ? "White" : "Black";
  const other = view.turn === "white" ? "Black" : "White";
  switch (view.status) {
    case "checkmate":
      return { title: "Checkmate", detail: `${other} wins` };
    case "resignation":
      return { title: "Resignation", detail: `${turn} resigned — ${other} wins` };
    case "timeout":
      return { title: "Time's up", detail: `${other} wins on time` };
    case "stalemate":
      return { title: "Draw", detail: "Stalemate" };
    case "draw_fifty_move":
      return { title: "Draw", detail: "50-move rule" };
    case "draw_repetition":
      return { title: "Draw", detail: "Threefold repetition" };
    default:
      return null;
  }
}

function formatClock(ms) {
  const totalSeconds = Math.max(0, Math.ceil(ms / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}

function renderClocks(view) {
  const whiteEl = document.getElementById("white-clock");
  const blackEl = document.getElementById("black-clock");

  if (!view.clock) {
    whiteEl.classList.add("clock-hidden");
    blackEl.classList.add("clock-hidden");
    return;
  }

  whiteEl.classList.remove("clock-hidden");
  blackEl.classList.remove("clock-hidden");
  whiteEl.textContent = formatClock(view.clock.whiteMs);
  blackEl.textContent = formatClock(view.clock.blackMs);
  whiteEl.classList.toggle("clock-active", view.turn === "white");
  blackEl.classList.toggle("clock-active", view.turn === "black");
  whiteEl.classList.toggle("clock-low", view.clock.whiteMs < CLOCK_LOW_THRESHOLD_MS);
  blackEl.classList.toggle("clock-low", view.clock.blackMs < CLOCK_LOW_THRESHOLD_MS);
}

// Most games live within a pawn or so of material, so that's where the bar
// should be most readable — tanh gives a curve that's close to linear (and
// steep) near zero, then flattens out fast so lopsided positions don't
// pin to one extreme the moment someone's up a queen. Calibrated so a
// 1-pawn (100cp) edge moves the bar exactly a third of the way from center
// toward the (unreached) edge: solving tanh(100 / S) = 1/3 for S.
const EVAL_BAR_SENSITIVITY_CP = 100 / Math.atanh(1 / 3);
// However lopsided the material is, the trailing side keeps at least this
// much of the bar — full 0%/100% is reserved for an actual checkmate/forced
// mate below, not just a big material swing.
const EVAL_BAR_FLOOR_PERCENT = 10;

// The last completed analyze() result, tagged with the exact position/depth
// it's for: { fen, depth, kind: "centipawns"|"mateIn", value } | null. Kept
// across re-renders of the *same* position (selecting/deselecting a piece
// re-renders the board but doesn't change view.fen) rather than cleared
// every render — clearing it unconditionally was what made every single
// click, move or not, kick off a brand new depth-N search below, which is
// what was delaying legal-move highlighting and move input generally.
let lastAnalysis = null;

// Only treat lastAnalysis as showable if it's actually for the position on
// screen right now — once the position moves on, a stale result for the
// old one must fall back to the cheap material score, not linger.
function currentAnalysis(view) {
  return lastAnalysis && lastAnalysis.fen === view.fen ? lastAnalysis : null;
}

function whitePercentFor(view) {
  if (view.status === "checkmate") {
    // The side to move has no moves and just got mated — this, and a
    // forced mate the search itself found (below), are the only cases
    // that empty the bar completely.
    return view.turn === "white" ? 0 : 100;
  }
  const analysis = currentAnalysis(view);
  if (analysis && analysis.kind === "mateIn") {
    return analysis.value > 0 ? 100 : 0;
  }
  const score = analysis ? analysis.value : view.score;
  const t = Math.tanh(score / EVAL_BAR_SENSITIVITY_CP); // -1..1
  return 50 + t * (50 - EVAL_BAR_FLOOR_PERCENT);
}

function evalBarText(view) {
  const analysis = currentAnalysis(view);
  if (analysis && analysis.kind === "mateIn") {
    return `M${analysis.value}`;
  }
  const score = analysis ? analysis.value : view.score;
  const pawns = (score / 100).toFixed(1);
  return score >= 0 ? `+${pawns}` : pawns;
}

function paintEvalBar(view) {
  const whitePercent = whitePercentFor(view);
  document.getElementById("eval-bar-white").style.height = `${whitePercent}%`;
  document.getElementById("eval-bar-black").style.height = `${100 - whitePercent}%`;
  document.getElementById("eval-bar-text").textContent = evalBarText(view);
  const analysis = currentAnalysis(view);
  document.getElementById("eval-bar").classList.toggle(
    "mate-found",
    view.status === "checkmate" || (analysis && analysis.kind === "mateIn")
  );
}

// FEN+depth of the most recently *requested* analysis, whether or not it
// has resolved yet — separate from lastAnalysis so that re-rendering the
// same position (e.g. a piece-selection click) while a search is still
// running doesn't fire a second, overlapping one for no reason.
let requestedAnalysisKey = null;

// A depth-N search is far more expensive than everything else the app
// does, so it's a separate command the frontend calls only when the
// position (or the requested depth) actually changes — not on every
// render, and not folded into the constantly-polled game state.
async function updateAnalysis(view, depth) {
  let result;
  try {
    result = await invoke("analyze", { depth });
  } catch (err) {
    console.error("analyze failed:", err);
    return; // leave the bar showing whatever it had before
  }
  // The position may have moved on again by the time a deep search
  // finishes — only paint a result that's still for the current position.
  // Compared by FEN, not object identity: the clock-poll timer (see
  // startClockPollingIfNeeded) reassigns currentView to a freshly deserialized
  // object every 250ms even when the position hasn't changed at all, so an
  // identity check here was discarding a perfectly valid result — and thus
  // the eval bar just not updating — any time a search outlived one tick,
  // which happens routinely once a clock is running.
  if (currentView.fen !== view.fen) return;
  lastAnalysis = { fen: view.fen, depth, ...result };
  paintEvalBar(currentView);
}

function renderEvalBar(view) {
  const column = document.getElementById("eval-bar-column");
  const showEvalBar = document.getElementById("eval-bar-checkbox").checked;
  column.classList.toggle("hidden", !showEvalBar);
  if (!showEvalBar) return;

  // Paint instantly with the cheap material score (or a still-valid
  // analysis already on hand for this exact position), then kick off the
  // real search in the background — but only if this position/depth
  // hasn't already been requested, so plain re-renders of an unchanged
  // position (selecting a piece, switching sidebar pages, ...) never
  // start a redundant search.
  paintEvalBar(view);
  const depth = Number(document.getElementById("eval-bar-depth").value);
  const key = `${view.fen}|${depth}`;
  if (key === requestedAnalysisKey) return;
  requestedAnalysisKey = key;
  // Deferred to the next event-loop tick rather than called inline here —
  // the move/board update for this render must get scheduled and painted
  // on its own, never bundled into the same synchronous burst as kicking
  // off an analyze() request. The eval bar is allowed to visibly catch up
  // a beat later; the piece appearing where it was dropped is not.
  setTimeout(() => updateAnalysis(view, depth), 0);
}

function stopClockPolling() {
  if (clockPollHandle !== null) {
    clearInterval(clockPollHandle);
    clockPollHandle = null;
  }
}

// A clock only needs live polling while it's actually running: there's a
// time control in play, and the game hasn't already ended. Re-armed on
// every render() so a finished game (or a freshly untimed one) stops
// polling instead of ticking forever in the background.
function startClockPollingIfNeeded(view) {
  stopClockPolling();
  if (!view.clock || GAME_OVER_STATUSES.has(view.status)) return;

  clockPollHandle = setInterval(async () => {
    const latest = await invoke("get_state");
    if (GAME_OVER_STATUSES.has(latest.status)) {
      // The clock just ran out between ticks — do a full render so the
      // game-over modal shows and polling stops.
      render(latest);
      return;
    }
    // Otherwise only the clock numbers moved. Rebuilding the whole board
    // (and re-binding all 64 click listeners) four times a second made
    // clicks feel dropped/laggy, so a normal tick only touches the clock
    // display instead of calling the full render().
    currentView = latest;
    renderClocks(latest);
  }, CLOCK_POLL_INTERVAL_MS);
}

function showGameOverModal(view) {
  const info = gameOverText(view);
  if (!info) return;
  document.getElementById("game-over-title").textContent = info.title;
  document.getElementById("game-over-detail").textContent = info.detail;
  document.getElementById("game-over-overlay").classList.remove("hidden");
}

function hideGameOverModal() {
  document.getElementById("game-over-overlay").classList.add("hidden");
}

function renderClockPicker() {
  const container = document.getElementById("clock-presets");
  container.innerHTML = "";

  for (const preset of TIME_PRESETS) {
    const button = document.createElement("button");
    button.className = "clock-preset";

    const label = document.createElement("div");
    label.className = "clock-preset-label";
    label.textContent = preset.label;

    const name = document.createElement("div");
    name.className = "clock-preset-name";
    name.textContent = preset.name;

    button.append(label, name);
    button.addEventListener("click", () =>
      chooseTimeControl(preset.minutes * 60_000, preset.incrementSeconds * 1000)
    );
    container.appendChild(button);
  }

  const noClock = document.createElement("button");
  noClock.className = "clock-preset clock-preset-none";
  noClock.textContent = "No clock";
  noClock.addEventListener("click", () => chooseTimeControl(null, 0));
  container.appendChild(noClock);
}

async function chooseTimeControl(initialMs, incrementMs) {
  render(await invoke("select_time_control", { initialMs, incrementMs }));
}

// Rank/file labels never change, so they're built once up front — only the
// 64 squares inside #board get rebuilt on every state update.
function renderLabels() {
  const wrapper = document.querySelector(".board-wrapper");

  for (let rank = 7; rank >= 0; rank--) {
    const label = document.createElement("div");
    label.className = "rank-label";
    label.textContent = rank + 1;
    label.style.gridRow = 8 - rank;
    label.style.gridColumn = 1;
    wrapper.appendChild(label);
  }

  const corner = document.createElement("div");
  corner.style.gridRow = 9;
  corner.style.gridColumn = 1;
  wrapper.appendChild(corner);

  FILES.forEach((file, index) => {
    const label = document.createElement("div");
    label.className = "file-label";
    label.textContent = file;
    label.style.gridRow = 9;
    label.style.gridColumn = index + 2;
    wrapper.appendChild(label);
  });
}

// moveHistory is a flat list of SAN strings in play order (White's move,
// then Black's, alternating) — this groups them into the classic
// move-number / White / Black table layout, showing only the last
// PGN_COLLAPSED_ROWS rows unless pgnExpanded is set.
function renderPgn(view) {
  const table = document.getElementById("pgn-table");
  const toggle = document.getElementById("pgn-toggle");
  table.innerHTML = "";

  const moves = view.moveHistory;
  const totalRows = Math.ceil(moves.length / 2);
  const isTruncated = !pgnExpanded && totalRows > PGN_COLLAPSED_ROWS;
  const rowsToShow = isTruncated ? PGN_COLLAPSED_ROWS : totalRows;
  const startRow = totalRows - rowsToShow;

  toggle.classList.toggle("hidden", !isTruncated);
  toggle.textContent = `··· show ${totalRows - PGN_COLLAPSED_ROWS} earlier move${totalRows - PGN_COLLAPSED_ROWS === 1 ? "" : "s"}`;

  for (let row = startRow; row < totalRows; row++) {
    const tr = document.createElement("tr");

    const numberCell = document.createElement("td");
    numberCell.className = "pgn-move-number";
    numberCell.textContent = `${row + 1}.`;

    const whiteCell = document.createElement("td");
    whiteCell.textContent = moves[row * 2] ?? "";

    const blackCell = document.createElement("td");
    blackCell.textContent = moves[row * 2 + 1] ?? "";

    tr.append(numberCell, whiteCell, blackCell);
    table.appendChild(tr);
  }
}

function renderFen(view) {
  document.getElementById("fen-text").value = view.fen;
}

let currentView = null;

// moveContext is "move"/"capture" when this render follows a move the
// player just made, or omitted for renders that don't represent a new move
// (selecting a piece, switching sidebar pages, a clock tick, ...) — it's
// only used to pick a sound, not to decide what to draw.
function render(view, moveContext) {
  currentView = view;
  // lastAnalysis is intentionally NOT cleared here — currentAnalysis(view)
  // (above) already treats it as stale once view.fen no longer matches, and
  // actually clearing it on every render (including a plain selection
  // click that doesn't change the position) used to wipe out a perfectly
  // valid analysis and flash the bar back to the material-only score.
  const board = document.getElementById("board");
  board.innerHTML = "";

  const kingInCheckSquare =
    view.status === "check" || view.status === "checkmate" ? findKingSquare(view, view.turn) : null;

  for (let rank = 7; rank >= 0; rank--) {
    for (let file = 0; file < 8; file++) {
      const squareName = `${FILES[file]}${rank + 1}`;
      const square = document.createElement("div");
      const isLight = (file + rank) % 2 === 1;
      const classes = ["square", isLight ? "light" : "dark"];
      if (squareName === selectedSquare) classes.push("selected");
      if (legalTargets.includes(squareName)) classes.push("legal-target");
      if (lastMove && (squareName === lastMove.from || squareName === lastMove.to)) classes.push("last-move");
      if (squareName === kingInCheckSquare) classes.push("king-in-check");
      square.className = classes.join(" ");
      square.dataset.square = squareName;
      square.addEventListener("mousedown", (event) => handlePointerDown(event, squareName, view));

      const piece = pieceAt(view.board, squareName);
      const isDragSource = dragState !== null && squareName === dragState.fromSquare;
      if (piece && !isDragSource) {
        const img = document.createElement("img");
        img.src = pieceSpriteSrc(piece);
        img.alt = `${piece.color} ${piece.kind}`;
        img.className = "piece-sprite";
        if (lastMove && squareName === lastMove.to && !lastMoveAnimated) {
          // Slide-in trick: start the piece offset by exactly the from→to
          // delta (in square units) via inline custom properties, then
          // piece-slide (style.css) animates that offset back to zero. The
          // element itself is freshly created each render — there's no DOM
          // node to actually move — so this fakes the same visual result.
          // Gated on lastMoveAnimated so this only plays once, the render
          // right after the move — without it, every later render (e.g.
          // clicking to select a different piece) would replay it, which
          // looked like a glitchy flash of the previous move re-happening.
          const dx = (fileOf(lastMove.from) - file) * SQUARE_SIZE_REM;
          const dy = (rank - rankOf(lastMove.from)) * SQUARE_SIZE_REM;
          img.style.setProperty("--dx", `${dx}rem`);
          img.style.setProperty("--dy", `${dy}rem`);
          img.classList.add("piece-arriving");
          lastMoveAnimated = true;
        }
        square.appendChild(img);
      }

      board.appendChild(square);
    }
  }

  document.getElementById("status").textContent = statusText(view);
  renderPgn(view);
  renderFen(view);
  renderClocks(view);
  renderEvalBar(view);
  playSoundForTransition(view, moveContext);

  document.getElementById("clock-picker-overlay").classList.toggle("hidden", !view.awaitingClockChoice);
  if (GAME_OVER_STATUSES.has(view.status)) {
    showGameOverModal(view);
  } else {
    hideGameOverModal();
  }
  startClockPollingIfNeeded(view);
}

// Shared by both interaction paths: clicking a second, already-highlighted
// square, and dropping a dragged piece onto one.
async function performMove(fromSquare, toSquare, view) {
  let promotion = null;
  if (isPromotionMove(view, fromSquare, toSquare)) {
    promotion = await askPromotionChoice(view.turn);
  }

  const wasCapture = !!pieceAt(view.board, toSquare);

  try {
    const nextView = await invoke("make_move", { from: fromSquare, to: toSquare, promotion });
    selectedSquare = null;
    legalTargets = [];
    lastMove = { from: fromSquare, to: toSquare };
    lastMoveAnimated = false;
    render(nextView, wasCapture ? "capture" : "move");
  } catch (err) {
    console.error("make_move failed:", err);
    selectedSquare = null;
    legalTargets = [];
    render(view);
  }
}

// Bumped on every call and captured locally so an out-of-order response
// (piece A's legal_moves arriving *after* piece B's, if A's response is
// slow enough to lose the race) never overwrites a newer selection — only
// the reply matching the latest call actually gets applied.
let selectionRequestId = 0;

// Selects a piece and fetches its legal moves. Deliberately not awaited by
// its caller in the drag path (see handlePointerDown) — the ghost has to
// appear the instant the pointer goes down, not once this IPC round-trip
// resolves, or dragging would inherit exactly the "beat" of latency this
// was built to get away from.
async function selectSquare(squareName, view) {
  selectedSquare = squareName;
  const requestId = ++selectionRequestId;
  const targets = await invoke("legal_moves", { square: squareName });
  if (requestId !== selectionRequestId) return; // superseded by a later selection
  legalTargets = targets;
  render(view);
}

function handlePointerDown(event, squareName, view) {
  if (event.button !== 0) return; // left button only

  if (selectedSquare && legalTargets.includes(squareName)) {
    performMove(selectedSquare, squareName, view);
    return;
  }

  const piece = pieceAt(view.board, squareName);
  if (piece && piece.color === view.turn) {
    selectSquare(squareName, view); // not awaited — see its own comment
    if (settings.interactionMode === "drag") {
      beginDrag(event, squareName);
    }
    return;
  }

  selectedSquare = null;
  legalTargets = [];
  render(view);
}

// ---------- Drag and drop ----------
//
// Layered on top of click-to-select-then-click-to-move (still fully intact
// above) rather than replacing it — picking a piece up starts a drag *and*
// selects it, so a plain click (mousedown+mouseup with no real movement)
// still works exactly as a click always did.
function beginDrag(event, fromSquare) {
  const img = event.currentTarget.querySelector(".piece-sprite");
  if (!img) return;

  const rect = img.getBoundingClientRect();
  const ghost = img.cloneNode(true);
  ghost.classList.add("drag-ghost");
  ghost.style.width = `${rect.width}px`;
  ghost.style.height = `${rect.height}px`;
  document.body.appendChild(ghost);

  dragState = {
    fromSquare,
    ghost,
    startX: event.clientX,
    startY: event.clientY,
    hoveredSquare: null,
    pendingX: event.clientX,
    pendingY: event.clientY,
    rafHandle: null,
  };
  positionGhost(event.clientX, event.clientY);

  document.addEventListener("mousemove", handleDragMove);
  document.addEventListener("mouseup", handleDragEnd);
}

function positionGhost(x, y) {
  dragState.ghost.style.left = `${x}px`;
  dragState.ghost.style.top = `${y}px`;
}

function setDropHover(squareName, isHovering) {
  if (!squareName) return;
  document.querySelector(`.square[data-square="${squareName}"]`)?.classList.toggle("drop-hover", isHovering);
}

// mousemove itself only records the latest pointer position (cheap) — the
// actual work (moving the ghost, hit-testing what's underneath it) happens
// at most once per animation frame in applyDragFrame. Doing that work
// directly in the mousemove handler was the cause of the ghost visibly
// lagging behind the real cursor: WSLg's software rendering can't paint
// fast enough to keep up with a full elementFromPoint hit-test plus style
// writes on every one of dozens of mousemove events a second, so updates
// piled up and were rendered late, one after another.
function handleDragMove(event) {
  dragState.pendingX = event.clientX;
  dragState.pendingY = event.clientY;
  if (dragState.rafHandle === null) {
    dragState.rafHandle = requestAnimationFrame(applyDragFrame);
  }
}

function applyDragFrame() {
  if (!dragState) return;
  dragState.rafHandle = null;
  const { pendingX: x, pendingY: y } = dragState;
  positionGhost(x, y);

  const targetEl = document.elementFromPoint(x, y);
  const squareEl = targetEl && targetEl.closest(".square");
  const squareName = squareEl ? squareEl.dataset.square : null;

  if (squareName !== dragState.hoveredSquare) {
    setDropHover(dragState.hoveredSquare, false);
    if (squareName && legalTargets.includes(squareName)) {
      setDropHover(squareName, true);
    }
    dragState.hoveredSquare = squareName;
  }
}

async function handleDragEnd(event) {
  const { fromSquare, ghost, hoveredSquare, rafHandle } = dragState;

  document.removeEventListener("mousemove", handleDragMove);
  document.removeEventListener("mouseup", handleDragEnd);
  if (rafHandle !== null) cancelAnimationFrame(rafHandle);
  ghost.remove();
  setDropHover(hoveredSquare, false);
  dragState = null;

  const droppedOnLegalTarget =
    hoveredSquare && hoveredSquare !== fromSquare && legalTargets.includes(hoveredSquare);

  if (droppedOnLegalTarget) {
    await performMove(fromSquare, hoveredSquare, currentView);
  } else {
    // Dropped somewhere illegal (or back on its own square) — nothing
    // moved, but render() was skipping the piece at fromSquare for as
    // long as dragState was set, so it needs one more render to bring it
    // back now that dragState is null again.
    render(currentView);
  }
}

async function startNewGame() {
  selectedSquare = null;
  legalTargets = [];
  pgnExpanded = false;
  lastMove = null;
  lastMoveAnimated = false;
  hideGameOverModal();
  render(await invoke("new_game"));
}

async function resign() {
  if (!confirm(`Are you sure ${currentView.turn === "white" ? "White" : "Black"} wants to resign?`)) {
    return;
  }
  render(await invoke("resign"));
}

async function copyToClipboard(text) {
  try {
    await navigator.clipboard.writeText(text);
  } catch (err) {
    console.error("clipboard copy failed:", err);
  }
}

async function loadFen() {
  const fen = document.getElementById("fen-text").value.trim();
  try {
    const view = await invoke("load_fen", { fen });
    selectedSquare = null;
    legalTargets = [];
    pgnExpanded = false;
    lastMove = null;
    lastMoveAnimated = false;
    render(view);
  } catch (err) {
    alert(`Couldn't load that FEN:\n${err}`);
  }
}

async function importPgn() {
  const pgn = window.prompt("Paste PGN to import:");
  if (!pgn) return; // cancelled or empty

  try {
    const view = await invoke("load_pgn", { pgn });
    selectedSquare = null;
    legalTargets = [];
    pgnExpanded = false;
    lastMove = null;
    lastMoveAnimated = false;
    render(view);
  } catch (err) {
    alert(`Couldn't import that PGN:\n${err}`);
  }
}

document.addEventListener("DOMContentLoaded", () => {
  renderSidebar();
  updatePageHeading();
  renderLabels();
  renderClockPicker();
  document.getElementById("new-game").addEventListener("click", startNewGame);
  document.getElementById("resign").addEventListener("click", resign);
  document.getElementById("game-over-close").addEventListener("click", startNewGame);
  document.getElementById("eval-bar-checkbox").addEventListener("change", () => renderEvalBar(currentView));
  document.getElementById("eval-bar-depth").addEventListener("change", () => renderEvalBar(currentView));

  // Clicking the toggle expands to full history; clicking anywhere else in
  // the panel while expanded collapses it back to the last
  // PGN_COLLAPSED_ROWS rows. stopPropagation on the toggle keeps that same
  // click from also bubbling up and immediately re-collapsing it.
  document.getElementById("pgn-toggle").addEventListener("click", (event) => {
    event.stopPropagation();
    pgnExpanded = true;
    renderPgn(currentView);
  });
  document.getElementById("pgn-panel").addEventListener("click", () => {
    if (pgnExpanded) {
      pgnExpanded = false;
      renderPgn(currentView);
    }
  });

  document.getElementById("copy-fen").addEventListener("click", () => {
    copyToClipboard(document.getElementById("fen-text").value);
  });
  document.getElementById("load-fen").addEventListener("click", loadFen);

  // Both header buttons live inside the click-to-collapse #pgn-panel, so
  // they need the same stopPropagation treatment as pgn-toggle above.
  document.getElementById("copy-pgn").addEventListener("click", (event) => {
    event.stopPropagation();
    copyToClipboard(currentView.pgn);
  });
  document.getElementById("import-pgn").addEventListener("click", (event) => {
    event.stopPropagation();
    importPgn();
  });

  renderSettingsPanel();
  document.getElementById("settings-toggle").addEventListener("click", (event) => {
    event.stopPropagation();
    document.getElementById("settings-panel").classList.toggle("hidden");
  });
  document.querySelectorAll(".settings-choice-option").forEach((button) => {
    button.addEventListener("click", () => {
      settings.interactionMode = button.dataset.mode;
      saveSettings();
      renderSettingsPanel();
    });
  });
  document.getElementById("settings-mute").addEventListener("change", (event) => {
    settings.muted = event.target.checked;
    saveSettings();
  });
  // Clicking anywhere outside the widget closes the panel, same pattern as
  // the PGN panel's expand/collapse above.
  document.addEventListener("click", (event) => {
    if (!document.getElementById("settings-widget").contains(event.target)) {
      document.getElementById("settings-panel").classList.add("hidden");
    }
  });

  invoke("get_state").then(render);
});
