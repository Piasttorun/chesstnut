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

let selectedSquare = null;
let legalTargets = [];
let pgnExpanded = false;
let clockPollHandle = null;

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
// much of the bar — full 0%/100% is reserved for an actual checkmate below,
// not just a big material swing.
const EVAL_BAR_FLOOR_PERCENT = 10;

function whitePercentFor(view) {
  if (view.status === "checkmate") {
    // The side to move has no moves and just got mated — this is the only
    // case that empties the bar completely.
    return view.turn === "white" ? 0 : 100;
  }
  const t = Math.tanh(view.score / EVAL_BAR_SENSITIVITY_CP); // -1..1
  return 50 + t * (50 - EVAL_BAR_FLOOR_PERCENT);
}

function renderEvalBar(view) {
  const column = document.getElementById("eval-bar-column");
  const showEvalBar = document.getElementById("eval-bar-checkbox").checked;
  column.classList.toggle("hidden", !showEvalBar);
  if (!showEvalBar) return;

  const whitePercent = whitePercentFor(view);
  document.getElementById("eval-bar-white").style.height = `${whitePercent}%`;
  document.getElementById("eval-bar-black").style.height = `${100 - whitePercent}%`;

  const pawns = (view.score / 100).toFixed(1);
  document.getElementById("eval-bar-text").textContent = view.score >= 0 ? `+${pawns}` : pawns;
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

function render(view) {
  currentView = view;
  const board = document.getElementById("board");
  board.innerHTML = "";

  for (let rank = 7; rank >= 0; rank--) {
    for (let file = 0; file < 8; file++) {
      const squareName = `${FILES[file]}${rank + 1}`;
      const square = document.createElement("div");
      const isLight = (file + rank) % 2 === 1;
      const classes = ["square", isLight ? "light" : "dark"];
      if (squareName === selectedSquare) classes.push("selected");
      if (legalTargets.includes(squareName)) classes.push("legal-target");
      square.className = classes.join(" ");
      square.dataset.square = squareName;
      square.addEventListener("click", () => handleSquareClick(squareName, view));

      const piece = pieceAt(view.board, squareName);
      if (piece) {
        const img = document.createElement("img");
        img.src = pieceSpriteSrc(piece);
        img.alt = `${piece.color} ${piece.kind}`;
        img.className = "piece-sprite";
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

  document.getElementById("clock-picker-overlay").classList.toggle("hidden", !view.awaitingClockChoice);
  if (GAME_OVER_STATUSES.has(view.status)) {
    showGameOverModal(view);
  } else {
    hideGameOverModal();
  }
  startClockPollingIfNeeded(view);
}

async function handleSquareClick(squareName, view) {
  if (selectedSquare && legalTargets.includes(squareName)) {
    let promotion = null;
    if (isPromotionMove(view, selectedSquare, squareName)) {
      promotion = await askPromotionChoice(view.turn);
    }

    try {
      const nextView = await invoke("make_move", { from: selectedSquare, to: squareName, promotion });
      selectedSquare = null;
      legalTargets = [];
      render(nextView);
    } catch (err) {
      console.error("make_move failed:", err);
      selectedSquare = null;
      legalTargets = [];
      render(view);
    }
    return;
  }

  const piece = pieceAt(view.board, squareName);
  if (piece && piece.color === view.turn) {
    selectedSquare = squareName;
    legalTargets = await invoke("legal_moves", { square: squareName });
  } else {
    selectedSquare = null;
    legalTargets = [];
  }

  render(view);
}

async function startNewGame() {
  selectedSquare = null;
  legalTargets = [];
  pgnExpanded = false;
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
    render(view);
  } catch (err) {
    alert(`Couldn't import that PGN:\n${err}`);
  }
}

document.addEventListener("DOMContentLoaded", () => {
  renderLabels();
  renderClockPicker();
  document.getElementById("new-game").addEventListener("click", startNewGame);
  document.getElementById("resign").addEventListener("click", resign);
  document.getElementById("game-over-close").addEventListener("click", startNewGame);
  document.getElementById("eval-bar-checkbox").addEventListener("change", () => renderEvalBar(currentView));

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

  invoke("get_state").then(render);
});
