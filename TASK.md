I want to add an option (default false) to the main cmdy config called something like `overwriteShellCommand` that if true, when cmdy is run and a command is selected to be run, cmdy overwrites the last entry in the shell's history. Supporting bash and zsh. That way, after the command ends, you can press up-arrow and that command will be present, instead of the cmdy command.

## Tasks

- [x] Add overwrite_shell_command field to AppConfig in config.rs
- [x] Detect the current shell (bash or zsh) from environment variables
- [x] Implement history file overwrite logic for bash and zsh
- [x] Integrate history overwriting into execute_command flow
- [x] Run cargo check, fix, fmt, and clippy
- [ ] Test the implementation manually with bash and zsh

## Implementation Strategy

The feature will:
1. Add a `overwrite_shell_command` boolean field (default: false) to the AppConfig struct
2. Detect which shell the user is running (bash or zsh) by checking the SHELL environment variable
3. Before executing the selected command, overwrite the last history entry in the shell's history file
4. Support bash (~/.bash_history) and zsh (~/.zsh_history) history file formats
