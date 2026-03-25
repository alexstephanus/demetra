use embedded_charts::embedded_graphics::{
    mono_font::{
        ascii::{FONT_7X14, FONT_9X18_BOLD},
        MonoTextStyle,
    },
    pixelcolor::Rgb888,
    primitives::{Line, PrimitiveStyle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
    Drawable, Pixel,
};
use embedded_charts::prelude::*;
// use num_traits::float::Float;
use slint::{Image, Rgb8Pixel, SharedPixelBuffer};

use crate::ui_types::SensorType;

struct SlintDrawTarget {
    buffer: SharedPixelBuffer<Rgb8Pixel>,
}

impl SlintDrawTarget {
    fn new(width: u32, height: u32) -> Self {
        Self {
            buffer: SharedPixelBuffer::new(width, height),
        }
    }

    fn into_image(self) -> Image {
        Image::from_rgb8(self.buffer)
    }
}

impl DrawTarget for SlintDrawTarget {
    type Color = Rgb888;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let width = self.buffer.width() as i32;
        let height = self.buffer.height() as i32;
        let stride = width as usize * 3;
        let bytes = self.buffer.make_mut_bytes();

        for Pixel(point, color) in pixels {
            if point.x >= 0 && point.x < width && point.y >= 0 && point.y < height {
                let offset = point.y as usize * stride + point.x as usize * 3;
                bytes[offset] = color.r();
                bytes[offset + 1] = color.g();
                bytes[offset + 2] = color.b();
            }
        }
        Ok(())
    }
}

impl OriginDimensions for SlintDrawTarget {
    fn size(&self) -> Size {
        Size::new(self.buffer.width(), self.buffer.height())
    }
}

fn sensor_line_color(sensor_type: SensorType) -> Rgb888 {
    match sensor_type {
        SensorType::Ph => Rgb888::new(0x4F, 0xC3, 0xF7),
        SensorType::Conductivity => Rgb888::new(0x81, 0xC7, 0x84),
        SensorType::Orp => Rgb888::new(0xFF, 0xB7, 0x4D),
        SensorType::Temperature => Rgb888::new(0xEF, 0x53, 0x50),
    }
}

fn sensor_label(sensor_type: SensorType) -> &'static str {
    match sensor_type {
        SensorType::Ph => "pH",
        SensorType::Conductivity => "EC",
        SensorType::Orp => "ORP",
        SensorType::Temperature => "Temp",
    }
}

const MAX_CHART_POINTS: usize = 256;

fn format_hours(hours: f32, buf: &mut [u8]) -> &str {
    if hours >= 1.0 {
        let h = (hours + 0.5) as u32;
        let len = write_u32(buf, h);
        buf[len] = b'h';
        core::str::from_utf8(&buf[..len + 1]).unwrap_or("")
    } else {
        let m = (hours * 60.0 + 0.5) as u32;
        let len = write_u32(buf, m);
        buf[len] = b'm';
        core::str::from_utf8(&buf[..len + 1]).unwrap_or("")
    }
}

fn format_value(val: f32, buf: &mut [u8]) -> &str {
    let negative = val < 0.0;
    let abs_val = if negative { -val } else { val };
    let rounded = (abs_val * 10.0 + 0.5) as u32;
    let int_part = rounded / 10;
    let frac_part = rounded % 10;

    let mut pos = 0;
    if negative {
        buf[pos] = b'-';
        pos += 1;
    }
    let int_len = write_u32(&mut buf[pos..], int_part);
    pos += int_len;

    if frac_part != 0 {
        buf[pos] = b'.';
        pos += 1;
        buf[pos] = b'0' + frac_part as u8;
        pos += 1;
    }

    core::str::from_utf8(&buf[..pos]).unwrap_or("")
}

fn write_u32(buf: &mut [u8], mut val: u32) -> usize {
    if val == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut tmp = [0u8; 10];
    let mut i = 0;
    while val > 0 {
        tmp[i] = b'0' + (val % 10) as u8;
        val /= 10;
        i += 1;
    }
    for j in 0..i {
        buf[j] = tmp[i - 1 - j];
    }
    i
}

fn value_to_y(val: f32, y_min: f32, y_max: f32, chart_top: i32, chart_height: i32) -> i32 {
    let normalized = (val - y_min) / (y_max - y_min);
    chart_top + ((1.0 - normalized) * (chart_height - 1) as f32) as i32
}

fn blend_range_band(
    target: &mut SlintDrawTarget,
    top_y: i32,
    bottom_y: i32,
    left_x: i32,
    right_x: i32,
) {
    let buf_width = target.buffer.width() as i32;
    let buf_height = target.buffer.height() as i32;
    let stride = buf_width as usize * 3;
    let bytes = target.buffer.make_mut_bytes();

    let top = top_y.max(0) as usize;
    let bottom = bottom_y.min(buf_height - 1) as usize;
    let left = left_x.max(0) as usize;
    let right = right_x.min(buf_width - 1) as usize;

    for y in top..=bottom {
        for x in left..=right {
            let offset = y * stride + x * 3;
            let r = bytes[offset] as u16;
            let g = bytes[offset + 1] as u16;
            let b = bytes[offset + 2] as u16;
            bytes[offset] = ((r * 2 + 0x81) / 3) as u8;
            bytes[offset + 1] = ((g * 2 + 0xC7) / 3) as u8;
            bytes[offset + 2] = ((b * 2 + 0x84) / 3) as u8;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_y_grid_and_labels(
    target: &mut SlintDrawTarget,
    min_acceptable: f32,
    max_acceptable: f32,
    y_min: f32,
    y_max: f32,
    width: u32,
    height: u32,
    margins: Margins,
) {
    let chart_top = margins.top as i32;
    let chart_bottom = height as i32 - margins.bottom as i32;
    let chart_height = chart_bottom - chart_top;
    let chart_left = margins.left as i32;
    let chart_right = width as i32 - margins.right as i32;

    let half_range = (max_acceptable - min_acceptable) / 2.0;
    let midpoint = (min_acceptable + max_acceptable) / 2.0;

    let tick_values = [
        min_acceptable - half_range,
        min_acceptable,
        midpoint,
        max_acceptable,
        max_acceptable + half_range,
    ];

    let grid_color = Rgb888::new(0xCC, 0xCC, 0xCC);
    let grid_style = PrimitiveStyle::with_stroke(grid_color, 1);

    for &val in &tick_values {
        let y = value_to_y(val, y_min, y_max, chart_top, chart_height);
        if y >= chart_top && y <= chart_bottom {
            let _ = Line::new(Point::new(chart_left, y), Point::new(chart_right, y))
                .into_styled(grid_style)
                .draw(target);
        }
    }

    let text_style = MonoTextStyle::new(&FONT_7X14, Rgb888::BLACK);
    let right_aligned = TextStyleBuilder::new()
        .alignment(Alignment::Right)
        .baseline(Baseline::Middle)
        .build();

    let label_x = chart_left - 3;

    for &val in &[min_acceptable, max_acceptable] {
        let y = value_to_y(val, y_min, y_max, chart_top, chart_height);
        if y >= chart_top && y <= chart_bottom {
            let mut buf = [0u8; 16];
            let label = format_value(val, &mut buf);
            let _ = Text::with_text_style(label, Point::new(label_x, y), text_style, right_aligned)
                .draw(target);
        }
    }
}

fn draw_x_grid(target: &mut SlintDrawTarget, width: u32, height: u32, margins: Margins) {
    let chart_top = margins.top as i32;
    let chart_bottom = height as i32 - margins.bottom as i32;
    let chart_left = margins.left as i32;
    let chart_right = width as i32 - margins.right as i32;
    let chart_width = chart_right - chart_left;

    let grid_color = Rgb888::new(0xCC, 0xCC, 0xCC);
    let grid_style = PrimitiveStyle::with_stroke(grid_color, 1);

    for i in 1..=3 {
        let x = chart_left + (chart_width * i) / 4;
        let _ = Line::new(Point::new(x, chart_top), Point::new(x, chart_bottom))
            .into_styled(grid_style)
            .draw(target);
    }
}

fn draw_time_labels(
    target: &mut SlintDrawTarget,
    total_hours: f32,
    width: u32,
    height: u32,
    margins: Margins,
) {
    let text_style = MonoTextStyle::new(&FONT_7X14, Rgb888::BLACK);

    let chart_left = margins.left as i32;
    let chart_right = width as i32 - margins.right as i32;
    let chart_bottom = height as i32 - margins.bottom as i32;
    let label_y = chart_bottom + 12;

    let left_aligned = TextStyleBuilder::new()
        .alignment(Alignment::Left)
        .baseline(Baseline::Bottom)
        .build();
    let center_aligned = TextStyleBuilder::new()
        .alignment(Alignment::Center)
        .baseline(Baseline::Bottom)
        .build();
    let right_aligned = TextStyleBuilder::new()
        .alignment(Alignment::Right)
        .baseline(Baseline::Bottom)
        .build();

    let chart_width = chart_right - chart_left;

    let mut buf0 = [0u8; 16];
    let left_label = format_hours(total_hours, &mut buf0);
    let _ = Text::with_text_style(
        left_label,
        Point::new(chart_left, label_y),
        text_style,
        left_aligned,
    )
    .draw(target);

    let mut buf1 = [0u8; 16];
    let q1_label = format_hours(total_hours * 3.0 / 4.0, &mut buf1);
    let q1_x = chart_left + chart_width / 4;
    let _ = Text::with_text_style(
        q1_label,
        Point::new(q1_x, label_y),
        text_style,
        center_aligned,
    )
    .draw(target);

    let mut buf2 = [0u8; 16];
    let mid_label = format_hours(total_hours / 2.0, &mut buf2);
    let mid_x = chart_left + chart_width / 2;
    let _ = Text::with_text_style(
        mid_label,
        Point::new(mid_x, label_y),
        text_style,
        center_aligned,
    )
    .draw(target);

    let mut buf3 = [0u8; 16];
    let q3_label = format_hours(total_hours / 4.0, &mut buf3);
    let q3_x = chart_left + chart_width * 3 / 4;
    let _ = Text::with_text_style(
        q3_label,
        Point::new(q3_x, label_y),
        text_style,
        center_aligned,
    )
    .draw(target);

    let _ = Text::with_text_style(
        "Now",
        Point::new(chart_right, label_y),
        text_style,
        right_aligned,
    )
    .draw(target);
}

fn draw_title(target: &mut SlintDrawTarget, sensor_type: SensorType, width: u32, margins: Margins) {
    let text_style = MonoTextStyle::new(&FONT_9X18_BOLD, Rgb888::BLACK);
    let centered = TextStyleBuilder::new()
        .alignment(Alignment::Center)
        .baseline(Baseline::Top)
        .build();

    let label = sensor_label(sensor_type);
    let center_x = (margins.left as i32 + width as i32 - margins.right as i32) / 2;
    let _ =
        Text::with_text_style(label, Point::new(center_x, 2), text_style, centered).draw(target);
}

pub fn render_sensor_chart(
    readings: &[(i64, f32)],
    sensor_type: SensorType,
    min_acceptable: f32,
    max_acceptable: f32,
    width: u32,
    height: u32,
) -> Image {
    let mut target = SlintDrawTarget::new(width, height);

    if readings.len() < 2 {
        return target.into_image();
    }

    let first_ts = readings[0].0 as f32;

    let step = if readings.len() > MAX_CHART_POINTS {
        readings.len() / MAX_CHART_POINTS
    } else {
        1
    };

    let mut data: StaticDataSeries<Point2D, MAX_CHART_POINTS> = StaticDataSeries::new();
    for reading in readings.iter().step_by(step) {
        let x = (reading.0 as f32 - first_ts) / 3600.0;
        let y = reading.1;
        let _ = data.push(Point2D::new(x, y));
    }

    if data.len() < 2 {
        return target.into_image();
    }

    let bounds = match data.bounds() {
        Ok(b) => b,
        Err(_) => return target.into_image(),
    };

    let half_range = (max_acceptable - min_acceptable) / 2.0;
    let y_min = min_acceptable - half_range;
    let y_max = max_acceptable + half_range;

    let line_color = sensor_line_color(sensor_type);
    let margins = Margins::new(22, 15, 40, 45);

    let x_axis = match LinearAxisBuilder::new(AxisOrientation::Horizontal, AxisPosition::Bottom)
        .range(bounds.min_x, bounds.max_x)
        .tick_count(0)
        .show_grid(false)
        .show_labels(false)
        .show_ticks(false)
        .professional_style()
        .build()
    {
        Ok(axis) => axis,
        Err(_) => return target.into_image(),
    };

    let y_axis = match LinearAxisBuilder::new(AxisOrientation::Vertical, AxisPosition::Left)
        .range(y_min, y_max)
        .tick_count(0)
        .show_grid(false)
        .show_labels(false)
        .show_ticks(false)
        .professional_style()
        .build()
    {
        Ok(axis) => axis,
        Err(_) => return target.into_image(),
    };

    let chart = match LineChart::builder()
        .line_color(line_color)
        .line_width(2)
        .background_color(Rgb888::WHITE)
        .margins(margins)
        .with_x_axis(x_axis)
        .with_y_axis(y_axis)
        .build()
    {
        Ok(c) => c,
        Err(_) => return target.into_image(),
    };

    let viewport = Rectangle::new(Point::zero(), Size::new(width, height));
    let _ = chart.draw(&data, chart.config(), viewport, &mut target);

    let chart_top = margins.top as i32;
    let chart_bottom = height as i32 - margins.bottom as i32;
    let chart_height = chart_bottom - chart_top;
    let chart_left = margins.left as i32;
    let chart_right = width as i32 - margins.right as i32;

    let band_top = value_to_y(max_acceptable, y_min, y_max, chart_top, chart_height);
    let band_bottom = value_to_y(min_acceptable, y_min, y_max, chart_top, chart_height);
    blend_range_band(&mut target, band_top, band_bottom, chart_left, chart_right);

    draw_x_grid(&mut target, width, height, margins);
    draw_y_grid_and_labels(
        &mut target,
        min_acceptable,
        max_acceptable,
        y_min,
        y_max,
        width,
        height,
        margins,
    );
    draw_time_labels(&mut target, bounds.max_x, width, height, margins);
    draw_title(&mut target, sensor_type, width, margins);

    target.into_image()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec::Vec;

    #[test]
    fn test_render_empty_readings_returns_image() {
        let image = render_sensor_chart(&[], SensorType::Ph, 5.5, 7.5, 320, 240);
        assert_eq!(image.size().width, 320);
        assert_eq!(image.size().height, 240);
    }

    #[test]
    fn test_render_single_reading_returns_image() {
        let readings = vec![(1000i64, 7.0f32)];
        let image = render_sensor_chart(&readings, SensorType::Ph, 5.5, 7.5, 320, 240);
        assert_eq!(image.size().width, 320);
    }

    #[test]
    fn test_render_chart_with_data() {
        let readings: Vec<(i64, f32)> = (0..100)
            .map(|i| (i * 360, 6.5 + (i as f32 * 0.01)))
            .collect();
        let image = render_sensor_chart(&readings, SensorType::Ph, 5.5, 7.5, 320, 240);
        assert_eq!(image.size().width, 320);
        assert_eq!(image.size().height, 240);
    }

    #[test]
    fn test_render_chart_all_sensor_types() {
        let test_cases = [
            (SensorType::Ph, 5.5, 7.5),
            (SensorType::Conductivity, 800.0, 1800.0),
            (SensorType::Orp, 400.0, 800.0),
            (SensorType::Temperature, 18.0, 28.0),
        ];
        let readings: Vec<(i64, f32)> = (0..50)
            .map(|i| (i * 600, 25.0 + (i as f32 * 0.1)))
            .collect();

        for (sensor_type, min_acc, max_acc) in test_cases {
            let image = render_sensor_chart(&readings, sensor_type, min_acc, max_acc, 480, 320);
            assert_eq!(image.size().width, 480);
        }
    }

    #[test]
    fn test_render_chart_constant_values() {
        let readings: Vec<(i64, f32)> = (0..20).map(|i| (i * 3600, 7.0)).collect();
        let image = render_sensor_chart(&readings, SensorType::Ph, 5.5, 7.5, 320, 240);
        assert_eq!(image.size().width, 320);
    }

    #[test]
    fn test_render_chart_downsamples_large_dataset() {
        let readings: Vec<(i64, f32)> = (0..2000)
            .map(|i| (i * 60, 7.0 + (i as f32 * 0.001)))
            .collect();
        let image = render_sensor_chart(&readings, SensorType::Ph, 5.5, 7.5, 320, 240);
        assert_eq!(image.size().width, 320);
    }

    #[test]
    fn test_format_value_integer() {
        let mut buf = [0u8; 16];
        assert_eq!(format_value(800.0, &mut buf), "800");
    }

    #[test]
    fn test_format_value_decimal() {
        let mut buf = [0u8; 16];
        assert_eq!(format_value(5.5, &mut buf), "5.5");
    }

    #[test]
    fn test_format_value_negative() {
        let mut buf = [0u8; 16];
        assert_eq!(format_value(-2.5, &mut buf), "-2.5");
    }

    #[test]
    fn test_value_to_y_maps_correctly() {
        let top = value_to_y(10.0, 0.0, 10.0, 0, 100);
        assert_eq!(top, 0);
        let bottom = value_to_y(0.0, 0.0, 10.0, 0, 100);
        assert_eq!(bottom, 99);
        let mid = value_to_y(5.0, 0.0, 10.0, 0, 100);
        assert_eq!(mid, 49);
    }
}
