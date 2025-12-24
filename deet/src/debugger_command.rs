pub enum DebuggerCommand {
    Quit,
    Run(Vec<String>),
    Continue,
    Backtrace,
    BreakPoint(String),
    Step(u64),
}

impl DebuggerCommand {
    pub fn from_tokens(tokens: &Vec<&str>) -> Option<DebuggerCommand> {
        match tokens[0] {
            "q" | "quit" => Some(DebuggerCommand::Quit),
            "r" | "run" => {
                let args = tokens[1..].to_vec();
                Some(DebuggerCommand::Run(
                    args.iter().map(|s| s.to_string()).collect(),
                ))
            }
            "c" | "cont" | "continue" => Some(DebuggerCommand::Continue),
            "bt" | "back" | "backtrace" => Some(DebuggerCommand::Backtrace),
            "b" | "break" | "breakpoint" => {
                if tokens.len() < 2 {
                    println!("No breakpoint location given");
                    return None;
                }
                Some(DebuggerCommand::BreakPoint(tokens[1].to_string()))
            }
            "s" | "step" => {
                let mut count: u64 = 1;
                if tokens.len() >= 2 {
                    if let Ok(c) = tokens[1].parse::<u64>() {
                        count = c;
                    } else {
                        println!("Invalid step count: {}", tokens[1]);
                        return None;
                    }
                }
                Some(DebuggerCommand::Step(count))
            }
            // Default case:
            _ => None,
        }
    }
}
