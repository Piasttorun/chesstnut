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

// ---------- Game mode (human vs. human, or vs. the engine) ----------
//
// Chosen in the pre-game setup modal alongside the time control and left
// as-is across "New Game" (so it doesn't reset to defaults every game) —
// only opponent === "computer" is new behavior; opponent === "human" (the
// default) reproduces the original two-humans-at-one-board experience
// exactly, since every AI-related check below short-circuits on it.
let gameMode = { opponent: "human", humanColor: "white", computerDepth: 8 };

function renderSetupPickers() {
  document.querySelectorAll("#opponent-choice .settings-choice-option").forEach((button) => {
    button.classList.toggle("active", button.dataset.opponent === gameMode.opponent);
  });
  document.querySelectorAll("#color-choice .settings-choice-option").forEach((button) => {
    button.classList.toggle("active", button.dataset.color === gameMode.humanColor);
  });
  document.getElementById("color-choice-row").classList.toggle("hidden", gameMode.opponent !== "computer");
  document.getElementById("computer-depth-row").classList.toggle("hidden", gameMode.opponent !== "computer");
  document.getElementById("computer-depth").value = String(gameMode.computerDepth);
}

// The board (and clocks — see renderClocks) only actually flip when
// playing the engine as Black; two humans share one board from a single,
// fixed orientation the way this app always has.
function isBoardFlipped() {
  return gameMode.opponent === "computer" && gameMode.humanColor === "black";
}

function isHumanTurn(view) {
  return gameMode.opponent !== "computer" || view.turn === gameMode.humanColor;
}

// Analysis has no opponent concept at all — both sides are always the
// player's to move, unlike Play where isHumanTurn gates input during the
// engine's turn.
function isHumanTurnFor(page, view) {
  return page === "analysis" ? true : isHumanTurn(view);
}

// Maps a shared action ("make_move", "legal_moves", ...) to the backend
// command that actually implements it for the given page — Play's commands
// keep their original unprefixed names (existing callers/tests depend on
// that), Analysis's are the analysis_-prefixed ones added alongside its own
// independent Game state (see src-tauri/src/commands.rs).
function cmdName(page, base) {
  return page === "analysis" ? `analysis_${base}` : base;
}

function viewFor(page) {
  return page === "analysis" ? analysisView : currentView;
}

function flippedFor(page) {
  return page === "analysis" ? analysisFlipped : isBoardFlipped();
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

// Maps true chess coordinates to on-screen grid position (0-indexed
// col/row), accounting for the board flip — used for the piece-slide
// animation's offset math, which cares about actual screen direction, not
// chess coordinates. Unflipped: rank 7 (rank 8) is row 0 (top), file 0
// (the a-file) is col 0 (left) — matches render()'s default iteration
// order. Flipped: rank 0 (rank 1) is row 0 (top), file 7 (the h-file) is
// col 0 (left) — everything mirrored, matching render()'s flipped order.
function visualPosition(file, rank, flipped) {
  return {
    col: flipped ? 7 - file : file,
    row: flipped ? rank : 7 - rank,
  };
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
// Kept separate per page — Play and Analysis are independent games whose
// statuses shouldn't cross-suppress or cross-trigger each other's sounds.
let previousStatus = null;
let analysisPreviousStatus = null;

function playSoundForTransition(page, view, moveContext) {
  const previous = page === "analysis" ? analysisPreviousStatus : previousStatus;
  if (GAME_OVER_STATUSES.has(view.status)) {
    if (previous !== view.status) playGameOverSound();
  } else if (view.status === "check") {
    if (previous !== "check") playCheckSound();
  } else if (moveContext === "capture") {
    playCaptureSound();
  } else if (moveContext === "move") {
    playMoveSound();
  }
  if (page === "analysis") analysisPreviousStatus = view.status;
  else previousStatus = view.status;
}

// ---------- Sidebar / pages ----------
//
// Three pages: "home" is a placeholder landing tab (nothing on it yet
// beyond the sidebar/banner chrome that's already global to every page) and
// is where the app opens by default, "play" is the original board — a real
// game against another human or the engine, with a clock and an opponent to
// resign against — and "analysis" is a free sandbox board (its own
// independent Game on the backend, see AnalysisGame in
// src-tauri/src/commands.rs) with no clock, no opponent, and the eval bar
// always on.
const PAGES = [
  { id: "home", label: "Home" },
  { id: "play", label: "Play" },
  { id: "analysis", label: "Analysis" },
];
let activePage = "home";

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

// Shows/hides the board area entirely (Home has none of it) and picks
// which page's action buttons and Import button are visible — separate
// from paint() itself since it only needs to run when the active page
// actually changes, not on every re-render of whichever page is showing.
function updatePageVisibility() {
  const showBoard = activePage === "play" || activePage === "analysis";
  document.getElementById("game-layout").classList.toggle("hidden", !showBoard);
  document.getElementById("fen-box").classList.toggle("hidden", !showBoard);
  document.getElementById("play-buttons").classList.toggle("hidden", activePage !== "play");
  document.getElementById("analysis-buttons").classList.toggle("hidden", activePage !== "analysis");
  document.getElementById("import-pgn").classList.toggle("hidden", activePage !== "analysis");
}

function setActivePage(pageId) {
  if (pageId === activePage) return;
  activePage = pageId;
  renderSidebar();
  updatePageHeading();
  updatePageVisibility();
  if (pageId === "play") {
    // Re-fetched rather than repainted from a possibly-stale cached
    // currentView — Play's clock keeps running server-side the whole time
    // this tab isn't visible (clock polling only runs while it's the
    // active page, see startClockPollingIfNeeded), so the cached view can
    // be seconds or minutes out of date by the time the player switches
    // back to it.
    invoke("get_state").then((view) => paint("play", view));
  } else if (pageId === "analysis" && analysisView) {
    // Nothing outside this tab can change the Analysis position, so the
    // cached view is never stale — no need to re-fetch.
    paint("analysis", analysisView);
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

function statusText(page, view) {
  const turn = view.turn === "white" ? "White" : "Black";
  const other = view.turn === "white" ? "Black" : "White";
  if (
    page === "play" &&
    gameMode.opponent === "computer" &&
    !view.awaitingClockChoice &&
    !isHumanTurn(view) &&
    !GAME_OVER_STATUSES.has(view.status)
  ) {
    // aiThinkingStartedAt is set the instant requestAiMove actually starts
    // (see there) — a deep search can now legitimately take tens of
    // seconds since a slow position no longer looks indistinguishable from
    // a stuck one, this ticking count is the difference.
    const elapsedSeconds = aiThinkingStartedAt === null ? 0 : Math.floor((Date.now() - aiThinkingStartedAt) / 1000);
    return `Computer is thinking… (${elapsedSeconds}s)`;
  }
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

// #black-clock and #white-clock are fixed DOM positions (top and bottom of
// the board respectively, per index.html's layout) — the IDs are legacy
// names from before the board could flip, not necessarily "black's clock"
// and "white's clock" anymore. Which color's time actually goes in which
// slot depends on orientation, same as the board squares themselves: when
// flipped, the human's own color sits at the bottom, so its clock does too.
function renderClocks(view) {
  const topEl = document.getElementById("black-clock");
  const bottomEl = document.getElementById("white-clock");

  if (!view.clock) {
    topEl.classList.add("clock-hidden");
    bottomEl.classList.add("clock-hidden");
    return;
  }

  const flipped = isBoardFlipped();
  const topColor = flipped ? "white" : "black";
  const bottomColor = flipped ? "black" : "white";
  const topMs = topColor === "white" ? view.clock.whiteMs : view.clock.blackMs;
  const bottomMs = bottomColor === "white" ? view.clock.whiteMs : view.clock.blackMs;

  topEl.classList.remove("clock-hidden");
  bottomEl.classList.remove("clock-hidden");
  topEl.textContent = formatClock(topMs);
  bottomEl.textContent = formatClock(bottomMs);
  topEl.classList.toggle("clock-active", view.turn === topColor);
  bottomEl.classList.toggle("clock-active", view.turn === bottomColor);
  topEl.classList.toggle("clock-low", topMs < CLOCK_LOW_THRESHOLD_MS);
  bottomEl.classList.toggle("clock-low", bottomMs < CLOCK_LOW_THRESHOLD_MS);
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
// old one must fall back to the cheap material score, not linger. Keyed by
// page too, not just fen: Play and Analysis are independent games that can
// easily land on the same fen (e.g. both at the starting position), and a
// result computed for one must never paint the other's bar.
function currentAnalysis(page, view) {
  return lastAnalysis && lastAnalysis.page === page && lastAnalysis.fen === view.fen ? lastAnalysis : null;
}

function whitePercentFor(page, view) {
  if (view.status === "checkmate") {
    // The side to move has no moves and just got mated — this, and a
    // forced mate the search itself found (below), are the only cases
    // that empty the bar completely.
    return view.turn === "white" ? 0 : 100;
  }
  const analysis = currentAnalysis(page, view);
  if (analysis && analysis.kind === "mateIn") {
    return analysis.value > 0 ? 100 : 0;
  }
  const score = analysis ? analysis.value : view.score;
  const t = Math.tanh(score / EVAL_BAR_SENSITIVITY_CP); // -1..1
  return 50 + t * (50 - EVAL_BAR_FLOOR_PERCENT);
}

function evalBarText(page, view) {
  const analysis = currentAnalysis(page, view);
  if (analysis && analysis.kind === "mateIn") {
    return `M${analysis.value}`;
  }
  const score = analysis ? analysis.value : view.score;
  const pawns = (score / 100).toFixed(1);
  return score >= 0 ? `+${pawns}` : pawns;
}

function paintEvalBar(page, view) {
  const whitePercent = whitePercentFor(page, view);
  document.getElementById("eval-bar-white").style.height = `${whitePercent}%`;
  document.getElementById("eval-bar-black").style.height = `${100 - whitePercent}%`;
  document.getElementById("eval-bar-text").textContent = evalBarText(page, view);
  const analysis = currentAnalysis(page, view);
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
async function updateAnalysis(page, view, depth) {
  let result;
  try {
    result = await invoke(cmdName(page, "analyze"), { depth });
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
  if (viewFor(page).fen !== view.fen) return;
  lastAnalysis = { page, fen: view.fen, depth, ...result };
  if (page === activePage) paintEvalBar(page, viewFor(page));
}

function renderEvalBar(page, view) {
  const column = document.getElementById("eval-bar-column");
  // Forced on for Analysis — there's no toggle to check there, the bar is
  // simply always part of that tab.
  const showEvalBar = page === "analysis" ? true : document.getElementById("eval-bar-checkbox").checked;
  column.classList.toggle("hidden", !showEvalBar);
  if (!showEvalBar) return;

  // Paint instantly with the cheap material score (or a still-valid
  // analysis already on hand for this exact position), then kick off the
  // real search in the background — but only if this position/depth
  // hasn't already been requested, so plain re-renders of an unchanged
  // position (selecting a piece, switching sidebar pages, ...) never
  // start a redundant search.
  paintEvalBar(page, view);
  const depth = Number(document.getElementById("eval-bar-depth").value);
  const key = `${page}|${view.fen}|${depth}`;
  if (key === requestedAnalysisKey) return;
  requestedAnalysisKey = key;
  // Deferred to the next event-loop tick rather than called inline here —
  // the move/board update for this render must get scheduled and painted
  // on its own, never bundled into the same synchronous burst as kicking
  // off an analyze() request. The eval bar is allowed to visibly catch up
  // a beat later; the piece appearing where it was dropped is not.
  setTimeout(() => updateAnalysis(page, view, depth), 0);
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
    // display instead of calling the full render() — and only if Play is
    // actually the page on screen right now; the clock DOM is shared with
    // Analysis (see paint()), so painting Play's numbers into it while the
    // player is looking at Analysis would show the wrong tab's clock.
    currentView = latest;
    if (activePage === "play") renderClocks(latest);
  }, CLOCK_POLL_INTERVAL_MS);
}

function showGameOverModal(view) {
  const info = gameOverText(view);
  if (!info) return;
  document.getElementById("game-over-title").textContent = info.title;
  document.getElementById("game-over-detail").textContent = info.detail;
  document.getElementById("game-over-fen").value = view.fen;
  document.getElementById("game-over-pgn").value = view.pgn;
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
// Re-run on every render() now that the board can flip (rank/file order
// depends on gameMode, which only settles once the pre-game setup is
// done) — previously built once at startup, back when the board only ever
// had one orientation. Clears its own previous output first since it can
// now be called repeatedly.
function renderLabels(page) {
  const wrapper = document.querySelector(".board-wrapper");
  wrapper.querySelectorAll(".rank-label, .file-label, .board-corner").forEach((el) => el.remove());

  const flipped = flippedFor(page);
  const ranks = flipped ? [0, 1, 2, 3, 4, 5, 6, 7] : [7, 6, 5, 4, 3, 2, 1, 0];
  const files = flipped ? [7, 6, 5, 4, 3, 2, 1, 0] : [0, 1, 2, 3, 4, 5, 6, 7];

  ranks.forEach((rank, index) => {
    const label = document.createElement("div");
    label.className = "rank-label";
    label.textContent = rank + 1;
    label.style.gridRow = index + 1;
    label.style.gridColumn = 1;
    wrapper.appendChild(label);
  });

  const corner = document.createElement("div");
  corner.className = "board-corner";
  corner.style.gridRow = 9;
  corner.style.gridColumn = 1;
  wrapper.appendChild(corner);

  files.forEach((file, index) => {
    const label = document.createElement("div");
    label.className = "file-label";
    label.textContent = FILES[file];
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
// The Analysis tab's own view — kept entirely separate from currentView
// (Play's), since the two boards are independent games on the backend (see
// AnalysisGame in src-tauri/src/commands.rs) and switching tabs must never
// mix their data.
let analysisView = null;
// Purely a display setting, not tied to any "human color" the way Play's
// flip is — Analysis has no side selection, just a manual toggle button.
let analysisFlipped = false;

// moveContext is "move"/"capture" when this render follows a move the
// player just made, or omitted for renders that don't represent a new move
// (selecting a piece, switching sidebar pages, a clock tick, ...) — it's
// only used to pick a sound, not to decide what to draw.
//
// render() (Play) and paintAnalysis() (Analysis) are both thin wrappers
// around this one shared core — the two tabs share a single physical
// board/PGN-panel/FEN-box/eval-bar in the DOM (only one is ever visible at
// a time) rather than duplicating that markup and all of this logic, so
// paint() takes an explicit page and caches the view for whichever one it
// is even when that page isn't the one currently on screen — a Play move
// arriving while the Analysis tab is showing (or vice versa — nothing
// else can touch Analysis, but the Play clock keeps ticking regardless of
// which tab is visible) must update the right cached view without
// clobbering the DOM the player is actually looking at.
function paint(page, view, moveContext) {
  if (page === "play") currentView = view;
  else analysisView = view;
  if (page !== activePage) return;

  // lastAnalysis is intentionally NOT cleared here — currentAnalysis(view)
  // (above) already treats it as stale once view.fen no longer matches, and
  // actually clearing it on every render (including a plain selection
  // click that doesn't change the position) used to wipe out a perfectly
  // valid analysis and flash the bar back to the material-only score.
  renderLabels(page);
  const board = document.getElementById("board");
  board.innerHTML = "";

  const kingInCheckSquare =
    view.status === "check" || view.status === "checkmate" ? findKingSquare(view, view.turn) : null;

  // ranks/files are iterated in visual order (top-left to bottom-right) —
  // reversed from the "normal" a1-bottom-left orientation when the board
  // is flipped — since squares are simply appended in DOM order and placed
  // by the CSS grid's auto-flow, not given explicit row/column positions.
  const flipped = flippedFor(page);
  const ranks = flipped ? [0, 1, 2, 3, 4, 5, 6, 7] : [7, 6, 5, 4, 3, 2, 1, 0];
  const files = flipped ? [7, 6, 5, 4, 3, 2, 1, 0] : [0, 1, 2, 3, 4, 5, 6, 7];

  for (const rank of ranks) {
    for (const file of files) {
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
      square.addEventListener("mousedown", (event) => handlePointerDown(page, event, squareName, view));

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
          const fromPos = visualPosition(fileOf(lastMove.from), rankOf(lastMove.from), flipped);
          const toPos = visualPosition(file, rank, flipped);
          const dx = (fromPos.col - toPos.col) * SQUARE_SIZE_REM;
          const dy = (fromPos.row - toPos.row) * SQUARE_SIZE_REM;
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

  document.getElementById("status").textContent = statusText(page, view);
  renderPgn(view);
  renderFen(view);
  if (page === "play") {
    renderClocks(view);
  } else {
    // Analysis never has a clock — keep the shared clock DOM out of the
    // way rather than leaving Play's last-painted numbers on screen.
    document.getElementById("black-clock").classList.add("clock-hidden");
    document.getElementById("white-clock").classList.add("clock-hidden");
  }
  renderEvalBar(page, view);
  playSoundForTransition(page, view, moveContext);

  if (page === "play") {
    renderSetupPickers();
    // Game::resign() resigns whichever side's turn it currently is —
    // there's no explicit "which color is resigning" — so letting the
    // human click Resign during the computer's turn would misattribute
    // the resignation to the computer's side. Simplest correct fix: only
    // allow it on the human's own turn, same as board input is already
    // frozen the rest of the time it's not their move.
    document.getElementById("resign").disabled =
      gameMode.opponent === "computer" &&
      !view.awaitingClockChoice &&
      !isHumanTurn(view) &&
      !GAME_OVER_STATUSES.has(view.status);
    document.getElementById("clock-picker-overlay").classList.toggle("hidden", !view.awaitingClockChoice);
    if (GAME_OVER_STATUSES.has(view.status)) {
      showGameOverModal(view);
    } else {
      hideGameOverModal();
    }
    startClockPollingIfNeeded(view);
    maybeTriggerAiMove(view);
  }
}

function render(view, moveContext) {
  paint("play", view, moveContext);
}

function paintAnalysis(view, moveContext) {
  paint("analysis", view, moveContext);
}

// Shared by both interaction paths: clicking a second, already-highlighted
// square, and dropping a dragged piece onto one.
async function performMove(page, fromSquare, toSquare, view) {
  let promotion = null;
  if (isPromotionMove(view, fromSquare, toSquare)) {
    promotion = await askPromotionChoice(view.turn);
  }

  const wasCapture = !!pieceAt(view.board, toSquare);

  try {
    const nextView = await invoke(cmdName(page, "make_move"), { from: fromSquare, to: toSquare, promotion });
    selectedSquare = null;
    legalTargets = [];
    lastMove = { from: fromSquare, to: toSquare };
    lastMoveAnimated = false;
    paint(page, nextView, wasCapture ? "capture" : "move");
  } catch (err) {
    console.error("make_move failed:", err);
    selectedSquare = null;
    legalTargets = [];
    paint(page, view);
  }
}

// ---------- Playing against the engine ----------
//
// Tracks the FEN an AI move has already been requested for, so a request
// only ever fires once per position — render() calls maybeTriggerAiMove on
// every render, including ones that aren't a new position at all (a clock
// tick, switching sidebar pages, ...), and this is what keeps that from
// re-requesting a move for a position already in flight or already
// answered. Reset alongside lastMove wherever the position changes for a
// reason other than a move (new game, FEN/PGN load) — same idea as why
// lastMove gets reset there, so a stale value from a previous game/position
// never suppresses a request that should actually fire.
let aiMoveRequestedForFen = null;

function maybeTriggerAiMove(view) {
  if (gameMode.opponent !== "computer") return;
  if (view.awaitingClockChoice || GAME_OVER_STATUSES.has(view.status)) return;
  if (isHumanTurn(view)) return;
  if (aiMoveRequestedForFen === view.fen) return;
  aiMoveRequestedForFen = view.fen;
  // Deferred to the next tick for the same reason updateAnalysis is (see
  // renderEvalBar) — this render's own DOM update must get scheduled and
  // painted before kicking off another IPC round-trip, not bundled into
  // the same synchronous burst as it.
  setTimeout(() => requestAiMove(view.fen), 0);
}

// Search-side improvements since this was first set (transposition table,
// killer moves, aspiration windows, PVS, late move reductions, and
// dropping per-node Game cloning in favor of a lightweight Position type
// — see engine/src/ai/search.rs) made even depth 8 land well under this in
// every position measured so far, but this stays generous rather than
// tight: it exists so a request that never comes back for any *other*
// reason (a genuine backend bug, an IPC hiccup, ...) can't leave the game
// waiting on a move forever, not to shave the common case as close as
// possible. It's not the human's turn while the computer is "thinking,"
// so with nothing to click and no move ever arriving, that's a hard lock
// — worth guarding against defensively even without a confirmed root cause.
const AI_MOVE_TIMEOUT_MS = 60_000;

function withTimeout(promise, ms) {
  return Promise.race([
    promise,
    new Promise((_, reject) => setTimeout(() => reject(new Error(`timed out after ${ms}ms`)), ms)),
  ]);
}

// Set the instant a request actually starts, cleared the instant it
// settles (either way) — statusText reads this to show a live "Computer is
// thinking… (Ns)" count instead of a bare, unchanging message. Even an
// unusually slow search taking several seconds looks exactly like a stuck
// one unless there's some visible sign it's still working; a ticking
// counter is that sign, cheap as it is.
let aiThinkingStartedAt = null;
let aiThinkingTickHandle = null;

function startAiThinkingTicker() {
  if (aiThinkingTickHandle !== null) {
    clearInterval(aiThinkingTickHandle);
  }
  aiThinkingStartedAt = Date.now();
  // Only touches the #status text directly (not a full paint()) for the
  // same reason clock polling doesn't call render() on every tick — no
  // need to rebuild the board and re-bind 64 click listeners four times a
  // second just to update one line of text. Guarded on activePage — #status
  // is shared with the Analysis tab (see paint()), so this must not paint
  // over Analysis's own status while Play's AI is thinking in the
  // background.
  aiThinkingTickHandle = setInterval(() => {
    if (activePage === "play") {
      document.getElementById("status").textContent = statusText("play", currentView);
    }
  }, 250);
}

function stopAiThinkingTicker() {
  aiThinkingStartedAt = null;
  if (aiThinkingTickHandle !== null) {
    clearInterval(aiThinkingTickHandle);
    aiThinkingTickHandle = null;
  }
}

async function requestAiMove(expectedFen) {
  // Deliberately gameMode.computerDepth, not the eval bar's own depth
  // dropdown — how hard the engine is thinking about the position it's
  // showing you and how strong an opponent it plays are different
  // questions with different natural defaults (you might want a weak,
  // fast opponent but still watch a deep evaluation of your own moves).
  //
  // No artificial minimum thinking time here (there used to be one, to
  // make an instant book move feel less like a reflex) — it maximized
  // exactly the window where clock-poll's independent 250ms currentView
  // update could race ahead of this request's own completion, which is
  // what was silently discarding a perfectly good move and freezing the
  // board mid-opening. That specific race is fixed now (see the note
  // that used to be here, and request_ai_move's generation check on the
  // Rust side), but the padding delay wasn't worth keeping around as a
  // purely cosmetic feature once it had caused real harm.
  startAiThinkingTicker();
  let result;
  try {
    result = await withTimeout(invoke("request_ai_move", { depth: gameMode.computerDepth }), AI_MOVE_TIMEOUT_MS);
  } catch (err) {
    console.error("request_ai_move failed or timed out — retrying shortly:", err);
    stopAiThinkingTicker();
    // withTimeout only stops *this function* from waiting on the backend
    // call — it doesn't cancel it. Left alone, an abandoned search can
    // still finish on its own later and apply its move, invisibly,
    // whenever it happens to complete. abandon_ai_move bumps the backend's
    // generation counter so that if it does finish, its own staleness
    // check rejects it instead of silently landing a move nobody's
    // waiting on anymore. This must happen before the retry below, not
    // after — a real bug here previously let the retry fire off a fresh
    // request_ai_move using this function's own stale `expectedFen`, and
    // if the abandoned search happened to succeed in between, the retry
    // ended up asking the engine to move for whichever side was *actually*
    // to move by then — the human's own side, from the human's point of
    // view, since nothing had told the backend this side was off limits.
    try {
      await invoke("abandon_ai_move");
    } catch (abandonErr) {
      console.error("abandon_ai_move failed:", abandonErr);
    }

    // Re-fetch real state rather than trusting `currentView`/`expectedFen`
    // — both reflect the position as it stood when this request started,
    // which is exactly the stale snapshot that let the bug above happen.
    setTimeout(async () => {
      const latest = await invoke("get_state");
      paint("play", latest);
      if (latest.fen === expectedFen) {
        aiMoveRequestedForFen = null;
        maybeTriggerAiMove(latest);
      }
      // If the fen no longer matches, the position already moved on for
      // some other reason (a resign, a new game, ...) — nothing to retry,
      // and the paint() above already shows whatever the real state is.
    }, 1500);
    return;
  }

  stopAiThinkingTicker();
  const wasCapture = !!pieceAt(currentView.board, result.to);
  lastMove = { from: result.from, to: result.to };
  lastMoveAnimated = false;
  render(result.view, wasCapture ? "capture" : "move");
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
async function selectSquare(page, squareName, view) {
  selectedSquare = squareName;
  const requestId = ++selectionRequestId;
  const targets = await invoke(cmdName(page, "legal_moves"), { square: squareName });
  if (requestId !== selectionRequestId) return; // superseded by a later selection
  legalTargets = targets;
  paint(page, view);
}

function handlePointerDown(page, event, squareName, view) {
  if (event.button !== 0) return; // left button only
  if (!isHumanTurnFor(page, view)) return; // the engine's own pieces aren't the player's to move

  if (selectedSquare && legalTargets.includes(squareName)) {
    performMove(page, selectedSquare, squareName, view);
    return;
  }

  const piece = pieceAt(view.board, squareName);
  if (piece && piece.color === view.turn) {
    selectSquare(page, squareName, view); // not awaited — see its own comment
    if (settings.interactionMode === "drag") {
      beginDrag(page, event, squareName);
    }
    return;
  }

  selectedSquare = null;
  legalTargets = [];
  paint(page, view);
}

// ---------- Drag and drop ----------
//
// Layered on top of click-to-select-then-click-to-move (still fully intact
// above) rather than replacing it — picking a piece up starts a drag *and*
// selects it, so a plain click (mousedown+mouseup with no real movement)
// still works exactly as a click always did.
function beginDrag(page, event, fromSquare) {
  const img = event.currentTarget.querySelector(".piece-sprite");
  if (!img) return;

  const rect = img.getBoundingClientRect();
  const ghost = img.cloneNode(true);
  ghost.classList.add("drag-ghost");
  ghost.style.width = `${rect.width}px`;
  ghost.style.height = `${rect.height}px`;
  document.body.appendChild(ghost);

  dragState = {
    page,
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
  const { page, fromSquare, ghost, hoveredSquare, rafHandle } = dragState;

  document.removeEventListener("mousemove", handleDragMove);
  document.removeEventListener("mouseup", handleDragEnd);
  if (rafHandle !== null) cancelAnimationFrame(rafHandle);
  ghost.remove();
  setDropHover(hoveredSquare, false);
  dragState = null;

  const droppedOnLegalTarget =
    hoveredSquare && hoveredSquare !== fromSquare && legalTargets.includes(hoveredSquare);

  if (droppedOnLegalTarget) {
    await performMove(page, fromSquare, hoveredSquare, viewFor(page));
  } else {
    // Dropped somewhere illegal (or back on its own square) — nothing
    // moved, but render() was skipping the piece at fromSquare for as
    // long as dragState was set, so it needs one more render to bring it
    // back now that dragState is null again.
    paint(page, viewFor(page));
  }
}

async function startNewGame() {
  selectedSquare = null;
  legalTargets = [];
  pgnExpanded = false;
  lastMove = null;
  lastMoveAnimated = false;
  aiMoveRequestedForFen = null;
  stopAiThinkingTicker();
  hideGameOverModal();
  render(await invoke("new_game"));
}

async function resign() {
  if (!confirm(`Are you sure ${currentView.turn === "white" ? "White" : "Black"} wants to resign?`)) {
    return;
  }
  stopAiThinkingTicker();
  render(await invoke("resign"));
}

async function copyToClipboard(text) {
  try {
    await navigator.clipboard.writeText(text);
  } catch (err) {
    console.error("clipboard copy failed:", err);
  }
}

// The FEN box is shared between the Play and Analysis tabs (see paint()),
// so "Load" has to target whichever one is actually on screen — loading
// into Play while the player is looking at Analysis (or vice versa) would
// silently edit a game they can't even see.
async function loadFen() {
  const page = activePage;
  const fen = document.getElementById("fen-text").value.trim();
  try {
    const view = await invoke(cmdName(page, "load_fen"), { fen });
    selectedSquare = null;
    legalTargets = [];
    pgnExpanded = false;
    lastMove = null;
    lastMoveAnimated = false;
    if (page === "play") {
      aiMoveRequestedForFen = null;
      stopAiThinkingTicker();
    }
    paint(page, view);
  } catch (err) {
    alert(`Couldn't load that FEN:\n${err}`);
  }
}

// Only reachable from the Analysis tab (see updatePageVisibility — the
// Import button is hidden everywhere else), so this always targets the
// Analysis board.
async function importPgn() {
  const pgn = window.prompt("Paste PGN to import:");
  if (!pgn) return; // cancelled or empty

  try {
    const view = await invoke("analysis_load_pgn", { pgn });
    selectedSquare = null;
    legalTargets = [];
    pgnExpanded = false;
    lastMove = null;
    lastMoveAnimated = false;
    paintAnalysis(view);
  } catch (err) {
    alert(`Couldn't import that PGN:\n${err}`);
  }
}

async function resetAnalysisPosition() {
  selectedSquare = null;
  legalTargets = [];
  pgnExpanded = false;
  lastMove = null;
  lastMoveAnimated = false;
  paintAnalysis(await invoke("analysis_reset"));
}

// Purely a local display toggle — no backend call, nothing about the
// position itself changes, so this just repaints with the same view under
// the opposite orientation.
function toggleAnalysisFlip() {
  analysisFlipped = !analysisFlipped;
  if (analysisView) paintAnalysis(analysisView);
}

document.addEventListener("DOMContentLoaded", () => {
  renderSidebar();
  updatePageHeading();
  updatePageVisibility();
  renderClockPicker();
  document.getElementById("new-game").addEventListener("click", startNewGame);
  document.getElementById("resign").addEventListener("click", resign);
  document.getElementById("game-over-close").addEventListener("click", startNewGame);
  document.getElementById("game-over-copy-fen").addEventListener("click", () => {
    copyToClipboard(document.getElementById("game-over-fen").value);
  });
  document.getElementById("game-over-copy-pgn").addEventListener("click", () => {
    copyToClipboard(document.getElementById("game-over-pgn").value);
  });
  document.getElementById("analysis-reset").addEventListener("click", resetAnalysisPosition);
  document.getElementById("analysis-flip").addEventListener("click", toggleAnalysisFlip);
  // The eval bar is shared between both tabs (see paint()/renderEvalBar) —
  // these controls always affect whichever page is actually showing it.
  document.getElementById("eval-bar-checkbox").addEventListener("change", () =>
    renderEvalBar(activePage, viewFor(activePage))
  );
  document.getElementById("eval-bar-depth").addEventListener("change", () =>
    renderEvalBar(activePage, viewFor(activePage))
  );

  // Clicking the toggle expands to full history; clicking anywhere else in
  // the panel while expanded collapses it back to the last
  // PGN_COLLAPSED_ROWS rows. stopPropagation on the toggle keeps that same
  // click from also bubbling up and immediately re-collapsing it. Reads
  // viewFor(activePage), not currentView — the PGN panel is shared between
  // Play and Analysis, same as the eval bar above.
  document.getElementById("pgn-toggle").addEventListener("click", (event) => {
    event.stopPropagation();
    pgnExpanded = true;
    renderPgn(viewFor(activePage));
  });
  document.getElementById("pgn-panel").addEventListener("click", () => {
    if (pgnExpanded) {
      pgnExpanded = false;
      renderPgn(viewFor(activePage));
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
    copyToClipboard(viewFor(activePage).pgn);
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
  // Scoped to #settings-panel specifically — the opponent/color pickers in
  // the pre-game modal reuse the same .settings-choice-option styling (see
  // renderSetupPickers) but aren't settings.interactionMode choices, and a
  // global selector here would misinterpret clicks on those as an attempt
  // to set interactionMode to undefined (they carry data-opponent/
  // data-color, not data-mode).
  document.querySelectorAll("#settings-panel .settings-choice-option").forEach((button) => {
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

  document.querySelectorAll("#opponent-choice .settings-choice-option").forEach((button) => {
    button.addEventListener("click", () => {
      gameMode.opponent = button.dataset.opponent;
      renderSetupPickers();
    });
  });
  document.querySelectorAll("#color-choice .settings-choice-option").forEach((button) => {
    button.addEventListener("click", () => {
      gameMode.humanColor = button.dataset.color;
      renderSetupPickers();
    });
  });
  document.getElementById("computer-depth").addEventListener("change", (event) => {
    gameMode.computerDepth = Number(event.target.value);
  });
  renderSetupPickers();

  // Both boards' initial state are fetched up front regardless of which
  // page is active at launch (Analysis, by default) — paint() itself
  // already skips touching the DOM for whichever page isn't currently
  // showing (see its early return), so this just primes both caches ahead
  // of whenever the player switches tabs.
  invoke("get_state").then((view) => paint("play", view));
  invoke("analysis_get_state").then((view) => paint("analysis", view));
});
