# ts6rpc — Discord Rich Presence for TeamSpeak 6

Shows what you're doing in TeamSpeak 6 on your Discord profile:

- **Server name and address**, e.g. `nein.live | TeamSpeak Server (ts.nein.live)`
- **Current channel** and how many people are in it, e.g. `🔊 Voice #1 (2 of 32)`
- **Time since connecting**
- A **screenshare icon** while you share your screen
- "Not connected" while TeamSpeak is open but you're not on a server

A **tray icon** lets you switch the presence on and off, toggle autostart and open the settings.

ts6rpc is a small standalone program (≈2–3 MB, ≈7 MB RAM) for **Windows** and **Linux** that talks to:

- **TeamSpeak** through its official *Remote Apps* WebSocket API (`ws://127.0.0.1:5899`).
  TeamSpeak 6 addons can only be themes, so this runs alongside the client instead.
- **Discord** through its local RPC socket (Linux) or named pipe (Windows).

## Install

### Windows

Open **PowerShell** and run:

```powershell
irm https://raw.githubusercontent.com/wildesPepega/ts6rpc/main/install.ps1 | iex
```

This installs ts6rpc for your user (no admin rights), adds a Start menu entry, starts it with Windows and runs it.
You can also download `ts6rpc-windows-x86_64.exe` from the [latest release](https://github.com/wildesPepega/ts6rpc/releases/latest) and run it directly.

### Linux

```sh
curl -fsSL https://raw.githubusercontent.com/wildesPepega/ts6rpc/main/install.sh | sh
```

This installs `~/.local/bin/ts6rpc` (no root), an app menu entry and an autostart entry, then starts it.
Works on any x86_64 or aarch64 distribution. Native, Flatpak, Snap and Vesktop Discord are supported.

Options: `| sh -s -- --no-autostart` to skip autostart.

### First start

TeamSpeak asks whether to allow **"Discord Rich Presence"**. Click allow. This only happens once.

ts6rpc retries both connections, so it doesn't matter whether TeamSpeak or Discord starts first.

## Tray icon

Click the icon to open the menu:

| Entry                | What it does |
|----------------------|--------------|
| *(status)*           | What Discord currently shows, or what ts6rpc is waiting for |
| **Show in Discord**  | Switch the Rich Presence on/off. Remembered across restarts. The icon turns grey while off |
| **Start on login**   | Start ts6rpc automatically when you log in |
| **Open config folder** | Opens the folder with `config.json` and `ts6rpc.log` |
| **Quit**             | Exit ts6rpc |

The tray works on Windows and on Linux desktops that support StatusNotifierItem: KDE Plasma, Cinnamon, XFCE, LXQt,
Budgie, MATE, Hyprland/Sway bars (waybar) and others. On **GNOME**, install the
[AppIndicator and KStatusNotifierItem Support](https://extensions.gnome.org/extension/615/appindicator-support/) extension
(preinstalled on Ubuntu). Without a tray ts6rpc just runs in the background.

## Uninstall

**Windows** (PowerShell):

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/wildesPepega/ts6rpc/main/install.ps1))) -Uninstall
```

**Linux:**

```sh
curl -fsSL https://raw.githubusercontent.com/wildesPepega/ts6rpc/main/install.sh | sh -s -- --uninstall
```

Your settings are kept; delete the config folder to remove them too.

## Configuration

The config file is created on first start (tray menu → **Open config folder**). Restart ts6rpc after editing it.

| OS      | Path                              |
|---------|-----------------------------------|
| Linux   | `~/.config/ts6rpc/config.json`    |
| Windows | `%APPDATA%\ts6rpc\config.json`    |

| Key                    | Default            | Description |
|------------------------|--------------------|-------------|
| `discordClientId`      | ts6rpc's app       | Discord application ID (its name is shown as "Playing …") |
| `tray`                 | `true`             | Show the tray icon |
| `teamspeakUrl`         | `ws://127.0.0.1:5899` | Remote Apps endpoint |
| `showWhenDisconnected` | `true`             | Show "Not connected" while TeamSpeak is open but not on a server |
| `showServerAddress`    | `true`             | Show the address next to the server name |
| `showChannelName`      | `true`             | Show the channel name |
| `hidePasswordChannels` | `true`             | Show "In a channel" instead of the name of password-protected channels |
| `showChannelUserCount` | `true`             | Show "(x of y)" people in the channel |
| `timestamp`            | `"server"`         | `"server"`, `"channel"` or `"none"` |
| `largeImage`           | ts6.jpeg           | Asset key or https image URL |
| `smallImages`          | screenshare icon   | Icons for `screenshare`, `away`, `deafened`, `muted` (empty = no icon) |
| `serverAddresses`      | `{}`               | Manual address per server UID, e.g. `{"VoNG…=": "ts.example.com:9987"}` |

### Where does the server address come from?

The Remote Apps API doesn't expose the address you connected with. ts6rpc reads it from
your **TeamSpeak bookmarks** (`settings.db`, read-only), matched by the server's unique ID.
For servers without a bookmark, add them to `serverAddresses` or just bookmark the server.

## Command line

```
ts6rpc [--no-tray]     run (without tray icon)
ts6rpc --version
```

Only one instance runs at a time. Logs go to the terminal and to `ts6rpc.log` in the config folder.
Set `TS6RPC_DEBUG=1` to also log TeamSpeak events and the exact activity sent to Discord.

## Building

```sh
cargo build --release                                     # Linux (glibc)
cargo zigbuild --release --target x86_64-unknown-linux-musl # Linux (static, like the releases)
cargo build --release --target x86_64-pc-windows-gnu      # Windows (needs mingw-w64)
cargo test
```

Releases are built by GitHub Actions when a `v*` tag is pushed.

## License

[MIT](LICENSE)
