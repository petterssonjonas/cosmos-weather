# Weather for COSMIC

A native COSMIC weather application with a GNOME Weather-style layout: location selector, current-hour forecast, Hourly/Daily views, a horizontally paginated temperature graph, and forecast details. Display name: **Weather**. Application ID: `io.github.CosmicWeather`.

Forecasts come from [MET Norway Locationforecast](https://api.met.no/weatherapi/locationforecast/2.0/documentation), the meteorological service behind Yr. This is not an official Yr application and does not use Yr branding.

## Build and install

Keep `cosmic-weather` and `cosmic-applet-datetime-plus` beside one another. Weather imports the latter repository's public weather library, rather than maintaining a second HTTP client or cache implementation. Both repositories are needed to build; the installed Weather executable runs independently.

Requires a COSMIC session, a current Rust toolchain, `just`, `desktop-file-utils`, and the native Wayland/font/graphics development dependencies of [libcosmic](https://github.com/pop-os/libcosmic). Built on Fedora 44 with Rust 1.98.1.

```sh
just
just install
```

The default installation prefix is `~/.local`. `CARGO_TARGET_DIR` is respected. Launch **Weather** from the application launcher, run `cosmic-weather`, or click the weather card in **Date & Notifications**.

The app requests a 1040×660 window with a 920×620 minimum; COSMIC's tiling layout can assign a different size. Weather does not change tiling settings or force itself to float. Forecast pages and the native canvas graph adapt to the actual window width without stretching or clipping an SVG. Location lists, settings and detailed sample lists scroll when necessary.

## Locations, units and applet integration

- Choose a location explicitly; there is no automatic or IP-based location discovery.
- City search runs only when submitted. Select a result to save and display it.
- Use the location selector to switch or remove saved locations. Up to 64 locations can be saved.
- Settings offers Celsius/Fahrenheit and provider information. Removing a selected saved location selects another saved location, or returns to location selection.
- Clicking the applet's weather card passes its selected location and temperature unit. A running Weather instance receives the new selection instead of creating another window.
- Weather's own saved locations and unit preferences are separate from applet preferences. Changes in Weather do not silently rewrite applet settings.

The launch interface also supports explicit handoff:

```sh
cosmic-weather --location-json '{"name":"Oslo","latitude":59.91,"longitude":10.75,"timezone":"Europe/Oslo"}' --unit celsius
cosmic-weather --help
```

Invalid or duplicate arguments exit with status 2 before loading preferences or opening a window. No-argument launches use saved preferences.

## Forecast semantics

- The current display is a **forecast for the current hour**, not a weather-station observation. No suitable current sample means unavailable, not a fabricated temperature.
- Hourly pages contain actual consecutive one-hour provider samples. The graph uses straight segments and stops at gaps; it does not invent hourly data in the later, lower-resolution forecast horizon.
- Daily high/low values are derived from available samples in the location's timezone. Partial and sparse days are identified; they are not presented as complete-day extrema. The daily icon comes from the sample nearest local noon, not an invented all-day summary.
- Dates and times use the location's IANA timezone, including DST. Detail views show actual timestamps, wind, humidity and one-hour precipitation when supplied.
- Calculated feels-like uses [NWS wind chill](https://www.weather.gov/safety/cold-wind-chill-chart) or [NWS heat index](https://www.wpc.ncep.noaa.gov/html/heatindex_equation.shtml), only within their supported input domains. Missing measurements or unsupported conditions remain unavailable. Actual temperature is not substituted for a missing feels-like estimate.
- Retrieved/provider-update timestamps, stale caches and provider errors remain visible. Cached data is not relabelled as fresh after a failed request.

## Refresh, storage and privacy

Automatic checks run every **15 minutes**. Manual refresh, reopening, switching cities and applet activation all use the same durable provider gate as the datetime applet: at least 15 minutes between network requests across processes and locations, plus any later `Expires`, `Retry-After` or error-backoff deadline. An uncached location may therefore need to wait during a cooldown. Refreshing the UI can reuse cached data without another network request.

- Preferences: `$XDG_CONFIG_HOME/cosmic-weather/settings.json`, normally `~/.config/cosmic-weather/settings.json`.
- Shared forecast cache and request gate: `$XDG_CACHE_HOME/cosmic-applet-datetime-plus/`, normally `~/.cache/cosmic-applet-datetime-plus/`.
- Preferences are private, validated and atomically replaced. Unreadable, malformed or unsupported existing preferences are not overwritten; the error remains visible.
- Explicit city searches send the query and connection IP address to [Open-Meteo Geocoding](https://open-meteo.com/en/docs/geocoding-api), based on GeoNames. Forecast requests send selected coordinates, truncated to four decimal places, and the connection IP address to MET Norway over HTTPS.
- No accounts, API keys, authentication tokens or telemetry are used. Both applications identify the shared client as `cosmic-applet-datetime-plus/0.1.0`; no fictional maintainer contact is supplied.

## Verification and removal

```sh
just check
just uninstall
```

Tests cover argument validation, preference preservation, hourly gaps, partial/sparse daily coverage and 23-/25-hour local dates. The shared library's tests live in `cosmic-applet-datetime-plus` and cover provider parsing, optional measurements, feels-like calculations, durable caching, cross-process locking and rate limits. Tests do not contact real weather providers.

Uninstall removes the executable and desktop/icon/AppStream registration. Preferences and shared caches remain. Local AppStream metadata intentionally has no invented public homepage or developer identity; `appstreamcli validate` reports that missing publication metadata.

## Credits and license

Application code: GPL-3.0-only; see `LICENSE`. Native application conventions follow the [COSMIC app template](https://github.com/pop-os/cosmic-app-template).

Forecast data: **MET Norway, [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)**. Adaptations include selecting and grouping samples, mapping condition icons, graphing and rounding temperatures, unit conversion and calculated feels-like estimates. City lookup: **Open-Meteo / GeoNames**. Provider attribution and links are also available in Settings.
