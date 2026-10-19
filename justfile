import 'recipes/common.just'

name := 'cosmos-weather'
appid := 'dev.jp.CosmosWeather'
profile := 'release'
prefix := env('HOME') / '.local'
rootdir := ''
cargo-target-dir := env('CARGO_TARGET_DIR', 'target')
base-dir := absolute_path(clean(rootdir / prefix))
config-home := if env('XDG_CONFIG_HOME', '') == '' { env('HOME') / '.config' } else { env('XDG_CONFIG_HOME') }
cache-home := if env('XDG_CACHE_HOME', '') == '' { env('HOME') / '.cache' } else { env('XDG_CACHE_HOME') }

[default]
default: build-release

build-debug:
    cargo build --locked --target-dir {{quote(cargo-target-dir)}}

build-release:
    cargo build --release --locked --target-dir {{quote(cargo-target-dir)}}

check:
    cargo clippy --all-targets --locked --target-dir {{quote(cargo-target-dir)}} -- -D warnings

test:
    cargo test --locked --target-dir {{quote(cargo-target-dir)}}

[positional-arguments]
run *args:
    cargo run --profile {{quote(if profile == 'debug' { 'dev' } else { profile })}} --locked --target-dir {{quote(cargo-target-dir)}} -- "$@"

# Build separately. This installs a normal application, not a panel applet.
install:
    @just --justfile {{quote(justfile())}} _install-file {{quote(cargo-target-dir / profile / name)}} {{quote(base-dir / 'bin' / name)}} 0755
    @just --justfile {{quote(justfile())}} _install-desktop resources/app.desktop {{quote(base-dir / 'share/applications' / appid + '.desktop')}} {{quote(absolute_path(prefix / 'bin' / name))}}
    @just --justfile {{quote(justfile())}} _install-file resources/app.metainfo.xml {{quote(base-dir / 'share/metainfo' / appid + '.metainfo.xml')}} 0644
    @just --justfile {{quote(justfile())}} _install-file resources/icons/hicolor/scalable/apps/icon.svg {{quote(base-dir / 'share/icons/hicolor/scalable/apps' / appid + '-symbolic.svg')}} 0644

# Preferences and the shared weather cache remain untouched.
uninstall:
    rm -f -- {{quote(base-dir / 'bin' / name)}} {{quote(base-dir / 'share/applications' / appid + '.desktop')}} {{quote(base-dir / 'share/metainfo' / appid + '.metainfo.xml')}} {{quote(base-dir / 'share/icons/hicolor/scalable/apps' / appid + '-symbolic.svg')}}

# Stop both applications and every datetime reminder window first. Legacy data stays intact.
# Use `just migrate false` if Cosmos Date & Time already migrated the shared cache.
migrate shared-cache='true':
    #!/usr/bin/env bash
    set -euo pipefail
    case {{quote(shared-cache)}} in true|false) ;; *) printf '%s\n' 'shared-cache must be true or false.' >&2; exit 1 ;; esac
    just --justfile {{quote(justfile())}} _migrate-directory {{quote(config-home / 'cosmic-weather')}} {{quote(config-home / name)}}
    if [[ {{quote(shared-cache)}} == true ]]; then
        just --justfile {{quote(justfile())}} _migrate-directory {{quote(cache-home / 'cosmic-applet-datetime-plus')}} {{quote(cache-home / 'cosmos-applet-datetime-plus')}}
    fi
