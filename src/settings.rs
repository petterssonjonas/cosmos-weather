use crate::cli::Launch;
use cosmic_applet_datetime_plus::weather::WeatherLocation;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    #[default]
    Celsius,
    Fahrenheit,
}

impl Unit {
    pub fn temperature(self, celsius: f64) -> String {
        let value = match self {
            Self::Celsius => celsius,
            Self::Fahrenheit => celsius * 1.8 + 32.0,
        };
        format!("{value:.0}°{}", self.label())
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Celsius => "C",
            Self::Fahrenheit => "F",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    version: u8,
    pub locations: Vec<WeatherLocation>,
    pub selected: Option<WeatherLocation>,
    pub unit: Unit,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            locations: Vec::new(),
            selected: None,
            unit: Unit::Celsius,
        }
    }
}

impl Preferences {
    fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.locations.len() > 64 {
            return Err("Unsupported preferences version or too many saved locations.".into());
        }
        for location in &self.locations {
            location.validate()?;
        }
        if let Some(location) = &self.selected {
            location.validate()?;
        }
        Ok(())
    }

    pub fn apply_launch(&mut self, launch: Launch) {
        if let Some(unit) = launch.unit {
            self.unit = unit;
        }
        if let Some(location) = launch.location {
            self.selected = Some(location);
        }
    }
}

pub struct Store {
    path: Option<PathBuf>,
    blocked: Option<String>,
}

impl Store {
    pub fn load() -> (Self, Preferences, Option<String>) {
        let path = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map(|p| p.join("cosmic-weather/settings.json"));
        Self::load_path(path)
    }

    fn load_path(path: Option<PathBuf>) -> (Self, Preferences, Option<String>) {
        let result = match &path {
            Some(path) if path.is_absolute() => match fs::read(path) {
                Ok(bytes) => serde_json::from_slice::<Preferences>(&bytes)
                    .map_err(|e| format!("Cannot read preferences: {e}"))
                    .and_then(|p| {
                        p.validate()?;
                        Ok(p)
                    }),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Preferences::default()),
                Err(e) => Err(format!("Cannot read preferences: {e}")),
            },
            _ => Err("No absolute configuration directory is available.".into()),
        };
        match result {
            Ok(preferences) => (
                Self {
                    path,
                    blocked: None,
                },
                preferences,
                None,
            ),
            Err(error) => {
                let error = format!(
                    "{error} Changes are session-only; the original preferences will not be overwritten."
                );
                (
                    Self {
                        path,
                        blocked: Some(error.clone()),
                    },
                    Preferences::default(),
                    Some(error),
                )
            }
        }
    }

    pub fn save(&self, preferences: &Preferences) -> Result<(), String> {
        if let Some(error) = &self.blocked {
            return Err(error.clone());
        }
        preferences.validate()?;
        let path = self
            .path
            .as_ref()
            .ok_or("No configuration path is available.")?;
        let directory = path.parent().ok_or("Invalid configuration path.")?;
        let bytes = serde_json::to_vec_pretty(preferences).map_err(|e| e.to_string())?;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(directory)
            .map_err(|e| format!("Cannot create preferences directory: {e}"))?;
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let temporary = directory.join(format!(
            ".settings-{}-{}.tmp",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temporary)
            .map_err(|e| format!("Cannot create atomic preferences file: {e}. Changes remain active for this session."))?;
        let result = (|| -> std::io::Result<()> {
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, path)?;
            fs::File::open(directory)?.sync_all()
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result.map_err(|e| {
            format!("Preferences could not be saved: {e}. Changes remain active for this session.")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_only_overrides_explicit_values() {
        let location = WeatherLocation {
            name: "Oslo".into(),
            latitude: 59.91,
            longitude: 10.75,
            timezone: "Europe/Oslo".into(),
        };
        let mut preferences = Preferences {
            selected: Some(location.clone()),
            unit: Unit::Fahrenheit,
            ..Preferences::default()
        };
        let crate::cli::Parsed::Launch(launch) =
            Launch::parse(vec!["--unit".into(), "celsius".into()]).unwrap()
        else {
            panic!()
        };
        preferences.apply_launch(launch);
        assert_eq!(
            (preferences.selected, preferences.unit),
            (Some(location), Unit::Celsius)
        );
    }

    #[test]
    fn corrupted_preferences_remain_untouched() {
        let path =
            std::env::temp_dir().join(format!("cosmic-weather-corrupt-{}", std::process::id()));
        fs::write(&path, b"not json").unwrap();
        let (store, preferences, error) = Store::load_path(Some(path.clone()));
        assert!(error.is_some());
        assert!(store.save(&preferences).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"not json");
        fs::remove_file(path).unwrap();
    }
}
