use crate::debugger_command::DebuggerCommand;
use crate::dwarf_data::{DwarfData, Error as DwarfError};
use crate::inferior::{Inferior, Status};
use nix::sys::ptrace;
use rustyline::error::ReadlineError;
use rustyline::history::FileHistory;
use rustyline::Editor;
use std::fs::File;
use std::io::{BufRead, BufReader};

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
                DebuggerCommand::BreakPoint(target) => {
                    let mut addr_opt: Option<u64> = None;
                    if target.starts_with('*') {
                        let addr_str = &target[1..];
                        addr_opt = parse_address(addr_str);
                    } else if let Ok(line_num) = target.parse::<u64>() {
                        if let Some(a) = self.debug_data.get_addr_for_line(None, line_num) {
                            addr_opt = Some(a);
                        }
                    } else {
                        if let Some(a) = self.debug_data.get_addr_for_function(None, &target) {
                            addr_opt = Some(a);
                        }
                    }
                    if addr_opt.is_none() {
                        eprintln!("Could not resolve breakpoint target {}. ", target);
                        eprintln!("Usage: {{b | break | breakpoint}} {{*raw address | line number | function name}}");
                        continue;
                    }
                    let addr = addr_opt.unwrap();
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
                DebuggerCommand::Step(count) => {
                    if self.inferior.is_none() {
                        println!("No inferior process running");
                        continue;
                    }

                    let mut status = Status::Exited(0); // Dummy initialization
                    let mut error = None;

                    // Create a scope to borrow self.inferior and self.debug_data
                    {
                        let inferior = self.inferior.as_mut().unwrap();
                        let debug_data = &self.debug_data;

                        // Loop 'count' times (for number of source lines)
                        'outer: for _ in 0..count {
                            let regs = ptrace::getregs(inferior.pid()).unwrap();
                            let start_line = debug_data.get_line_from_addr(regs.rip);

                            // Loop instructions until line changes
                            loop {
                                // Step one instruction using your Inferior::step method
                                let step_res = inferior.step();
                                match step_res {
                                    Ok(s) => {
                                        status = s;
                                        match status {
                                            Status::Stopped(signal, rip) => {
                                                // If stopped by something other than SIGTRAP, stop stepping
                                                if signal != nix::sys::signal::Signal::SIGTRAP {
                                                    break 'outer;
                                                }

                                                let current_line =
                                                    debug_data.get_line_from_addr(rip);

                                                // Check if we moved to a new line
                                                if start_line.is_some() && current_line.is_some() {
                                                    let start = start_line.as_ref().unwrap();
                                                    let current = current_line.as_ref().unwrap();
                                                    if start.file != current.file
                                                        || start.number != current.number
                                                    {
                                                        // println!("Line changed, stopping step");
                                                        break; // Line changed!
                                                    }
                                                }
                                            }
                                            _ => break 'outer, // Exited or Signaled
                                        }
                                    }
                                    Err(e) => {
                                        error = Some(e);
                                        break 'outer;
                                    }
                                }
                            }
                        }
                    }

                    if let Some(e) = error {
                        eprintln!("Step failed: {}", e);
                    } else {
                        self.print_status(&status);
                    }
                }
            }
        }
    }

    fn continue_inferior(&mut self) {
        let continue_res = self.inferior.as_mut().unwrap().cont();
        if continue_res.is_ok() {
            self.print_status(&continue_res.unwrap());
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

    fn print_status(&mut self, status: &Status) {
        match status {
            Status::Stopped(signal, rip) => {
                println!("Child stopped (signal {:?})", signal);
                let debug_current_line = self.debug_data.get_line_from_addr(*rip);
                let debug_current_func = self.debug_data.get_function_from_addr(*rip);
                if debug_current_line.is_some() || debug_current_func.is_some() {
                    print!("Stopped at ");
                    if debug_current_func.is_none() {
                        print!("<unknown function> ");
                    } else {
                        let current_func_name = debug_current_func.unwrap();
                        print!("{} ", current_func_name);
                    }
                    if debug_current_line.is_none() {
                        println!("<unknown location>");
                    } else {
                        let current_line = debug_current_line.unwrap();
                        println!("({}:{})", current_line.file, current_line.number);
                        Debugger::print_source_line(&current_line.file, current_line.number);
                    }
                }
            }
            Status::Exited(exit_code) => {
                println!("Child exited (status {})", exit_code);
                self.inferior = None;
            }
            Status::Signaled(signal) => {
                println!("Child terminated with signal {:?}", signal);
                self.inferior = None;
            }
        }
    }

    fn print_source_line(file_path: &str, line_number: u64) {
        if let Ok(file) = File::open(file_path) {
            let reader = BufReader::new(file);
            // nth() is 0-indexed, line_number is 1-indexed
            if let Some(Ok(line)) = reader.lines().nth((line_number - 1) as usize) {
                println!("{}\t{}", line_number, line.trim());
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
