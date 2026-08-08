// The set of actions the operator can trigger from the console. Keys, descriptions and the
// help line shown by the UI are all derived from this single source of truth.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EmulatedCommand {
    Ring,
    ToggleAutoBuzz,
    AckAndBuzz,
    Ack,
    Quit,
}

impl EmulatedCommand {
    // All commands in the order they appear in the help line.
    const ALL: [EmulatedCommand; 5] = [
        EmulatedCommand::Ring,
        EmulatedCommand::ToggleAutoBuzz,
        EmulatedCommand::AckAndBuzz,
        EmulatedCommand::Ack,
        EmulatedCommand::Quit,
    ];

    // The console key that triggers this command.
    pub fn key(self) -> char {
        match self {
            EmulatedCommand::Ring => 'r',
            EmulatedCommand::ToggleAutoBuzz => 'a',
            EmulatedCommand::AckAndBuzz => 'b',
            EmulatedCommand::Ack => 'k',
            EmulatedCommand::Quit => 'q',
        }
    }

    // Short human-readable label for the help line.
    pub fn description(self) -> &'static str {
        match self {
            EmulatedCommand::Ring => "ring",
            EmulatedCommand::ToggleAutoBuzz => "toggle auto-buzz",
            EmulatedCommand::AckAndBuzz => "ack & buzz",
            EmulatedCommand::Ack => "ack",
            EmulatedCommand::Quit => "quit",
        }
    }

    // Maps a pressed key to its command, if any.
    pub fn fromKey(key: char) -> Option<EmulatedCommand> {
        EmulatedCommand::ALL
            .into_iter()
            .find(|cmd| cmd.key() == key)
    }
}

// Builds the help line shown by the UI.
pub fn getCommandDescription() -> String {
    EmulatedCommand::ALL
        .iter()
        .map(|cmd| format!("{} | {}", cmd.key(), cmd.description()))
        .collect::<Vec<_>>()
        .join("\r\n  ")
}
