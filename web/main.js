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

let selectedSquare = null;
let legalTargets = [];

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
    case "stalemate":
      return "Stalemate — draw";
    case "draw_fifty_move":
      return "Draw — 50-move rule";
    case "draw_repetition":
      return "Draw — threefold repetition";
    case "check":
      return `${turn} to move — check!`;
    default:
      return `${turn} to move`;
  }
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

function render(view) {
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
  render(await invoke("new_game"));
}

document.addEventListener("DOMContentLoaded", () => {
  renderLabels();
  document.getElementById("new-game").addEventListener("click", startNewGame);
  invoke("get_state").then(render);
});
