use crate::debugger_command::DebuggerCommand;
use crate::dwarf_data::{DwarfData, Error as DwarfError};
use crate::inferior::{Inferior, Status};
use rustyline::error::ReadlineError;
use rustyline::history::FileHistory;
use rustyline::Editor;

pub struct Debugger {
    target: String,
    history_path: String,
    readline: Editor<(), FileHistory>,
    inferior: Option<Inferior>,
    debug_data: DwarfData,
    breakpoints: Vec<u64>,
}

impl Debugger {
    /// Initializes the debugger.
    pub fn new(target: &str) -> Debugger {
        // TODO (milestone 3): initialize the DwarfData
        let debug_data = match DwarfData::from_file(target) {
            Ok(val) => val,
            Err(DwarfError::ErrorOpeningFile) => {
                println!("Could not open file {}", target);
                std::process::exit(1);
            }
            Err(DwarfError::DwarfFormatError(err)) => {
                println!(
                    "Could not load debugging symbols from {}: {:?}",
                    target, err
                );
                std::process::exit(1);
            }
        };
        debug_data.print();
        let history_path = format!("{}/.deet_history", std::env::var("HOME").unwrap());
        let mut readline = match Editor::<(), FileHistory>::new() {
            Ok(editor) => editor,
            Err(err) => {
                println!("Failed to initialize readline editor: {}", err);
                std::process::exit(1);
            }
        };
        // Attempt to load history from ~/.deet_history if it exists
        let _ = readline.load_history(&history_path);

        Debugger {
            target: target.to_string(),
            history_path,
            readline,
            inferior: None,
            debug_data,
            breakpoints: Vec::new(),
        }
    }

    pub fn run(&mut self) {
        loop {
            match self.get_next_command() {
                DebuggerCommand::Run(args) => {
                    if let Some(inferior) = self.inferior.as_mut() {
                        inferior.kill();
                        self.inferior = None;
                    }
                    if let Some(inferior) = Inferior::new(&self.target, &args, &self.breakpoints) {
                        // Create the inferior
                        self.inferior = Some(inferior);
                        // TODO (milestone 1): make the inferior run
                        // You may use self.inferior.as_mut().unwrap() to get a mutable reference
                        // to the Inferior object
                        self.continue_inferior();
                    } else {
                        println!("Error starting subprocess");
                    }
                }
                DebuggerCommand::Continue => {
                    if self.inferior.is_none() {
                        println!("No inferior process running");
                        continue;
                    }
                    self.continue_inferior();
                }
                DebuggerCommand::Backtrace => {
                    if self.inferior.is_none() {
                        println!("No inferior process running");
                        continue;
                    }
                    let bt_res = self
                        .inferior
                        .as_ref()
                        .unwrap()
                        .print_backtrace(&self.debug_data);
                    if bt_res.is_err() {
                        println!("Backtrace failed: {}", bt_res.err().unwrap());
                    }
                }
                DebuggerCommand::BreakPoint(location) => {
                    // if self.inferior.is_none() {
                    //     println!("No inferior process running");
                    //     continue;
                    // }
                    if !location.starts_with('*') {
                        println!("Breakpoint location must start with '*'");
                        continue;
                    }
                    let addr_str = &location[1..];
                    let addr = parse_address(addr_str);
                    if addr.is_none() {
                        println!("Invalid breakpoint address: {}", addr_str);
                        continue;
                    }
                    let addr = addr.unwrap();
                    println!(
                        "Setting breakpoint {} at {:#x}",
                        self.breakpoints.len(),
                        addr
                    );
                    self.breakpoints.push(addr);
                    if self.inferior.is_some() {
                        self.inferior.as_mut().unwrap().set_breakpoint(addr).ok();
                    }
                }
                DebuggerCommand::Quit => {
                    if let Some(inferior) = self.inferior.as_mut() {
                        inferior.kill();
                    }
                    return;
                }
            }
        }
    }

    fn continue_inferior(&mut self) {
        let continue_res = self.inferior.as_mut().unwrap().cont();
        if continue_res.is_ok() {
            match continue_res.unwrap() {
                Status::Stopped(signal, rip) => {
                    println!("Child stopped (signal {:?})", signal);
                    let debug_current_line = self.debug_data.get_line_from_addr(rip);
                    let debug_current_func = self.debug_data.get_function_from_addr(rip);
                    if debug_current_line.is_some() && debug_current_func.is_some() {
                        let current_line = debug_current_line.unwrap();
                        let current_func_name = debug_current_func.unwrap();
                        println!(
                            "Stopped at {} ({}:{})",
                            current_func_name, current_line.file, current_line.number
                        );
                    }
                }
                Status::Exited(exit_code) => println!("Child exited (status {})", exit_code),
                Status::Signaled(signal) => {
                    println!("Child exited exited due to a signal {:?}", signal)
                }
            }
        }
    }

    /// This function prompts the user to enter a command, and continues re-prompting until the user
    /// enters a valid command. It uses DebuggerCommand::from_tokens to do the command parsing.
    ///
    /// You don't need to read, understand, or modify this function.
    fn get_next_command(&mut self) -> DebuggerCommand {
        loop {
            // Print prompt and get next line of user input
            match self.readline.readline("(deet) ") {
                Err(ReadlineError::Interrupted) => {
                    // User pressed ctrl+c. We're going to ignore it
                    println!("Type \"quit\" to exit");
                }
                Err(ReadlineError::Eof) => {
                    // User pressed ctrl+d, which is the equivalent of "quit" for our purposes
                    return DebuggerCommand::Quit;
                }
                Err(err) => {
                    panic!("Unexpected I/O error: {:?}", err);
                }
                Ok(line) => {
                    if line.trim().len() == 0 {
                        continue;
                    }
                    let _ = self.readline.add_history_entry(line.as_str());
                    if let Err(err) = self.readline.save_history(&self.history_path) {
                        println!(
                            "Warning: failed to save history file at {}: {}",
                            self.history_path, err
                        );
                    }
                    let tokens: Vec<&str> = line.split_whitespace().collect();
                    if let Some(cmd) = DebuggerCommand::from_tokens(&tokens) {
                        return cmd;
                    } else {
                        println!("Unrecognized command.");
                    }
                }
            }
        }
    }
}

fn parse_address(addr: &str) -> Option<u64> {
    let addr_without_0x = if addr.to_lowercase().starts_with("0x") {
        &addr[2..]
    } else {
        &addr
    };
    u64::from_str_radix(addr_without_0x, 16).ok()
}
