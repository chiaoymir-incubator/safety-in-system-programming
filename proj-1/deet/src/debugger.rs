use crate::debugger_command::DebuggerCommand;
use crate::inferior::Inferior;
use crate::inferior::Status;
use crate::dwarf_data::{DwarfData, Error as DwarfError};
use rustyline::error::ReadlineError;
use rustyline::Editor;


pub struct Debugger {
    target: String,
    history_path: String,
    readline: Editor<()>,
    inferior: Option<Inferior>,
    debug_data: DwarfData,
    breakpoints: Vec<usize>,
}

impl Debugger {
    /// Initializes the debugger.
    pub fn new(target: &str) -> Debugger {
        // initialize the DwarfData
        let debug_data = match DwarfData::from_file(target) {
            Ok(val) => val,
            Err(DwarfError::ErrorOpeningFile) => {
                println!("Could not open file {}", target);
                std::process::exit(1);
            }
            Err(DwarfError::DwarfFormatError(err)) => {
                println!("Could not debugging symbols from {}: {:?}", target, err);
                std::process::exit(1);
            }
        };

        let history_path = format!("{}/.deet_history", std::env::var("HOME").unwrap());
        let mut readline = Editor::<()>::new();
        // Attempt to load history from ~/.deet_history if it exists
        let _ = readline.load_history(&history_path);

        debug_data.print();

        Debugger {
            target: target.to_string(),
            history_path,
            readline,
            inferior: None,
            debug_data,
            breakpoints: Vec::<usize>::new(),
        }
    }

    fn cont(&mut self) {
        match &mut self.inferior {
            Some(inferior) => {
                match inferior.cont() {
                    Err(err) => println!("error: {}", err),
                    Ok(status) => {
                        match status {
                            Status::Stopped(sig, rip) => {
                                println!("Child Stopped (status {})", sig);
                                inferior.print_location(&self.debug_data, rip);
                            },
                            Status::Exited(sig) => {
                                println!("Child exited (status {})", sig);
                                self.kill();
                            },
                            _ => (),
                        }
                    }
                }
            },
            None => println!("The inferior is not running!"),
        }
    }

    fn kill(&mut self) {
        match &mut self.inferior {
            Some(inferior) => {
                if !inferior.kill() {
                    panic!("Not able to reap the previous process!");
                }
                inferior.wait(None);
                // self.breakpoints.clear();
            },
            None => (),
        }
    }

    fn print_backtrace(&self) {
        match &self.inferior {
            Some(inferior) => {
                inferior.print_backtrace(&self.debug_data);
            },
            None => (),
        }
    }

    fn parse_address(addr: &str) -> Option<usize> {
        let addr_without_0x = if addr.to_lowercase().starts_with("0x") {
            &addr[2..]
        } else {
            &addr
        };
        usize::from_str_radix(addr_without_0x, 16).ok()
    }

    fn add_breakpoint(&mut self, addr: usize) {
        println!("Set breakpoint {} at {:#x}", self.breakpoints.len(), addr);
        self.breakpoints.push(addr);
        if self.inferior.is_some() {
            let inferior = self.inferior.as_mut().unwrap();
            inferior.add_breakpoint(addr);
        }
    }


    pub fn run(&mut self) {
        loop {
            match self.get_next_command() {
                DebuggerCommand::Run(args) => {
                    self.kill();
                    if let Some(inferior) = Inferior::new(&self.target, &args, &self.breakpoints) {
                        // Create the inferior
                        self.inferior = Some(inferior);
                        self.cont();
                        // You may use self.inferior.as_mut().unwrap() to get a mutable reference
                        // to the Inferior object
                    } else {
                        println!("Error starting subprocess");
                    }
                }
                DebuggerCommand::Quit => {
                    self.kill();
                    return;
                },
                DebuggerCommand::Cont => {
                    self.cont();
                },
                DebuggerCommand::Backtrace => {
                    self.print_backtrace();
                },
                DebuggerCommand::Break => {
                    let line_option = self.readline.history().last();
                    match line_option {
                        Some(line) => {
                            let tokens: Vec<&str>  = line.split_whitespace().collect();
                            let arg = tokens[1];
                            if arg.starts_with("*") {
                                let s = &arg[1..];
                                match Debugger::parse_address(s) {
                                    None => (),
                                    Some(addr) => {
                                        self.add_breakpoint(addr);
                                    },
                                }
                            } else {
                                let result = arg.parse::<usize>();
                                match result {
                                    Ok(line) => {
                                        match self.debug_data.get_addr_for_line(None, line) {
                                            None => (),
                                            Some(addr) => {
                                                self.add_breakpoint(addr);
                                            },
                                        }
                                    },
                                    Err(_) => {
                                        match self.debug_data.get_addr_for_function(None, arg) {
                                            None => (),
                                            Some(addr) => {
                                                self.add_breakpoint(addr);
                                            },
                                        }
                                    },
                                }
                            }
                        },
                        None => (),
                    }
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
                    self.readline.add_history_entry(line.as_str());
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
