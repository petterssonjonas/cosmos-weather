use crate::{
    cli::{Launch, Parsed},
    forecast::{self, Day, PAGE_SIZE},
    settings::{Preferences, Store, Unit},
};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use cosmic::{
    Core,
    app::Task,
    iced::{self, Alignment, Length},
    prelude::*,
    widget,
};
use cosmic_applet_datetime_plus::weather::{self, Client, Forecast, ForecastHour, WeatherLocation};
use std::{ops::Range, sync::Arc, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForecastView {
    Hourly,
    Daily,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Forecast,
    Locations,
    Settings,
    HourDetail(usize),
    DayDetail(usize),
}

#[derive(Debug, Clone)]
pub enum Message {
    Refresh,
    Tick,
    ForecastReady(u64, Result<Arc<Forecast>, String>),
    Query(String),
    Search,
    SearchReady(u64, Result<Vec<WeatherLocation>, String>),
    Choose(WeatherLocation, bool),
    Remove(usize),
    SaveLocation,
    Locations,
    Settings,
    Back,
    Unit(Unit),
    View(ForecastView),
    Previous,
    Next,
    Hour(usize),
    Day(usize),
    Open(&'static str),
    Opened(Result<(), String>),
}

pub struct App {
    core: Core,
    client: Result<Arc<Client>, String>,
    preferences: Preferences,
    store: Store,
    persistence_error: Option<String>,
    forecast: Option<Arc<Forecast>>,
    forecast_error: Option<String>,
    busy: bool,
    generation: u64,
    screen: Screen,
    view: ForecastView,
    page: usize,
    hourly: Range<usize>,
    days: Vec<Day>,
    chart: Option<forecast::Chart>,
    now: DateTime<Utc>,
    query: String,
    search_results: Vec<WeatherLocation>,
    search_error: Option<String>,
    search_busy: bool,
    search_generation: u64,
    link_error: Option<String>,
}

impl cosmic::Application for App {
    type Executor = cosmic::executor::Default;
    type Flags = Launch;
    type Message = Message;
    const APP_ID: &'static str = "io.github.CosmicWeather";

    fn core(&self) -> &Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, launch: Launch) -> (Self, Task<Message>) {
        let (store, mut preferences, persistence_error) = Store::load();
        let has_overrides = launch.location.is_some() || launch.unit.is_some();
        preferences.apply_launch(launch);
        let mut app = Self {
            core,
            client: Client::new().map(Arc::new),
            preferences,
            store,
            persistence_error,
            forecast: None,
            forecast_error: None,
            busy: false,
            generation: 0,
            screen: Screen::Forecast,
            view: ForecastView::Hourly,
            page: 0,
            hourly: 0..0,
            days: Vec::new(),
            chart: None,
            now: Utc::now(),
            query: String::new(),
            search_results: Vec::new(),
            search_error: None,
            search_busy: false,
            search_generation: 0,
            link_error: None,
        };
        app.set_header_title("Weather".into());
        if has_overrides {
            app.persist();
        }
        let title = app
            .core
            .main_window_id()
            .map_or_else(Task::none, |id| app.set_window_title("Weather".into(), id));
        let refresh = app.refresh();
        (app, Task::batch([title, refresh]))
    }

    fn header_start(&self) -> Vec<Element<'_, Message>> {
        vec![
            widget::button::text(if self.busy {
                "Refreshing…"
            } else {
                "Refresh"
            })
            .leading_icon(widget::icon::from_name("view-refresh-symbolic"))
            .on_press_maybe(
                (!self.busy && self.preferences.selected.is_some()).then_some(Message::Refresh),
            )
            .tooltip("Check the shared cache; provider cooldowns are always respected")
            .into(),
        ]
    }

    fn header_center(&self) -> Vec<Element<'_, Message>> {
        if self.screen != Screen::Forecast {
            return Vec::new();
        }
        [ForecastView::Hourly, ForecastView::Daily]
            .into_iter()
            .map(|view| {
                let label = match view {
                    ForecastView::Hourly => "Hourly",
                    ForecastView::Daily => "Daily",
                };
                let button = if self.view == view {
                    widget::button::suggested(label)
                } else {
                    widget::button::text(label)
                };
                // The centered header measures twice; stable widths preserve label layout.
                button.width(96).on_press(Message::View(view)).into()
            })
            .collect()
    }

    fn header_end(&self) -> Vec<Element<'_, Message>> {
        vec![
            widget::button::text("Settings")
                .leading_icon(widget::icon::from_name("preferences-system-symbolic"))
                .on_press(Message::Settings)
                .into(),
        ]
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        iced::time::every(Duration::from_secs(15 * 60)).map(|_| Message::Tick)
    }

    fn dbus_activation(&mut self, message: cosmic::dbus_activation::Message) -> Task<Message> {
        let mut tasks = Vec::new();
        if let cosmic::dbus_activation::Details::ActivateAction { action, args } = message.msg {
            if action == "launch" {
                match Launch::parse(args) {
                    Ok(Parsed::Launch(launch)) => {
                        let location_changed = launch
                            .location
                            .as_ref()
                            .is_some_and(|l| self.preferences.selected.as_ref() != Some(l));
                        let changed = launch.location.is_some() || launch.unit.is_some();
                        self.preferences.apply_launch(launch);
                        self.screen = Screen::Forecast;
                        if changed {
                            self.persist();
                        }
                        if location_changed {
                            self.reset_location();
                            tasks.push(self.refresh());
                        }
                    }
                    Ok(Parsed::Help) => {
                        self.forecast_error = Some("--help is not an activation action.".into())
                    }
                    Err(error) => self.forecast_error = Some(error),
                }
            } else {
                self.forecast_error = Some(format!("Unknown activation action: {action}"));
            }
        }
        if let Some(id) = self.core.main_window_id() {
            tasks.push(iced::window::gain_focus(id));
        }
        Task::batch(tasks)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Refresh | Message::Tick => {
                self.now = Utc::now();
                self.rebuild_forecast();
                return self.refresh();
            }
            Message::ForecastReady(generation, result) => {
                if generation != self.generation {
                    return Task::none();
                }
                self.busy = false;
                self.now = Utc::now();
                match result {
                    Ok(forecast) => {
                        self.forecast = Some(forecast);
                        self.forecast_error = None;
                    }
                    Err(error) => self.forecast_error = Some(error),
                }
                self.rebuild_forecast();
            }
            Message::Query(query) => {
                self.query = query;
                self.search_generation = self.search_generation.wrapping_add(1);
                self.search_busy = false;
                self.search_error = None;
                self.search_results.clear();
            }
            Message::Search => {
                if self.search_busy {
                    return Task::none();
                }
                let query = self.query.trim().to_owned();
                if query.is_empty() {
                    self.search_error = Some("Enter a city name to search.".into());
                    return Task::none();
                }
                let client = match &self.client {
                    Ok(client) => Arc::clone(client),
                    Err(error) => {
                        self.search_error = Some(error.clone());
                        return Task::none();
                    }
                };
                self.search_generation = self.search_generation.wrapping_add(1);
                let generation = self.search_generation;
                self.search_busy = true;
                self.search_error = None;
                return Task::perform(async move { client.search(&query).await }, move |result| {
                    cosmic::Action::App(Message::SearchReady(generation, result))
                });
            }
            Message::SearchReady(generation, result) => {
                if generation != self.search_generation {
                    return Task::none();
                }
                self.search_busy = false;
                match result {
                    Ok(locations) => {
                        self.search_results = locations;
                        if self.search_results.is_empty() {
                            self.search_error =
                                Some("No locations found. Try another city name.".into());
                        }
                    }
                    Err(error) => self.search_error = Some(error),
                }
            }
            Message::Choose(location, save) => {
                if let Err(error) = location.validate() {
                    self.search_error = Some(error);
                    return Task::none();
                }
                if save && !self.preferences.locations.contains(&location) {
                    if self.preferences.locations.len() >= 64 {
                        self.search_error = Some(
                            "Remove a saved location before saving another (limit 64).".into(),
                        );
                        return Task::none();
                    }
                    self.preferences.locations.push(location.clone());
                }
                let changed = self.preferences.selected.as_ref() != Some(&location);
                self.preferences.selected = Some(location);
                self.screen = Screen::Forecast;
                self.persist();
                if changed {
                    self.reset_location();
                }
                return self.refresh();
            }
            Message::SaveLocation => {
                if let Some(location) = &self.preferences.selected
                    && !self.preferences.locations.contains(location)
                {
                    if self.preferences.locations.len() >= 64 {
                        self.persistence_error = Some(
                            "Remove a saved location before saving another (limit 64).".into(),
                        );
                    } else {
                        self.preferences.locations.push(location.clone());
                        self.persist();
                    }
                }
            }
            Message::Remove(index) => {
                if index < self.preferences.locations.len() {
                    let removed = self.preferences.locations.remove(index);
                    if self.preferences.selected.as_ref() == Some(&removed) {
                        self.preferences.selected = self.preferences.locations.first().cloned();
                        self.reset_location();
                        self.persist();
                        return self.refresh();
                    }
                    self.persist();
                }
            }
            Message::Locations => self.screen = Screen::Locations,
            Message::Settings => self.screen = Screen::Settings,
            Message::Back => self.screen = Screen::Forecast,
            Message::Unit(unit) => {
                self.preferences.unit = unit;
                self.persist();
            }
            Message::View(view) => {
                self.view = view;
                self.page = 0;
                self.rebuild_chart();
            }
            Message::Previous => {
                self.page = self.page.saturating_sub(1);
                self.rebuild_chart();
            }
            Message::Next => {
                if self.page + 1 < self.page_count() {
                    self.page += 1;
                }
                self.rebuild_chart();
            }
            Message::Hour(index) => self.screen = Screen::HourDetail(index),
            Message::Day(index) => self.screen = Screen::DayDetail(index),
            Message::Open(url) => {
                return Task::perform(
                    async move {
                        let status = tokio::process::Command::new("xdg-open")
                            .arg(url)
                            .status()
                            .await
                            .map_err(|e| e.to_string())?;
                        if status.success() {
                            Ok(())
                        } else {
                            Err(format!("Could not open the link (exit {status})."))
                        }
                    },
                    |result| cosmic::Action::App(Message::Opened(result)),
                );
            }
            Message::Opened(result) => self.link_error = result.err(),
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let content = match self.screen {
            Screen::Forecast => self.forecast_view(),
            Screen::Locations => self.locations_view(),
            Screen::Settings => self.settings_view(),
            Screen::HourDetail(index) => self.hour_detail(index),
            Screen::DayDetail(index) => self.day_detail(index),
        };
        let mut body = widget::column::with_capacity(3).spacing(8);
        if let Some(error) = &self.persistence_error {
            body = body.push(widget::text::body(error));
        }
        body = body.push(content);
        widget::container(body)
            .padding(16)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

impl App {
    fn persist(&mut self) {
        self.persistence_error = self.store.save(&self.preferences).err();
    }

    fn reset_location(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.search_generation = self.search_generation.wrapping_add(1);
        self.search_busy = false;
        self.busy = false;
        self.forecast = None;
        self.forecast_error = None;
        self.page = 0;
        self.hourly = 0..0;
        self.days.clear();
        self.chart = None;
        if matches!(self.screen, Screen::HourDetail(_) | Screen::DayDetail(_)) {
            self.screen = Screen::Forecast;
        }
    }

    fn refresh(&mut self) -> Task<Message> {
        if self.busy {
            return Task::none();
        }
        let Some(location) = self.preferences.selected.clone() else {
            return Task::none();
        };
        let client = match &self.client {
            Ok(client) => Arc::clone(client),
            Err(error) => {
                self.forecast_error = Some(error.clone());
                return Task::none();
            }
        };
        self.busy = true;
        let generation = self.generation;
        Task::perform(
            async move { client.forecast(&location).await.map(Arc::new) },
            move |result| cosmic::Action::App(Message::ForecastReady(generation, result)),
        )
    }

    fn timezone(&self) -> Tz {
        self.preferences
            .selected
            .as_ref()
            .and_then(|l| l.timezone.parse().ok())
            .unwrap_or(chrono_tz::UTC)
    }

    fn rebuild_forecast(&mut self) {
        if let Some(data) = &self.forecast {
            self.hourly = forecast::hourly_range(&data.hours, self.now);
            self.days = forecast::days(&data.hours, self.timezone(), self.now);
        }
        self.page = self.page.min(self.page_count().saturating_sub(1));
        // A refresh can change sample indexes; never leave a detail attached to another timestamp.
        if matches!(self.screen, Screen::HourDetail(_) | Screen::DayDetail(_)) {
            self.screen = Screen::Forecast;
        }
        self.rebuild_chart();
    }

    fn page_count(&self) -> usize {
        let count = match self.view {
            ForecastView::Hourly => self.hourly.len(),
            ForecastView::Daily => self.days.len(),
        };
        count.div_ceil(PAGE_SIZE)
    }

    fn page_range(&self) -> Range<usize> {
        let start = self.page * PAGE_SIZE;
        let total = match self.view {
            ForecastView::Hourly => self.hourly.len(),
            ForecastView::Daily => self.days.len(),
        };
        start.min(total)..(start + PAGE_SIZE).min(total)
    }

    fn rebuild_chart(&mut self) {
        self.chart = if self.view == ForecastView::Hourly {
            self.forecast.as_ref().map(|data| {
                let page = self.page_range();
                forecast::chart(
                    &data.hours[self.hourly.start + page.start..self.hourly.start + page.end],
                )
            })
        } else {
            None
        };
    }

    fn forecast_view(&self) -> Element<'_, Message> {
        let mut body = widget::column::with_capacity(8)
            .spacing(12)
            .width(Length::Fill);
        let Some(location) = &self.preferences.selected else {
            return widget::column::with_capacity(5).spacing(16)
                .push(widget::text::title1("Choose a location"))
                .push(widget::text::body("Search for a city to see current conditions and forecasts. Weather never discovers your location automatically."))
                .push(widget::button::suggested("Choose location").on_press(Message::Locations))
                .push(widget::text::caption("Saved forecasts remain available offline when the shared cache contains them."))
                .into();
        };
        let mut heading = widget::column::with_capacity(4).spacing(6).push(
            widget::button::text(&location.name)
                .trailing_icon(widget::icon::from_name("pan-down-symbolic"))
                .on_press(Message::Locations),
        );
        let current = self.forecast.as_ref().and_then(|data| {
            forecast::current_index(&data.hours, self.now).map(|i| &data.hours[i])
        });
        if let Some(hour) = current {
            let feels = hour
                .apparent_temperature_c()
                .map(|t| {
                    format!(
                        "Calculated feels like {}",
                        self.preferences.unit.temperature(t)
                    )
                })
                .unwrap_or_else(|| "Calculated feels like unavailable".into());
            heading = heading.push(
                widget::row::with_capacity(3)
                    .spacing(12)
                    .align_y(Alignment::Center)
                    .push(
                        widget::text(self.preferences.unit.temperature(hour.temperature_c))
                            .size(42),
                    )
                    .push(
                        widget::column::with_capacity(2)
                            .spacing(4)
                            .push(widget::text::body(weather::description(&hour.symbol)))
                            .push(widget::text::caption(feels)),
                    ),
            );
        } else {
            heading = heading.push(widget::text::body(if self.busy {
                "Checking forecast and shared cache…"
            } else {
                "No current hourly sample is available."
            }));
        }
        let icon = current.map_or("io.github.CosmicWeather-symbolic", |h| {
            weather::icon_name(&h.symbol)
        });
        let mut hero = widget::row::with_capacity(4)
            .spacing(16)
            .align_y(Alignment::Center)
            .push(widget::icon::from_name(icon).size(72).icon())
            .push(heading)
            .push(widget::space::horizontal());
        if !self.preferences.locations.contains(location) {
            hero = hero.push(widget::button::text("Save location").on_press(Message::SaveLocation));
        }
        body = body.push(hero);
        if let Some(error) = &self.forecast_error {
            body = body.push(widget::text::body(format!("Forecast unavailable: {error}")));
        }
        if let Some(data) = &self.forecast {
            if data.stale || self.now > data.valid_until {
                body = body.push(widget::text::body("Stale cached forecast — not current weather. Refresh respects the provider cooldown."));
            }
            if let Some(warning) = &data.warning {
                body = body.push(widget::text::caption(warning));
            }
            body = body.push(match self.view {
                ForecastView::Hourly => self.hourly_view(data),
                ForecastView::Daily => self.daily_view(data),
            });
            let timezone = self.timezone();
            body = body.push(widget::text::caption(format!(
                "Retrieved {} · Provider updated {} · {}",
                data.fetched_at
                    .with_timezone(&timezone)
                    .format("%d %b %H:%M %Z"),
                data.generated_at
                    .with_timezone(&timezone)
                    .format("%d %b %H:%M %Z"),
                location.timezone
            )));
        } else if !self.busy {
            body = body.push(widget::text::body("No cached forecast is available for this location. Check your connection, then refresh."));
        }
        body.push(
            widget::button::text("Weather data: Norwegian Meteorological Institute (MET Norway)")
                .on_press(Message::Open("https://www.met.no/en")),
        )
        .push_maybe(self.link_error.as_ref().map(widget::text::caption))
        .into()
    }

    fn page_controls(&self, label: String) -> Element<'_, Message> {
        widget::row::with_capacity(5)
            .spacing(8)
            .align_y(Alignment::Center)
            .push(
                widget::button::text("Previous")
                    .leading_icon(widget::icon::from_name("go-previous-symbolic"))
                    .on_press_maybe((self.page > 0).then_some(Message::Previous)),
            )
            .push(widget::space::horizontal())
            .push(widget::text::caption(label))
            .push(widget::space::horizontal())
            .push(
                widget::button::text("Next")
                    .trailing_icon(widget::icon::from_name("go-next-symbolic"))
                    .on_press_maybe((self.page + 1 < self.page_count()).then_some(Message::Next)),
            )
            .into()
    }

    fn hourly_view(&self, data: &Forecast) -> Element<'_, Message> {
        if self.hourly.is_empty() {
            return widget::text::body("No upcoming hourly samples remain in this forecast. Daily view shows any remaining provider samples.").into();
        }
        let timezone = self.timezone();
        let page = self.page_range();
        let count = page.len();
        let mut slots = widget::row::with_capacity(count).width(Length::Fill);
        let mut temperatures = widget::row::with_capacity(count).width(Length::Fill);
        for offset in page {
            let index = self.hourly.start + offset;
            let hour = &data.hours[index];
            let time = hour.time.with_timezone(&timezone);
            let title = time.format("%H:%M").to_string();
            let date = time.format("%d %b %Z").to_string();
            let description = weather::description(&hour.symbol);
            let temperature = self.preferences.unit.temperature(hour.temperature_c);
            let slot = widget::column::with_capacity(3)
                .spacing(6)
                .align_x(Alignment::Center)
                .width(Length::Fill)
                .push(widget::text::body(title.clone()))
                .push(widget::text::caption(date.clone()))
                .push(
                    widget::icon::from_name(weather::icon_name(&hour.symbol))
                        .size(28)
                        .icon(),
                );
            slots = slots.push(
                widget::button::custom(slot)
                    .name(format!(
                        "{date} {title}, {temperature}, {description}; show details"
                    ))
                    .class(cosmic::theme::Button::Text)
                    .padding(8)
                    .width(Length::Fill)
                    .on_press(Message::Hour(index)),
            );
            temperatures = temperatures.push(
                widget::container(widget::text::body(temperature))
                    .width(Length::Fill)
                    .align_x(Alignment::Center),
            );
        }
        let mut chart = widget::column::with_capacity(3).spacing(4).push(slots);
        if let Some(curve) = &self.chart {
            chart = chart.push(iced::widget::canvas(curve).width(Length::Fill).height(110));
        }
        chart = chart.push(temperatures);
        let last = data.hours[self.hourly.end - 1]
            .time
            .with_timezone(&timezone);
        widget::column::with_capacity(3)
            .spacing(8)
            .push(
                widget::container(chart)
                    .padding(8)
                    .width(Length::Fill)
                    .class(cosmic::theme::Container::Card),
            )
            .push(self.page_controls(format!(
                "Page {} of {} · Hourly through {}",
                self.page + 1,
                self.page_count(),
                last.format("%a %d %b %H:%M %Z")
            )))
            .into()
    }

    fn daily_view(&self, data: &Forecast) -> Element<'_, Message> {
        if self.days.is_empty() {
            return widget::text::body("No upcoming daily forecast samples are available.").into();
        }
        let mut slots = widget::row::with_capacity(PAGE_SIZE).width(Length::Fill);
        for index in self.page_range() {
            let day = &self.days[index];
            let representative = &data.hours[day.representative];
            let content = widget::column::with_capacity(8)
                .spacing(12)
                .align_x(Alignment::Center)
                .width(Length::Fill)
                .push(widget::text::body(day.date.format("%a").to_string()))
                .push(widget::text::caption(day.date.format("%d %b").to_string()))
                .push(
                    widget::icon::from_name(weather::icon_name(&representative.symbol))
                        .size(32)
                        .icon(),
                )
                .push(widget::text::body(
                    self.preferences.unit.temperature(day.high),
                ))
                .push(widget::text::body(
                    self.preferences.unit.temperature(day.low),
                ))
                .push(widget::text::caption(if day.complete {
                    "Hourly samples"
                } else {
                    "Partial samples"
                }))
                .push(widget::text::caption("Details"));
            slots = slots.push(
                widget::button::custom(content)
                    .padding(12)
                    .width(Length::Fill)
                    .class(cosmic::theme::Button::Text)
                    .name(format!(
                        "{}, sampled high {}, low {}, {}; show daily details",
                        day.date.format("%A %d %B"),
                        self.preferences.unit.temperature(day.high),
                        self.preferences.unit.temperature(day.low),
                        if day.complete {
                            "full hourly coverage"
                        } else {
                            "partial or sparse coverage"
                        }
                    ))
                    .on_press(Message::Day(index)),
            );
        }
        widget::column::with_capacity(4).spacing(8)
            .push(widget::container(slots).padding(8).width(Length::Fill).class(cosmic::theme::Container::Card))
            .push(self.page_controls(format!("Page {} of {} · Sample-derived high / low", self.page + 1, self.page_count())))
            .push(widget::text::caption("Ranges use available forecast samples, not observed or guaranteed daily extrema. Icons show the sample nearest local noon."))
            .into()
    }

    fn locations_view(&self) -> Element<'_, Message> {
        let mut content = widget::column::with_capacity(8).spacing(12)
            .push(widget::button::text("Back to forecast").on_press(Message::Back))
            .push(widget::text::title2("Locations"))
            .push(widget::row::with_capacity(2).spacing(8)
                .push(widget::text_input::search_input("Search city", &self.query)
                    .label("City name").on_input(Message::Query).on_submit(|_| Message::Search)
                    .on_clear(Message::Query(String::new())))
                .push(widget::button::suggested(if self.search_busy { "Searching…" } else { "Search" })
                    .on_press_maybe((!self.search_busy).then_some(Message::Search))))
            .push(widget::text::caption("Search runs only when submitted. Results are saved when selected. No automatic location discovery."));
        if let Some(error) = &self.search_error {
            content = content.push(widget::text::body(error));
        }
        let mut list = widget::column::with_capacity(
            self.search_results.len() + self.preferences.locations.len() + 4,
        )
        .spacing(8);
        if !self.search_results.is_empty() {
            list = list.push(widget::text::heading("Search results"));
            for location in &self.search_results {
                list = list.push(
                    widget::button::text(format!(
                        "{} · {} ({:.2}, {:.2})",
                        location.name, location.timezone, location.latitude, location.longitude
                    ))
                    .on_press(Message::Choose(location.clone(), true))
                    .width(Length::Fill),
                );
            }
        }
        list = list.push(widget::text::heading("Saved locations"));
        if self.preferences.locations.is_empty() {
            list = list.push(widget::text::body("No saved locations yet."));
        }
        for (index, location) in self.preferences.locations.iter().enumerate() {
            list = list.push(
                widget::row::with_capacity(2)
                    .spacing(8)
                    .push(
                        widget::button::text(format!(
                            "{} · {}{}",
                            location.name,
                            location.timezone,
                            if self.preferences.selected.as_ref() == Some(location) {
                                " · Selected"
                            } else {
                                ""
                            }
                        ))
                        .on_press(Message::Choose(location.clone(), false))
                        .width(Length::Fill),
                    )
                    .push(
                        widget::button::destructive(format!("Remove {}", location.name))
                            .on_press(Message::Remove(index)),
                    ),
            );
        }
        content
            .push(widget::scrollable(list).height(Length::Fill))
            .height(Length::Fill)
            .into()
    }

    fn settings_view(&self) -> Element<'_, Message> {
        let mut content = widget::column::with_capacity(12)
            .spacing(16)
            .push(widget::button::text("Back to forecast").on_press(Message::Back))
            .push(widget::text::title2("Settings & About"))
            .push(widget::text::heading("Temperature unit"));
        let units = [Unit::Celsius, Unit::Fahrenheit].into_iter().map(|unit| {
            let label = if unit == Unit::Celsius {
                "Celsius (°C)"
            } else {
                "Fahrenheit (°F)"
            };
            let button = if self.preferences.unit == unit {
                widget::button::suggested(label)
            } else {
                widget::button::standard(label)
            };
            Element::from(button.on_press(Message::Unit(unit)))
        });
        content = content.push(widget::row::with_children(units).spacing(8))
            .push(widget::button::text("Manage locations").on_press(Message::Locations))
            .push(widget::text::heading("Weather"))
            .push(widget::text::body(format!("Version {} · GPL-3.0-only\nNative COSMIC application. Forecasts and city lookup use the shared datetime-plus weather client and cache.", env!("CARGO_PKG_VERSION"))))
            .push(widget::text::body("Forecasts: MET Norway Locationforecast (CC BY 4.0). City search: Open-Meteo Geocoding, based on GeoNames. Calculated feels-like uses NWS wind-chill or heat-index formulas only when their inputs and applicability requirements are met."))
            .push(widget::text::caption("Automatic refresh: every 15 minutes. Manual refresh, activation and city changes never bypass provider cache, Expires or Retry-After. Stored preferences contain only selected/saved locations and temperature unit."))
            .push(widget::row::with_capacity(3).spacing(8)
                .push(widget::button::text("MET Norway").on_press(Message::Open("https://www.met.no/en")))
                .push(widget::button::text("Open-Meteo").on_press(Message::Open("https://open-meteo.com/en/docs/geocoding-api")))
                .push(widget::button::text("COSMIC").on_press(Message::Open("https://system76.com/cosmic"))));
        if let Some(error) = &self.link_error {
            content = content.push(widget::text::body(error));
        }
        widget::scrollable(content).height(Length::Fill).into()
    }

    fn sample_detail(&self, hour: &ForecastHour) -> Element<'_, Message> {
        let timezone = self.timezone();
        let feels = hour
            .apparent_temperature_c()
            .map(|t| self.preferences.unit.temperature(t))
            .unwrap_or_else(|| {
                "Unavailable: missing inputs or outside formula applicability".into()
            });
        let rain = hour
            .precipitation_mm
            .map(|p| format!("{p:.1} mm in the following hour"))
            .unwrap_or_else(|| "Unavailable (no one-hour precipitation sample)".into());
        let wind = hour
            .wind_speed_mps
            .map(|w| format!("{w:.1} m/s"))
            .unwrap_or_else(|| "Unavailable".into());
        let humidity = hour
            .relative_humidity_percent
            .map(|h| format!("{h:.0}%"))
            .unwrap_or_else(|| "Unavailable".into());
        widget::column::with_capacity(7)
            .spacing(12)
            .push(widget::text::heading(
                hour.time
                    .with_timezone(&timezone)
                    .format("%A %d %B %Y, %H:%M %Z (UTC%:z)")
                    .to_string(),
            ))
            .push(
                widget::row::with_capacity(3)
                    .spacing(16)
                    .align_y(Alignment::Center)
                    .push(
                        widget::icon::from_name(weather::icon_name(&hour.symbol))
                            .size(48)
                            .icon(),
                    )
                    .push(widget::text::title2(
                        self.preferences.unit.temperature(hour.temperature_c),
                    ))
                    .push(widget::text::body(weather::description(&hour.symbol))),
            )
            .push(widget::text::body(format!(
                "Calculated feels like: {feels}"
            )))
            .push(widget::text::body(format!("Precipitation: {rain}")))
            .push(widget::text::body(format!(
                "Wind: {wind} · Relative humidity: {humidity}"
            )))
            .into()
    }

    fn hour_detail(&self, index: usize) -> Element<'_, Message> {
        let mut content = widget::column::with_capacity(4)
            .spacing(16)
            .push(widget::button::text("Back to forecast").on_press(Message::Back))
            .push(widget::text::title2("Hourly forecast details"));
        if let Some(hour) = self.forecast.as_ref().and_then(|f| f.hours.get(index)) {
            content = content.push(self.sample_detail(hour));
        } else {
            content = content.push(widget::text::body(
                "This sample is no longer available. Return to the forecast.",
            ));
        }
        widget::scrollable(content).height(Length::Fill).into()
    }

    fn day_detail(&self, index: usize) -> Element<'_, Message> {
        let mut content = widget::column::with_capacity(4)
            .spacing(16)
            .push(widget::button::text("Back to forecast").on_press(Message::Back));
        if let Some(day) = self.days.get(index).zip(self.forecast.as_ref()) {
            let (day, data) = day;
            content = content.push(widget::text::title2(day.date.format("%A %d %B %Y").to_string()))
                .push(widget::text::body(format!("Sample-derived high {} · low {} · {} samples · {}",
                    self.preferences.unit.temperature(day.high), self.preferences.unit.temperature(day.low), day.samples.len(),
                    if day.complete { "Complete hourly coverage" } else { "Partial or sparse local-day coverage" })))
                .push(widget::text::caption("Each entry below is an actual provider sample. Gaps are not interpolated. Conditions are forecasts, not observations."));
            for &sample in &day.samples {
                content = content.push(self.sample_detail(&data.hours[sample]));
            }
        } else {
            content = content.push(widget::text::body(
                "This day is no longer available. Return to the forecast.",
            ));
        }
        widget::scrollable(content).height(Length::Fill).into()
    }
}
