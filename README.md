# Postfix_Ratelimit

A Milter for Postfix that limits the number of emails sent from a user within a specified time frame.

## Features

- Limits the number of emails sent per user
- Configurable time frame and email limit
- Supports serving multiple Postfix/Sendmail servers

## Installation

1. Install the binary
    ```bash
    cargo install postfix_ratelimit
    cp ~/.cargo/bin/postfix_ratelimit /usr/local/bin/postfix_ratelimit
    ```

2. Create a configuration file at `/etc/postfix_ratelimit.conf` or `/usr/local/etc/postfix_ratelimit.conf` with the following content:
    ```toml
    # Please change the paths and values as needed
    db_file = "/path/to/your/postfix_ratelimit.db"
    log_file = "/path/to/your/postfix_ratelimit.log" # Optional but recommended
    interval = 60          # Time window in minutes
    limit = 20             # Max emails allowed in the time window
    ```
  > Please see the [Configuration](#Configuration) section for all available options.

3. See the Postfix [documentation](https://www.postfix.org/MILTER_README.html#config) for setting up the Milter in Postfix.
  
## Usage

You can now run the Milter with the following command:

```bash
postfix_ratelimit
```
> You can also create a service to run it in the background.

### Signals

You can send different signals to the program to control it:

- SIGUSR1 (10) prints the currently loaded configuation values to the console
- SIGUSR2 (12) resets all rate limits by clearing the database
- SIGHUP (1) restarts the program to reload the configuration file or save the database
- Termination Signals (2, 3, 15) save the database and stop the program

## Configuration

You can configure options like this:

```
option = value
```

### Options

| Option            | Type    | Default                  | Description                                                                                                                        |
|-------------------|---------|--------------------------|------------------------------------------------------------------------------------------------------------------------------------|
| db_file           | String  | (none, required)         | Path to the SQLite database file used for storing rate limit data. **This option must be set manually.**                            |
| socket            | String  | "inet:127.0.0.1:11847"   | Address on which the milter will listen, specified as either "inet:IP:PORT" for a TCP socket or "unix:/path/to/socket" for a Unix socket. |
| interval          | u64     | 60                       | Time window for rate limiting, specified in minutes.                                                                               |
| limit             | u64     | 20                       | Maximum number of emails allowed to be sent within each interval.                                                                  |
| max_recipients    | u64     | 20                       | Maximum number of recipients allowed per individual email message. 0 for no limit.                                                 |
| count_recipients  | bool    | true                     | If true, each recipient counts separately towards the rate limit, causing the limit to be reached faster with emails sent to multiple recipients. |
| per_host          | bool    | false                    | If true, rate limiting is tracked separately per sender and per connecting host; if false, only the sender's email address is considered. |
| use_sasl          | bool    | false                    | Enables rate limiting based on the SASL user. This requires the server to provide the {auth_authen} macro.                         |
| clean_interval    | u64     | 120                      | Frequency, in minutes, at which expired entries are removed from the database. Does not affect ratelimiting.                       |
| log_file          | String  | (none, optional)         | File path to write logs. Leave empty for no logging to file.                                                                       |
| debug             | bool    | false                    | Enables Debug mode which prints extra messages to the terminal.                                                                    |
| reject_error      | bool    | false                    | Rejects emails that encountered some kind of issue during processing like the sender missing. False by default.                     |

### CLI Options

--config="<PATH>": Specify an configuration file path.
--socket="<SOCKET>": Specify the socket to listen on. Same format as the config file.
--debug: Enable debug mode.
--help: Show help message.
