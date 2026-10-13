use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use cosmic::iced::{self, Point, Rectangle, mouse, widget::canvas};
use cosmic_applet_datetime_plus::weather::ForecastHour;
use std::{cell::Cell, collections::BTreeMap, ops::Range};

pub const PAGE_SIZE: usize = 8;

#[derive(Debug)]
pub struct Day {
    pub date: NaiveDate,
    pub samples: Vec<usize>,
    pub low: f64,
    pub high: f64,
    pub representative: usize,
    pub complete: bool,
}

pub fn current_index(hours: &[ForecastHour], now: DateTime<Utc>) -> Option<usize> {
    hours
        .iter()
        .rposition(|h| h.time <= now && now - h.time < Duration::hours(1))
}

/// Only the actual consecutive one-hour samples are shown in the hourly chart.
pub fn hourly_range(hours: &[ForecastHour], now: DateTime<Utc>) -> Range<usize> {
    let start = current_index(hours, now)
        .or_else(|| hours.iter().position(|h| h.time >= now))
        .unwrap_or(hours.len());
    if hours
        .get(start)
        .is_some_and(|h| h.time - now >= Duration::hours(1))
    {
        return start..start;
    }
    let mut end = start;
    while end < hours.len() {
        if end > start && hours[end].time - hours[end - 1].time != Duration::hours(1) {
            break;
        }
        end += 1;
    }
    start..end
}

pub fn days(hours: &[ForecastHour], timezone: Tz, now: DateTime<Utc>) -> Vec<Day> {
    let start =
        current_index(hours, now).unwrap_or_else(|| hours.partition_point(|h| h.time < now));
    let mut groups: BTreeMap<NaiveDate, Vec<usize>> = BTreeMap::new();
    for (index, hour) in hours.iter().enumerate().skip(start) {
        groups
            .entry(hour.time.with_timezone(&timezone).date_naive())
            .or_default()
            .push(index);
    }
    groups
        .into_iter()
        .map(|(date, samples)| {
            let low = samples
                .iter()
                .map(|&i| hours[i].temperature_c)
                .fold(f64::INFINITY, f64::min);
            let high = samples
                .iter()
                .map(|&i| hours[i].temperature_c)
                .fold(f64::NEG_INFINITY, f64::max);
            // A midday sample is a representative condition, not an all-day summary.
            let noon = date
                .and_hms_opt(12, 0, 0)
                .and_then(|t| timezone.from_local_datetime(&t).earliest());
            let representative = *samples
                .iter()
                .min_by_key(|&&i| {
                    noon.map_or(0, |noon| {
                        (hours[i].time - noon.with_timezone(&Utc))
                            .num_seconds()
                            .abs()
                    })
                })
                .unwrap_or(&samples[0]);
            let bounds = date
                .and_hms_opt(0, 0, 0)
                .and_then(|t| timezone.from_local_datetime(&t).earliest())
                .zip(
                    date.succ_opt()
                        .and_then(|d| d.and_hms_opt(0, 0, 0))
                        .and_then(|t| timezone.from_local_datetime(&t).earliest()),
                );
            let complete = bounds.is_some_and(|(first, next)| {
                let first = first.with_timezone(&Utc);
                let next = next.with_timezone(&Utc);
                hours[samples[0]].time == first
                    && hours[*samples.last().unwrap_or(&samples[0])].time + Duration::hours(1)
                        == next
                    && samples
                        .windows(2)
                        .all(|pair| hours[pair[1]].time - hours[pair[0]].time == Duration::hours(1))
            });
            Day {
                date,
                samples,
                low,
                high,
                representative,
                complete,
            }
        })
        .collect()
}

pub struct Chart {
    heights: [f32; PAGE_SIZE],
    len: usize,
    cache: canvas::Cache<cosmic::Renderer>,
    color: Cell<Option<iced::Color>>,
}

/// Straight segments connect real samples without inventing intermediate extrema.
pub fn chart(hours: &[ForecastHour]) -> Chart {
    assert!(hours.len() <= PAGE_SIZE);
    let mut chart = Chart {
        heights: [0.0; PAGE_SIZE],
        len: hours.len(),
        cache: canvas::Cache::new(),
        color: Cell::new(None),
    };
    if hours.is_empty() {
        return chart;
    }
    let low = hours
        .iter()
        .map(|h| h.temperature_c)
        .fold(f64::INFINITY, f64::min);
    let high = hours
        .iter()
        .map(|h| h.temperature_c)
        .fold(f64::NEG_INFINITY, f64::max);
    let span = (high - low).max(4.0);
    let middle = (high + low) / 2.0;
    for (height, hour) in chart.heights.iter_mut().zip(hours) {
        *height = (0.5 - (hour.temperature_c - middle) / span * (80.0 / 110.0)) as f32;
    }
    chart
}

impl<Message> canvas::Program<Message, cosmic::Theme, cosmic::Renderer> for Chart {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &cosmic::Renderer,
        theme: &cosmic::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry<cosmic::Renderer>> {
        if self.len == 0 {
            return Vec::new();
        }
        let color: iced::Color = theme.cosmic().accent_color().into();
        if self.color.replace(Some(color)) != Some(color) {
            self.cache.clear();
        }
        let geometry = self.cache.draw(renderer, bounds.size(), |frame| {
            let mut points = [Point::ORIGIN; PAGE_SIZE];
            for (index, point) in points.iter_mut().enumerate().take(self.len) {
                *point = Point::new(
                    (index as f32 + 0.5) * frame.width() / self.len as f32,
                    self.heights[index] * frame.height(),
                );
            }
            let points = &points[..self.len];
            let line = canvas::Path::new(|path| {
                path.move_to(points[0]);
                for &point in &points[1..] {
                    path.line_to(point);
                }
            });
            let area = canvas::Path::new(|path| {
                path.move_to(Point::new(points[0].x, frame.height()));
                for &point in points {
                    path.line_to(point);
                }
                path.line_to(Point::new(points[self.len - 1].x, frame.height()));
                path.close();
            });
            frame.fill(
                &area,
                iced::Color {
                    a: color.a * 0.14,
                    ..color
                },
            );
            frame.stroke(
                &line,
                canvas::Stroke {
                    style: canvas::stroke::Style::Solid(color),
                    width: 2.5,
                    ..canvas::Stroke::default()
                },
            );
            let markers = canvas::Path::new(|path| {
                for &point in points {
                    path.circle(point, 2.0);
                }
            });
            frame.fill(&markers, color);
        });
        vec![geometry]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(time: DateTime<Utc>) -> ForecastHour {
        ForecastHour {
            time,
            temperature_c: 10.0,
            symbol: "cloudy".into(),
            precipitation_mm: None,
            wind_speed_mps: None,
            relative_humidity_percent: None,
        }
    }

    #[test]
    fn hourly_chart_stops_at_provider_gap_without_filling_it() {
        let now = Utc.with_ymd_and_hms(2026, 10, 13, 12, 0, 0).unwrap();
        let hours: Vec<_> = [0, 1, 2, 8]
            .map(|offset| sample(now + Duration::hours(offset)))
            .into();
        assert_eq!(hourly_range(&hours, now), 0..3);
    }

    #[test]
    fn daily_groups_follow_city_dates_and_both_dst_day_lengths() {
        let timezone: Tz = "Europe/Stockholm".parse().unwrap();
        for (month, day, count) in [(3, 29, 23), (10, 25, 25)] {
            let first = timezone
                .with_ymd_and_hms(2026, month, day, 0, 0, 0)
                .unwrap()
                .with_timezone(&Utc);
            let hours: Vec<_> = (0..count)
                .map(|offset| sample(first + Duration::hours(offset)))
                .collect();
            let days = days(&hours, timezone, first);
            assert_eq!(
                (days.len(), days[0].samples.len(), days[0].complete),
                (1, count as usize, true)
            );
        }
    }

    #[test]
    fn partial_and_sparse_days_do_not_claim_full_day_ranges() {
        let first = Utc.with_ymd_and_hms(2026, 10, 13, 12, 0, 0).unwrap();
        let hours = [sample(first), sample(first + Duration::hours(6))];
        assert!(!days(&hours, chrono_tz::UTC, first)[0].complete);
    }

    #[test]
    fn sparse_late_horizon_is_not_presented_as_hourly() {
        let now = Utc.with_ymd_and_hms(2026, 10, 13, 12, 0, 0).unwrap();
        let hours = [
            sample(now + Duration::hours(3)),
            sample(now + Duration::hours(9)),
        ];
        assert!(hourly_range(&hours, now).is_empty());
    }
}
