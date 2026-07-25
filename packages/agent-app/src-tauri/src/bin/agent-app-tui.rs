use agent_app_lib::core::Gateway;

#[tokio::main]
async fn main() {
    // Gateway initialization failures are non-recoverable at startup
    let gateway = match Gateway::default() {
        Ok(g) => g,
        Err(error) => {
            eprintln!("error [{}]: {}", error.code(), error);
            std::process::exit(error.exit_code());
        }
    };

    // Initialize terminal with a guard for safety
    let _guard = match agent_app_lib::tui::TerminalGuard::new() {
        Ok(g) => g,
        Err(error) => {
            eprintln!("Failed to initialize terminal: {error}");
            let _ = agent_app_lib::tui::restore_terminal();
            std::process::exit(1);
        }
    };

    // Create ratatui terminal backend
    let mut terminal = match agent_app_lib::tui::init_terminal() {
        Ok(t) => t,
        Err(error) => {
            eprintln!("Failed to create terminal backend: {error}");
            std::process::exit(1);
        }
    };

    // Create the TUI app and run the event loop
    let mut app = match agent_app_lib::tui::app::TuiApp::new(gateway) {
        Ok(a) => a,
        Err(error) => {
            eprintln!("error [{}]: {}", error.code(), error);
            std::process::exit(error.exit_code());
        }
    };

    let result = app.run(&mut terminal).await;

    // Explicitly restore terminal before process exit
    drop(terminal);
    drop(_guard);

    if let Err(error) = result {
        eprintln!("error [{}]: {}", error.code(), error);
        std::process::exit(error.exit_code());
    }
}
