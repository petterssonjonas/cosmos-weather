mod app;
mod cli;
mod forecast;
mod settings;

fn main() -> std::process::ExitCode {
    let args = match std::env::args_os()
        .skip(1)
        .map(|arg| {
            arg.into_string()
                .map_err(|_| "Arguments must be valid UTF-8.")
        })
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(args) => args,
        Err(error) => {
            eprintln!("cosmos-weather: {error}");
            return std::process::ExitCode::from(2);
        }
    };
    let launch = match cli::Launch::parse(args) {
        Ok(cli::Parsed::Help) => {
            print!("{}", cli::HELP);
            return std::process::ExitCode::SUCCESS;
        }
        Ok(cli::Parsed::Launch(launch)) => launch,
        Err(error) => {
            eprintln!("cosmos-weather: {error}");
            return std::process::ExitCode::from(2);
        }
    };
    let settings = cosmic::app::Settings::default()
        .size(cosmic::iced::Size::new(1040.0, 660.0))
        .size_limits(
            cosmic::iced::Limits::NONE
                .min_width(920.0)
                .min_height(620.0),
        )
        .exit_on_close(true);
    match cosmic::app::run_single_instance::<app::App>(settings, launch) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("cosmos-weather: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
