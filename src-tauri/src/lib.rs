mod commands;

use std::sync::Mutex;

use chesstnut::engine::game::Game;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .manage(Mutex::new(Game::new_pending_clock()))
    .invoke_handler(tauri::generate_handler![
      commands::new_game,
      commands::get_state,
      commands::legal_moves,
      commands::make_move,
      commands::load_fen,
      commands::load_pgn,
      commands::select_time_control,
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
