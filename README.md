# Cmdy

Cmdy (pronounced "commandy") is a simple terminal helper for creating and running commands you need to run repeatedly, but which are not worth the effort and memorization of creating a permanent alias.

Think of Cmdy as being in between using up-arrow searching to run a command repeatedly, and creating a permanent script that you have to name and memorize. It's a great place to house commands you only *sometimes* run (which might be defy memorization) or commands that you run many times, but over a short period of time.

The benefit is that you just have to remember one thing in order to re-run your script: `cmdy`.

Cmdy commands are just scripts. The default shebang is `/bin/zsh`, but you can write scripts in Python, Perl, PHP, Bash, or whatever you like.

Your Cmdy commands are stored in `~/.cmdy/commands`. if you symlink `~/.cmdy to Dropbox, you can sync your commands across multiple machines.

## Installation

1. Install gum: `brew install gum`
2. Download Cmdy as `cmdy` to someplace in your path: `curl https://raw.githubusercontent.com/markjaquith/cmdy/main/cmdy > /usr/local/bin/cmdy`
3. Make Cmdy executable: `chmod +x /usr/local/bin/cmdy`

## Usage

- `cmdy` — run a command
- `cmdy create` — create a new command
- `cmdy edit` — edit a command
- `cmdy delete` — delete a command

## License

Cmdy is Copyright 2022, Mark Jaquith and is released under the terms of the MIT license.
