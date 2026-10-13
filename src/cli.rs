use crate::settings::Unit;
use cosmic_applet_datetime_plus::weather::WeatherLocation;

pub const HELP: &str = "Weather — native COSMIC weather\n\nUsage: cosmic-weather [--location-json JSON] [--unit celsius|fahrenheit]\n       cosmic-weather --help\n\nNo arguments opens saved preferences. Locations are chosen explicitly; no automatic\nlocation discovery occurs. Existing instances receive the location and unit.\nRefreshes respect the shared provider cache and cooldown.\n";

#[derive(Clone, Debug, Default)]
pub struct Launch {
    pub location: Option<WeatherLocation>,
    pub unit: Option<Unit>,
    action: String,
    args: Vec<String>,
}

#[derive(Debug)]
pub enum Parsed {
    Help,
    Launch(Launch),
}

impl Launch {
    pub fn parse(args: Vec<String>) -> Result<Parsed, String> {
        if args == ["--help"] {
            return Ok(Parsed::Help);
        }
        let mut launch = Self {
            action: "launch".into(),
            ..Self::default()
        };
        let mut values = args.iter();
        while let Some(option) = values.next() {
            match option.as_str() {
                "--location-json" => {
                    if launch.location.is_some() {
                        return Err("--location-json may only be specified once.".into());
                    }
                    let json = values
                        .next()
                        .ok_or("--location-json requires a JSON value.")?;
                    let location: WeatherLocation = serde_json::from_str(json)
                        .map_err(|err| format!("Invalid --location-json: {err}"))?;
                    location.validate()?;
                    launch.location = Some(location);
                }
                "--unit" => {
                    if launch.unit.is_some() {
                        return Err("--unit may only be specified once.".into());
                    }
                    launch.unit = Some(match values.next().map(String::as_str) {
                        Some("celsius") => Unit::Celsius,
                        Some("fahrenheit") => Unit::Fahrenheit,
                        _ => return Err("--unit requires celsius or fahrenheit.".into()),
                    });
                }
                _ => return Err(format!("Unknown argument: {option}. Use --help for usage.")),
            }
        }
        launch.args = args;
        Ok(Parsed::Launch(launch))
    }
}

impl cosmic::app::CosmicFlags for Launch {
    type SubCommand = String;
    type Args = Vec<String>;

    fn action(&self) -> Option<&String> {
        Some(&self.action)
    }
    fn args(&self) -> Vec<&str> {
        self.args.iter().map(String::as_str).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_and_duplicate_arguments_fail_before_loading_preferences() {
        for args in [
            vec!["--unit", "kelvin"],
            vec!["--unit", "celsius", "--unit", "fahrenheit"],
            vec!["--location-json", "{}"],
            vec!["--help", "--unit", "celsius"],
            vec!["--unknown"],
        ] {
            assert!(Launch::parse(args.into_iter().map(str::to_owned).collect()).is_err());
        }
    }
}
