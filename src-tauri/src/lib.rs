mod commands;

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use chesstnut::engine::game::Game;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .manage(Mutex::new(Game::new_pending_clock()))
    // Bumped by every command that actually changes the position (see
    // commands::bump_generation) so `analyze` can tell a still-running
    // search that the position it's analyzing is stale and stop early —
    // see chesstnut::ai::Cancellation.
    .manage(Arc::new(AtomicU64::new(0)))
    .invoke_handler(tauri::generate_handler![
      commands::new_game,
      commands::get_state,
      commands::legal_moves,
      commands::make_move,
      commands::load_fen,
      commands::load_pgn,
      commands::select_time_control,
      commands::resign,
      commands::analyze,
      commands::request_ai_move,
    ])
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }
      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
