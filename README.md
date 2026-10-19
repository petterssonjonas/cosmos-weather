# Cosmos Weather

A COSMIC desktop application with saved locations, current-hour forecasts, hourly and daily views, a temperature graph and forecast details. This is a normal application, not a panel applet.

## Requirements

Runtime: a COSMIC desktop, graphical Wayland session, session D-Bus and HTTPS access for city searches and forecasts. No account, API key or Python process is required.

Build/install: current stable Rust from [rustup](https://rustup.rs), `just`, Bash, GNU coreutils/findutils, Git and native development packages. Distribution Rust can be too old. Weather also needs the adjacent `cosmos-applet-datetime-plus` source repository to build. The installed executable runs independently.

These package examples do not mean all distributions were tested. See [libcosmic dependencies](https://github.com/pop-os/libcosmic#dependencies).

Fedora:

```sh
sudo dnf install git just gcc-c++ cmake pkgconf-pkg-config desktop-file-utils \
  expat-devel freetype-devel fontconfig-devel wayland-devel libxkbcommon-devel \
  libxkbcommon-x11-devel libinput-devel mesa-libEGL-devel libX11-devel \
  libXcursor-devel libXi-devel libXrandr-devel dbus-devel openssl-devel
```

Debian 13 or newer:

```sh
sudo apt update
sudo apt install git just build-essential cmake pkgconf desktop-file-utils \
  libexpat1-dev libfreetype-dev libfontconfig-dev libwayland-dev libxkbcommon-dev \
  libxkbcommon-x11-dev libinput-dev libegl1-mesa-dev libx11-dev libxcursor-dev \
  libxi-dev libxrandr-dev libdbus-1-dev libssl-dev
```

Arch Linux:

```sh
sudo pacman -S --needed git just base-devel cmake pkgconf desktop-file-utils \
  expat freetype2 fontconfig wayland libxkbcommon libxkbcommon-x11 libinput \
  mesa libglvnd libx11 libxcursor libxi libxrandr dbus openssl
```

These are build/install packages, not additional Weather runtime programs. Rust tests use `just test`; `just check` also needs the rustup Clippy component. Shared weather-library tests belong to the datetime repository.

## Build and install

Install rustup from its linked site first. Run as your desktop user. Clone both repositories into the same parent directory:

```sh
mkdir -p ~/Projects
cd ~/Projects
git clone https://github.com/petterssonjonas/cosmos-applet-datetime-plus
git clone https://github.com/petterssonjonas/cosmos-weather
cd cosmos-weather
rustup toolchain install stable
rustup override set stable
just build-release
just install
```

The build uses `../cosmos-applet-datetime-plus` as a local path dependency. Do not move or rename that source directory before building. You do not need to install the datetime applet to use Weather.

Use `sudo` only for system package prerequisites, never for `just build-release` or `just install`. Files go to `~/.local/bin` and `~/.local/share`. Use your mapped desktop user in build containers, not root.

Launch **Cosmos Weather** from the desktop launcher, or run:

```sh
~/.local/bin/cosmos-weather
```

If necessary, reopen the launcher or log in again. No panel enable step is needed. Installation does not change panel configuration or tiling settings.

## Use and limits

Search for a city explicitly, select a result, then choose Hourly or Daily. Settings provides Celsius/Fahrenheit and provider information. Up to 64 locations can be saved. There is no automatic or IP-based location detection.

The current value is a forecast for the current hour, not a station observation. Missing values stay unavailable. Graphs stop at gaps; partial and sparse daily coverage is identified. Times use the selected location's timezone. Feels-like values use NWS wind-chill or heat-index calculations only within their supported ranges. Provider errors and stale data remain visible.

The datetime applet can open Weather with its selected city and unit. An existing Weather window receives that selection instead of opening a duplicate. Weather preferences do not change applet preferences.

## Privacy and storage

City searches send the query and connection IP address to [Open-Meteo Geocoding / GeoNames](https://open-meteo.com/en/docs/geocoding-api). Forecast requests send selected coordinates, truncated to four decimal places, and connection IP address to [MET Norway](https://api.met.no/weatherapi/locationforecast/2.0/documentation) over HTTPS. No telemetry or authentication tokens are used.

Automatic checks run every 15 minutes. Weather and the datetime applet share a durable provider gate across processes and locations. Network requests wait at least 15 minutes, or longer for server expiry and retry limits. Manual Refresh cannot bypass this gate. A newly selected uncached city can require a wait.

- Private preferences: `~/.config/cosmos-weather/settings.json`.
- Shared forecast cache and provider request state: `~/.cache/cosmos-applet-datetime-plus/`.

Standard XDG variables override these defaults. Invalid or unsupported preference files are not silently overwritten.

## Migration and removal

Stop old Weather, the old datetime applet and all reminder windows before migration. Keep both generations stopped: their provider locks use separate names.

Run `just migrate` here to copy old `cosmic-weather` preferences and the shared `cosmic-applet-datetime-plus` cache into their new `cosmos-*` namespaces. If the datetime applet already migrated the cache, run `just migrate false` here. If Weather migrated first, use `just migrate false` in `cosmos-applet-datetime-plus` afterward.

Sources remain backups. Existing destinations, unsafe paths and non-regular files are refused. Each directory copy is atomic, but the copies are not one combined transaction. Resolve conflicts before retrying. There is no automatic migration or legacy executable alias. Use the old version's uninstaller, if available, to remove obsolete registrations. Keep data backups until the new applications work correctly.

Run `just uninstall` to remove this version. Preferences and shared caches remain.

## Credits and license

Independent project; not an official System76 or Yr application. Code: GPL-3.0-only; see `LICENSE`. Native conventions follow the [COSMIC app template](https://github.com/pop-os/cosmic-app-template).

Forecast data: MET Norway, [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). Adaptations include sample selection and grouping, condition icons, graphing, rounding, unit conversion and calculated feels-like values. City lookup: Open-Meteo / GeoNames. Provider attribution and links also appear in Settings.
