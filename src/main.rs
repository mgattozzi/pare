use arboard::Clipboard;
use clap::Parser;
use crossterm::event;
use crossterm::event::Event;
use crossterm::event::KeyCode;
use crossterm::event::KeyEventKind;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use error_stack::Context;
use error_stack::Result;
use error_stack::ResultExt;
use ratatui::prelude::*;
use ratatui::widgets::*;
use sqlite::Connection;
use std::fmt;
use std::fs;
use std::io;
use std::io::stdout;
use std::io::IsTerminal;
use std::io::Read;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {}

fn main() -> Result<(), Error> {
    let _args = Args::parse();

    let db_folder = dirs::data_dir()
        .expect("data dir for the platform should exist")
        .join("pare");
    let db_path = db_folder.join("db");
    fs::create_dir_all(db_folder)
        .change_context(Error)
        .attach_printable("unable to create data folder for sqlite database")?;
    let db = sqlite::open(db_path)
        .change_context(Error)
        .attach_printable("unable to open the sqlite database")?;

    let stdin = io::stdin();
    if stdin.is_terminal() {
        enable_raw_mode().change_context(Error)?;
        stdout()
            .execute(EnterAlternateScreen)
            .change_context(Error)?;
        let mut terminal = Terminal::new(CrosstermBackend::new(stdout())).change_context(Error)?;
        let mut rows = Vec::new();

        let query = "SELECT * FROM clips";
        db.iterate(query, |pairs| {
            rows.push([pairs[1].1.unwrap().into()]);
            true
        })
        .change_context(Error)
        .attach_printable("insertion into database failed")?;

        let mut state = AppState::new(rows, db);

        let mut should_quit = false;
        while !should_quit {
            terminal.draw(|f| ui(f, &mut state)).change_context(Error)?;
            should_quit = handle_events(&mut state).change_context(Error)?;
        }

        disable_raw_mode().change_context(Error)?;
        stdout()
            .execute(LeaveAlternateScreen)
            .change_context(Error)?;
    } else {
        let mut clip = String::new();
        let mut handle = stdin.lock();
        handle
            .read_to_string(&mut clip)
            .change_context(Error)
            .attach_printable("unable to read in data from stdin")?;

        let query = format!(
            "
      CREATE TABLE IF NOT EXISTS clips (id INTEGER PRIMARY KEY AUTOINCREMENT, clip STRING);
      INSERT INTO clips (clip) VALUES ('{clip}');
    "
        );

        db.execute(query)
            .change_context(Error)
            .attach_printable("insertion into database failed")?;
        let mut clipboard = Clipboard::new()
            .change_context(Error)
            .attach_printable("unable to get access to the clipboard")?;
        clipboard
            .set_text(clip)
            .change_context(Error)
            .attach_printable("unable to set text for the clipboard")?;
    }
    Ok(())
}

fn handle_events(app_state: &mut AppState) -> Result<bool, Error> {
    if event::poll(std::time::Duration::from_millis(50)).change_context(Error)? {
        if let Event::Key(key) = event::read().change_context(Error)? {
            match (key.kind, key.code) {
                (KeyEventKind::Press, KeyCode::Esc) => return Ok(true),
                (KeyEventKind::Press, KeyCode::Down) => {
                    let max_idx = app_state.db_rows.len().checked_sub(1).unwrap_or(0);
                    let selected = app_state.state.selected().unwrap_or(0);

                    let selection = if selected == max_idx {
                        max_idx
                    } else {
                        selected + 1
                    };

                    app_state.state.select(Some(selection));
                }
                (KeyEventKind::Press, KeyCode::Up) => app_state.state.select(Some(
                    app_state
                        .state
                        .selected()
                        .unwrap_or(0)
                        .checked_sub(1)
                        .unwrap_or(0),
                )),
                (KeyEventKind::Press, KeyCode::Enter) => {
                    Clipboard::new()
                        .unwrap()
                        .set_text(&app_state.db_rows[app_state.state.selected().unwrap()][0])
                        .unwrap();
                    return Ok(true);
                }
                (KeyEventKind::Press, KeyCode::Delete) => {
                    let selected = app_state.state.selected().unwrap_or(0);
                    if app_state.db_rows.len() > 0 {
                        app_state.db_rows.remove(selected);
                        let query = format!(
                            "DELETE FROM clips
                          WHERE id in
                          (SELECT id FROM clips LIMIT 1 OFFSET {selected})
                        "
                        );
                        app_state
                            .db
                            .execute(query)
                            .change_context(Error)
                            .attach_printable("delete from database failed")?;
                    }
                }
                _ => {}
            }
        }
    }
    Ok(false)
}

fn ui(frame: &mut Frame, app_state: &mut AppState) {
    let main_layout =
        Layout::new(Direction::Vertical, [Constraint::Percentage(100)]).split(frame.size());
    frame.render_stateful_widget(
        Table::new(
            app_state
                .db_rows
                .clone()
                .into_iter()
                .map(Row::new)
                .collect::<Vec<Row<'_>>>(),
            [Constraint::Percentage(100)],
        )
        .highlight_style(Style::new().red().italic())
        .block(Block::bordered()),
        main_layout[0],
        &mut app_state.state,
    );
}

struct AppState {
    db_rows: Vec<[String; 1]>,
    db: Connection,
    state: TableState,
}

impl AppState {
    fn new(db_rows: Vec<[String; 1]>, db: Connection) -> Self {
        Self {
            db_rows,
            db,
            state: TableState::default().with_selected(0),
        }
    }
}

#[derive(Debug)]
struct Error;

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.write_str("Could not clip input")
    }
}

impl Context for Error {}
